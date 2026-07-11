use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;

use nebula_common::{EndpointInfo, EndpointStatus, NodeStatus};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::docker_api::{EngineMetricSnapshot, NodeMetricsSnapshot, SharedNodeMetrics};
use crate::engine::{Engine, EngineHandle, EngineProcess};
use crate::gpu::read_gpu_statuses;
use crate::reconcile::{mark_request_failed, replica_key, ReplicaKey, RunningModel};
use crate::util::now_ms;

/// Snapshot for health/scrape outside the `running` lock (C2).
struct ProbeTarget {
    rkey: ReplicaKey,
    model_uid: String,
    replica_id: u32,
    base_url: String,
    engine: Arc<dyn Engine>,
    request_id: Option<String>,
}

fn probe_handle(base_url: &str) -> EngineHandle {
    EngineHandle {
        base_url: base_url.to_string(),
        engine_model: String::new(),
        process: EngineProcess::External,
    }
}

/// Number of consecutive health-check failures before marking endpoint as Unhealthy.
const UNHEALTHY_THRESHOLD: u32 = 3;
/// Number of consecutive failures before attempting a container restart.
const RESTART_THRESHOLD: u32 = 5;
/// Base cooldown after a restart attempt (seconds); multiplied by exponential backoff.
const RESTART_COOLDOWN_SECS: u64 = 30;
/// Max restart attempts inside the budget window.
const RESTART_BUDGET_N: u32 = 5;
/// Budget window (24h).
const RESTART_BUDGET_WINDOW_MS: u64 = 86_400_000;
/// Cap for exponential backoff cooldown.
const RESTART_COOLDOWN_MAX_SECS: u64 = 300;

#[derive(Debug, Default)]
struct RestartBudget {
    window_start_ms: u64,
    attempts: u32,
    next_allowed_ms: u64,
}

impl RestartBudget {
    fn backoff_secs(attempt: u32) -> u64 {
        let exp = 1u64 << attempt.min(8);
        (RESTART_COOLDOWN_SECS.saturating_mul(exp)).min(RESTART_COOLDOWN_MAX_SECS)
    }

    /// Returns Ok(cooldown_secs) if a restart is allowed, Err(reason) if budget exhausted.
    fn try_consume(&mut self, now: u64) -> Result<u64, &'static str> {
        if self.window_start_ms == 0 || now.saturating_sub(self.window_start_ms) > RESTART_BUDGET_WINDOW_MS
        {
            self.window_start_ms = now;
            self.attempts = 0;
            self.next_allowed_ms = 0;
        }
        if now < self.next_allowed_ms {
            return Err("backoff");
        }
        if self.attempts >= RESTART_BUDGET_N {
            return Err("budget_exhausted");
        }
        self.attempts += 1;
        let cooldown = Self::backoff_secs(self.attempts.saturating_sub(1));
        self.next_allowed_ms = now.saturating_add(cooldown.saturating_mul(1000));
        Ok(cooldown)
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn budget_exhausts_after_n() {
        let mut b = RestartBudget::default();
        let mut now = 1_000u64;
        for i in 0..RESTART_BUDGET_N {
            assert!(b.try_consume(now).is_ok(), "attempt {i}");
            now = b.next_allowed_ms;
        }
        assert_eq!(b.try_consume(now), Err("budget_exhausted"));
    }

    #[test]
    fn budget_resets_after_window() {
        let mut b = RestartBudget::default();
        let mut now = 1_000u64;
        for _ in 0..RESTART_BUDGET_N {
            let _ = b.try_consume(now);
            now = b.next_allowed_ms;
        }
        now = b.window_start_ms + RESTART_BUDGET_WINDOW_MS + 1;
        assert!(b.try_consume(now).is_ok());
    }
}

pub async fn register_endpoint(
    store: &EtcdMetaStore,
    info: &EndpointInfo,
    ttl_ms: u64,
    lease_id: Option<i64>,
) -> anyhow::Result<()> {
    let key = format!("/endpoints/{}/{}", info.model_uid, info.replica_id);
    let bytes = serde_json::to_vec(info)?;
    if let Some(lease_id) = lease_id {
        let _ = store.put_with_lease(&key, bytes, lease_id).await?;
    } else {
        let _ = store.put(&key, bytes, Some(ttl_ms)).await?;
    }
    Ok(())
}

