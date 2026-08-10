use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;

use nebula_common::{EndpointInfo, EndpointStatus, EngineAlertType, EngineProbeAlert, NodeStatus};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::docker_api::{
    EngineMetricSnapshot, NodeMetricsSnapshot, ScrapeOutcomeRecord, SharedNodeMetrics,
};
use crate::engine::{container::inspect_docker_container, Engine, EngineHandle, EngineProcess};
use crate::gpu::read_gpu_statuses;
use crate::reconcile::{mark_request_failed, replica_key, ReplicaKey, RunningModel};
use crate::util::now_ms;

/// KV cache usage above this ratio triggers a warning alert (not a hard failure).
const KV_CACHE_HIGH_THRESHOLD: f64 = 0.95;
/// GPU memory used/total above this percentage triggers a warning alert.
const GPU_MEMORY_PRESSURE_PCT: f64 = 98.0;

/// Snapshot for health/scrape outside the `running` lock (C2).
struct ProbeTarget {
    rkey: ReplicaKey,
    model_uid: String,
    replica_id: u32,
    base_url: String,
    engine: Arc<dyn Engine>,
    request_id: Option<String>,
    container_name: Option<String>,
    gpu_indices: Option<Vec<u32>>,
}

/// Outcome of container-level probe before HTTP health check.
enum ContainerProbeOutcome {
    Ok,
    Failed { reason: String, alert_type: EngineAlertType, exit_code: Option<i32> },
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

fn engine_probe_alert_key(node_id: &str, model_uid: &str, replica_id: u32) -> String {
    format!(
        "/alerts/{}/engine_{}_{}",
        node_id, model_uid, replica_id
    )
}

async fn emit_engine_probe_alert(
    store: &EtcdMetaStore,
    alert: &EngineProbeAlert,
    ttl_ms: u64,
    lease_id: Option<i64>,
) -> anyhow::Result<()> {
    let key = engine_probe_alert_key(&alert.node_id, &alert.model_uid, alert.replica_id);
    let bytes = serde_json::to_vec(alert)?;
    if let Some(lease_id) = lease_id {
        let _ = store.put_with_lease(&key, bytes, lease_id).await?;
    } else {
        let _ = store.put(&key, bytes, Some(ttl_ms)).await?;
    }
    Ok(())
}

async fn clear_engine_probe_alert(
    store: &EtcdMetaStore,
    node_id: &str,
    model_uid: &str,
    replica_id: u32,
) -> anyhow::Result<()> {
    let key = engine_probe_alert_key(node_id, model_uid, replica_id);
    let _ = store.delete(&key).await?;
    Ok(())
}

fn docker_container_name(process: &EngineProcess) -> Option<String> {
    match process {
        EngineProcess::DockerContainer { name, .. } => Some(name.clone()),
        _ => None,
    }
}

async fn probe_container(container_name: &str) -> ContainerProbeOutcome {
    let Some(state) = inspect_docker_container(container_name).await else {
        return ContainerProbeOutcome::Failed {
            reason: "docker container not found".into(),
            alert_type: EngineAlertType::ContainerExited,
            exit_code: None,
        };
    };

    if state.oom_killed {
        return ContainerProbeOutcome::Failed {
            reason: format!("container OOM killed (exit={:?})", state.exit_code),
            alert_type: EngineAlertType::OomKilled,
            exit_code: state.exit_code,
        };
    }

    if !state.running {
        return ContainerProbeOutcome::Failed {
            reason: format!(
                "container not running (status={}, exit={:?})",
                state.status, state.exit_code
            ),
            alert_type: EngineAlertType::ContainerExited,
            exit_code: state.exit_code,
        };
    }

    ContainerProbeOutcome::Ok
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

pub async fn register_capability(
    store: &EtcdMetaStore,
    cap: &nebula_common::ReplicaCapability,
    ttl_ms: u64,
    lease_id: Option<i64>,
) -> anyhow::Result<()> {
    let key = format!("/capabilities/{}/{}", cap.model_uid, cap.replica_id);
    let bytes = serde_json::to_vec(cap)?;
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

pub async fn delete_capability(
    store: &EtcdMetaStore,
    model_uid: &str,
    replica_id: u32,
) -> anyhow::Result<()> {
    let key = format!("/capabilities/{}/{}", model_uid, replica_id);
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
    let http = nebula_common::health_http_client().unwrap_or_default();

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
        let mut scrape_outcomes: Vec<ScrapeOutcomeRecord> = Vec::new();
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
        let platform = std::env::var("NEBULA_NODE_PLATFORM")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| Some(nebula_common::DEFAULT_NODE_PLATFORM.to_string()));
        let status = NodeStatus {
            node_id: node_id.clone(),
            last_heartbeat_ms: now_ms(),
            gpus,
            api_addr: Some(format!("http://0.0.0.0:{}", api_port)),
            platform,
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

        // Sync endpoint metadata from live engine handles (e.g. after recreate on new port).
        if let (Ok(mut ep_guard), Ok(running_guard)) = (endpoint.try_lock(), running.try_lock()) {
            for (key, rm) in running_guard.iter() {
                if let Some(info) = ep_guard.get_mut(key) {
                    let live = Some(rm.handle.base_url.clone());
                    if info.base_url != live {
                        tracing::info!(
                            model_uid=%info.model_uid,
                            replica_id=info.replica_id,
                            old=?info.base_url,
                            new=?live,
                            "syncing endpoint base_url from running engine"
                        );
                        info.base_url = live;
                    }
                }
            }
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

        // Refresh capability snapshots alongside endpoints.
        if let Ok(running_guard) = running.try_lock() {
            for rm in running_guard.values() {
                if let Some(cap) = rm.capability.as_ref() {
                    if let Err(e) = register_capability(&store, cap, ttl_ms, lease_id).await {
                        tracing::warn!(error=%e, "failed to refresh capability");
                    }
                }
            }
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
                        container_name: docker_container_name(&rm.handle.process),
                        gpu_indices: rm.start_ctx.gpu_indices.clone(),
                    })
                })
                .collect(),
        };