/// P0-2: publish latest engine stats for control-plane consumers (Router/Scheduler/Drain).
pub async fn register_stats(
    store: &EtcdMetaStore,
    stats: &nebula_common::EndpointStats,
    ttl_ms: u64,
    lease_id: Option<i64>,
) -> anyhow::Result<()> {
    let key = format!("/stats/{}/{}", stats.model_uid, stats.replica_id);
    let bytes = serde_json::to_vec(stats)?;
    if let Some(lease_id) = lease_id {
        let _ = store.put_with_lease(&key, bytes, lease_id).await?;
    } else {
        let _ = store.put(&key, bytes, Some(ttl_ms)).await?;
    }
    Ok(())
}

pub async fn delete_endpoint(
    store: &EtcdMetaStore,
    model_uid: &str,
    replica_id: u32,
) -> anyhow::Result<()> {
    let key = format!("/endpoints/{}/{}", model_uid, replica_id);
    let _ = store.delete(&key).await?;
    Ok(())
}

pub async fn delete_stats(
    store: &EtcdMetaStore,
    model_uid: &str,
    replica_id: u32,
) -> anyhow::Result<()> {
    let key = format!("/stats/{}/{}", model_uid, replica_id);
    let _ = store.delete(&key).await?;
    Ok(())
}

pub async fn heartbeat_loop(
    store: EtcdMetaStore,
    node_id: String,
    ttl_ms: u64,
    interval_ms: u64,
    api_port: u16,
    endpoint: Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    running: Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    xtrace: Option<xtrace_client::Client>,
    shared_metrics: SharedNodeMetrics,
    lease_id: Option<i64>,
) {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    // Track consecutive health-check failures per (model_uid, replica_id)
    let mut fail_counts: HashMap<ReplicaKey, u32> = HashMap::new();
    // Track last restart timestamp (ms) per replica for cooldown
    let mut restart_at: HashMap<ReplicaKey, u64> = HashMap::new();
    // Per-replica restart budget (24h window)
    let mut restart_budgets: HashMap<ReplicaKey, RestartBudget> = HashMap::new();

    let key = format!("/nodes/{}/status", node_id);
    loop {
        let mut metric_points: Vec<xtrace_client::MetricPoint> = Vec::new();
        let mut engine_snapshots: Vec<EngineMetricSnapshot> = Vec::new();
        let gpus = read_gpu_statuses().await;

        // Collect GPU metrics for xtrace
        if xtrace.is_some() {
            let ts = Utc::now();
            for gpu in &gpus {
                let labels = HashMap::from([
                    ("node_id".to_string(), node_id.clone()),
                    ("gpu_index".to_string(), gpu.index.to_string()),
                ]);
                metric_points.push(xtrace_client::MetricPoint {
                    name: "gpu_memory_used_mb".to_string(),
                    labels: labels.clone(),
                    value: gpu.memory_used_mb as f64,
                    timestamp: ts,
                });
                metric_points.push(xtrace_client::MetricPoint {
                    name: "gpu_memory_total_mb".to_string(),
                    labels: labels.clone(),
                    value: gpu.memory_total_mb as f64,
                    timestamp: ts,
                });
                if let Some(temp) = gpu.temperature_c {
                    metric_points.push(xtrace_client::MetricPoint {
                        name: "gpu_temperature".to_string(),
                        labels: labels.clone(),
                        value: temp as f64,
                        timestamp: ts,
                    });
                }
                if let Some(util) = gpu.utilization_gpu {
                    metric_points.push(xtrace_client::MetricPoint {
                        name: "gpu_utilization".to_string(),
                        labels,
                        value: util as f64,
                        timestamp: ts,
                    });
                }
            }
        }

        let gpus_for_metrics = gpus.clone();
        let status = NodeStatus {
            node_id: node_id.clone(),
            last_heartbeat_ms: now_ms(),
            gpus,
            api_addr: Some(format!("http://0.0.0.0:{}", api_port)),
        };

        let bytes = match serde_json::to_vec(&status) {
            Ok(b) => b,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
                continue;
            }
        };

        if let Err(e) = match lease_id {
            Some(id) => store.put_with_lease(&key, bytes, id).await.map(|_| ()),
            None => store.put(&key, bytes, Some(ttl_ms)).await.map(|_| ()),
        } {
            tracing::warn!(error=%e, "failed to write heartbeat");
        }

        // Refresh endpoint registrations (best-effort; never block heartbeat loop).
        if let Ok(mut guard) = endpoint.try_lock() {
            for info in guard.values_mut() {
                info.last_heartbeat_ms = now_ms();
                if let Err(e) = register_endpoint(&store, info, ttl_ms, lease_id).await {
                    tracing::warn!(error=%e, "failed to refresh endpoint");
                }
            }
        } else {
            tracing::debug!("skipping endpoint refresh: endpoint state lock busy");
        }

        // C2: snapshot under try_lock; health/scrape outside so reconcile is not blocked.
        let now = now_ms();
        let targets: Vec<ProbeTarget> = match running.try_lock() {
            Err(_) => {
                tracing::debug!("skipping engine health/stats: running state lock busy");
                Vec::new()
            }
            Ok(running_guard) => running_guard
                .values()
                .filter_map(|rm| {
                    if rm.failed {
                        return None;
                    }
                    let rkey = replica_key(&rm.model_uid, rm.replica_id);
                    if let Some(&last_restart) = restart_at.get(&rkey) {
                        let budget = restart_budgets.get(&rkey);
                        let cooldown_ms = budget
                            .map(|b| b.next_allowed_ms.saturating_sub(last_restart))
                            .unwrap_or(RESTART_COOLDOWN_SECS.saturating_mul(1000));
                        let elapsed = now.saturating_sub(last_restart);
                        if elapsed < cooldown_ms {
                            tracing::debug!(
                                model_uid=%rm.model_uid,
                                replica_id=rm.replica_id,
                                remaining_ms=cooldown_ms.saturating_sub(elapsed),
                                "skipping health check during restart cooldown"
                            );
                            return None;
                        }
                    }
                    Some(ProbeTarget {
                        rkey,
                        model_uid: rm.model_uid.clone(),
                        replica_id: rm.replica_id,
                        base_url: rm.handle.base_url.clone(),
                        engine: rm.engine.clone(),
                        request_id: rm.request_id.clone(),
                    })
                })
                .collect(),
        };

        for target in targets {
            let probe = probe_handle(&target.base_url);
            let healthy = target.engine.health_check(&probe).await;
            let count = fail_counts.entry(target.rkey.clone()).or_insert(0);

            if healthy {
                if *count > 0 {
                    tracing::info!(
                        model_uid=%target.model_uid,
                        replica_id=target.replica_id,
                        prev_failures=*count,
                        "engine recovered"
                    );
                    *count = 0;
                    restart_at.remove(&target.rkey);
                    let mut ep_guard = endpoint.lock().await;
                    if let Some(info) = ep_guard.get_mut(&target.rkey) {
                        if info.status == EndpointStatus::Unhealthy {
                            info.status = EndpointStatus::Ready;
                            let _ = register_endpoint(&store, info, ttl_ms, lease_id).await;
                            tracing::info!(
                                model_uid=%target.model_uid,
                                replica_id=target.replica_id,
                                "endpoint marked Ready again"
                            );
                        }
                    }
                }

                if let Some(stats) = target
                    .engine
                    .scrape_stats(&http, &probe, &target.model_uid, target.replica_id)
                    .await
                {
                    if xtrace.is_some() {
                        let ts = Utc::now();
                        let labels = HashMap::from([
                            ("node_id".to_string(), node_id.clone()),
                            ("model_uid".to_string(), target.model_uid.clone()),
                            ("replica_id".to_string(), target.replica_id.to_string()),
                        ]);
                        metric_points.push(xtrace_client::MetricPoint {
                            name: "pending_requests".to_string(),
                            labels: labels.clone(),
                            value: stats.pending_requests as f64,
                            timestamp: ts,
                        });
                        if let (Some(used), Some(free)) =
                            (stats.kv_cache_used_bytes, stats.kv_cache_free_bytes)
                        {
                            let total = used + free;
                            let usage = if total > 0 {
                                used as f64 / total as f64
                            } else {
                                0.0
                            };
                            metric_points.push(xtrace_client::MetricPoint {
                                name: "kv_cache_usage".to_string(),
                                labels: labels.clone(),
                                value: usage,
                                timestamp: ts,
                            });
                        }
                        if let Some(rate) = stats.prefix_cache_hit_rate {
                            metric_points.push(xtrace_client::MetricPoint {
                                name: "prefix_cache_hit_rate".to_string(),
                                labels,
                                value: rate,
                                timestamp: ts,
                            });
                        }
                    }

                    let kv_usage = match (stats.kv_cache_used_bytes, stats.kv_cache_free_bytes) {
                        (Some(used), Some(free)) => {
                            let total = used + free;
                            if total > 0 {
                                Some(used as f64 / total as f64)
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    engine_snapshots.push(EngineMetricSnapshot {
                        model_uid: target.model_uid.clone(),
                        replica_id: target.replica_id,
                        pending_requests: stats.pending_requests,
                        kv_cache_usage: kv_usage,
                        prefix_cache_hit_rate: stats.prefix_cache_hit_rate,
                    });

                    if let Err(e) = register_stats(&store, &stats, ttl_ms, lease_id).await {
                        tracing::warn!(
                            model_uid=%target.model_uid,
                            replica_id=target.replica_id,
                            error=%e,
                            "failed to write /stats/ to etcd"
                        );
                    }
                }
                continue;
            }

            *count += 1;
            tracing::warn!(
                model_uid=%target.model_uid,
                replica_id=target.replica_id,
                consecutive_failures=*count,
                "engine health check failed"
            );

            if *count == UNHEALTHY_THRESHOLD {
                let mut ep_guard = endpoint.lock().await;
                if let Some(info) = ep_guard.get_mut(&target.rkey) {
                    info.status = EndpointStatus::Unhealthy;
                    let _ = register_endpoint(&store, info, ttl_ms, lease_id).await;
                    tracing::warn!(
                        model_uid=%target.model_uid,
                        replica_id=target.replica_id,
                        "endpoint marked Unhealthy"
                    );
                }
            }

            if *count < RESTART_THRESHOLD {
                continue;
            }

            let budget = restart_budgets.entry(target.rkey.clone()).or_default();
            match budget.try_consume(now) {
                Ok(cooldown_secs) => {
                    tracing::warn!(
                        model_uid=%target.model_uid,
                        replica_id=target.replica_id,
                        attempt=budget.attempts,
                        cooldown_secs,
                        "attempting engine restart"
                    );
                    // Rare path: brief lock to mutate the real handle.
                    let mut guard = running.lock().await;
                    if let Some(rm) = guard.get_mut(&target.rkey) {
                        if !rm.failed {
                            let ctx = rm.start_ctx.clone();
                            if let Err(e) = rm.engine.try_restart(&mut rm.handle, &ctx).await {
                                tracing::error!(
                                    model_uid=%target.model_uid,
                                    replica_id=target.replica_id,
                                    error=%e,
                                    "engine restart failed"
                                );
                            }
                        }
                    }
                    drop(guard);
                    *count = 1;
                    restart_at.insert(target.rkey.clone(), now_ms());
                }
                Err("backoff") => {
                    tracing::debug!(
                        model_uid=%target.model_uid,
                        replica_id=target.replica_id,
                        "restart skipped: still in backoff"
                    );
                }
                Err(_) => {
                    let reason = format!(
                        "recovery budget exhausted ({} restarts / 24h)",
                        RESTART_BUDGET_N
                    );
                    tracing::error!(
                        model_uid=%target.model_uid,
                        replica_id=target.replica_id,
                        %reason,
                        "marking replica Failed; stopping restart loop"
                    );
                    {
                        let mut guard = running.lock().await;
                        if let Some(rm) = guard.get_mut(&target.rkey) {
                            rm.failed = true;
                        }
                    }
                    let mut ep_guard = endpoint.lock().await;
                    if let Some(info) = ep_guard.get_mut(&target.rkey) {
                        info.status = EndpointStatus::Failed;
                        let _ = register_endpoint(&store, info, ttl_ms, lease_id).await;
                    }
                    if let Some(request_id) = target.request_id.as_deref() {
                        mark_request_failed(&store, request_id, reason).await;
                    }
                }
            }
        }

        // Update shared Prometheus metrics snapshot
        {
            let mut snap = shared_metrics.lock().await;
            *snap = NodeMetricsSnapshot {
                gpus: gpus_for_metrics,
                engines: engine_snapshots,
            };
        }

        // Push metrics to xtrace (non-blocking, best-effort)
        if let Some(ref client) = xtrace {
            if !metric_points.is_empty() {
                let client = client.clone();
                let points = std::mem::take(&mut metric_points);
                tokio::spawn(async move {
                    if let Err(e) = client.push_metrics(&points).await {
                        tracing::debug!(error=%e, "failed to push metrics to xtrace");
                    }
                });
            }
        }

        tokio::time::sleep(Duration::from_millis(interval_ms)).await;
    }
}