        for target in targets {
            let probe = probe_handle(&target.base_url);
            let count = fail_counts.entry(target.rkey.clone()).or_insert(0);

            let mut container_failed = false;
            if let Some(cname) = target.container_name.as_deref() {
                match probe_container(cname).await {
                    ContainerProbeOutcome::Ok => {}
                    ContainerProbeOutcome::Failed {
                        reason,
                        alert_type,
                        exit_code,
                    } => {
                        container_failed = true;
                        tracing::error!(
                            model_uid=%target.model_uid,
                            replica_id=target.replica_id,
                            container=%cname,
                            %reason,
                            "container probe failed"
                        );
                        let alert = EngineProbeAlert {
                            node_id: node_id.clone(),
                            model_uid: target.model_uid.clone(),
                            replica_id: target.replica_id,
                            alert_type,
                            message: reason.clone(),
                            exit_code,
                            created_at_ms: now_ms(),
                        };
                        if let Err(e) =
                            emit_engine_probe_alert(&store, &alert, ttl_ms, lease_id).await
                        {
                            tracing::warn!(error=%e, "failed to write engine probe alert");
                        }
                        let mut ep_guard = endpoint.lock().await;
                        if let Some(info) = ep_guard.get_mut(&target.rkey) {
                            info.status = EndpointStatus::Unhealthy;
                            info.status_detail = Some(reason);
                            let _ = register_endpoint(&store, info, ttl_ms, lease_id).await;
                        }
                        *count = RESTART_THRESHOLD;
                    }
                }
            }

            let healthy = if container_failed {
                false
            } else {
                target.engine.health_check(&probe).await
            };

            if healthy {
                let mut probe_alert_emitted = false;

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
                            info.status_detail = None;
                            info.base_url = Some(target.base_url.clone());
                            let _ = register_endpoint(&store, info, ttl_ms, lease_id).await;
                            tracing::info!(
                                model_uid=%target.model_uid,
                                replica_id=target.replica_id,
                                "endpoint marked Ready again"
                            );
                        }
                    }
                }

                if let Some(ref indices) = target.gpu_indices {
                    for &idx in indices {
                        if let Some(gpu) = gpus_for_metrics.iter().find(|g| g.index == idx) {
                            if gpu.memory_total_mb > 0 {
                                let pct = gpu.memory_used_mb as f64 * 100.0
                                    / gpu.memory_total_mb as f64;
                                if pct >= GPU_MEMORY_PRESSURE_PCT {
                                    let msg = format!(
                                        "GPU {idx} memory at {pct:.1}% ({}/{})",
                                        gpu.memory_used_mb, gpu.memory_total_mb
                                    );
                                    tracing::warn!(
                                        model_uid=%target.model_uid,
                                        replica_id=target.replica_id,
                                        %msg,
                                        "GPU memory pressure"
                                    );
                                    let alert = EngineProbeAlert {
                                        node_id: node_id.clone(),
                                        model_uid: target.model_uid.clone(),
                                        replica_id: target.replica_id,
                                        alert_type: EngineAlertType::GpuMemoryPressure,
                                        message: msg,
                                        exit_code: None,
                                        created_at_ms: now_ms(),
                                    };
                                    let _ = emit_engine_probe_alert(
                                        &store, &alert, ttl_ms, lease_id,
                                    )
                                    .await;
                                    probe_alert_emitted = true;
                                }
                            }
                        }
                    }
                }

                match target
                    .engine
                    .scrape_stats(&http, &probe, &target.model_uid, target.replica_id)
                    .await
                {
                    Ok(stats) => {
                        if stats
                            .kv_cache_usage
                            .is_some_and(|u| u >= KV_CACHE_HIGH_THRESHOLD)
                        {
                            let usage = stats.kv_cache_usage.unwrap();
                            let msg = format!("KV cache usage at {usage:.1}%");
                            let alert = EngineProbeAlert {
                                node_id: node_id.clone(),
                                model_uid: target.model_uid.clone(),
                                replica_id: target.replica_id,
                                alert_type: EngineAlertType::KvCacheHigh,
                                message: msg,
                                exit_code: None,
                                created_at_ms: now_ms(),
                            };
                            let _ =
                                emit_engine_probe_alert(&store, &alert, ttl_ms, lease_id).await;
                            probe_alert_emitted = true;
                        }
                        scrape_outcomes.push(ScrapeOutcomeRecord {
                            model_uid: target.model_uid.clone(),
                            replica_id: target.replica_id,
                            engine_type: target.engine.engine_type().to_string(),
                            result: "success".to_string(),
                        });
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
                            if let Some(usage) = stats.kv_cache_usage {
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

                        engine_snapshots.push(EngineMetricSnapshot {
                            model_uid: target.model_uid.clone(),
                            replica_id: target.replica_id,
                            pending_requests: stats.pending_requests,
                            kv_cache_usage: stats.kv_cache_usage,
                            prefix_cache_hit_rate: stats.prefix_cache_hit_rate,
                            container_running: target.container_name.as_ref().map(|_| true),
                            probe_failures: *count,
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
                    Err(err) => {
                        scrape_outcomes.push(ScrapeOutcomeRecord {
                            model_uid: target.model_uid.clone(),
                            replica_id: target.replica_id,
                            engine_type: target.engine.engine_type().to_string(),
                            result: err.as_str().to_string(),
                        });
                    }
                }

                if !probe_alert_emitted {
                    let _ = clear_engine_probe_alert(
                        &store,
                        &node_id,
                        &target.model_uid,
                        target.replica_id,
                    )
                    .await;
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
                let detail = {
                    let mut ep_guard = endpoint.lock().await;
                    if let Some(info) = ep_guard.get_mut(&target.rkey) {
                        info.status = EndpointStatus::Unhealthy;
                        if info.status_detail.is_none() {
                            info.status_detail =
                                Some("engine health probe failed".to_string());
                        }
                        let detail = info.status_detail.clone();
                        let _ = register_endpoint(&store, info, ttl_ms, lease_id).await;
                        tracing::warn!(
                            model_uid=%target.model_uid,
                            replica_id=target.replica_id,
                            detail=?detail,
                            "endpoint marked Unhealthy"
                        );
                        detail
                    } else {
                        None
                    }
                };
                let alert = EngineProbeAlert {
                    node_id: node_id.clone(),
                    model_uid: target.model_uid.clone(),
                    replica_id: target.replica_id,
                    alert_type: EngineAlertType::HealthProbeFailed,
                    message: detail.unwrap_or_else(|| "engine health probe failed".into()),
                    exit_code: None,
                    created_at_ms: now_ms(),
                };
                let _ = emit_engine_probe_alert(&store, &alert, ttl_ms, lease_id).await;
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
                        info.status_detail = Some(reason.clone());
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
                scrapes: scrape_outcomes,
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
