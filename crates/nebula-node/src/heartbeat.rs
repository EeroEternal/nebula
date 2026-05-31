use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;

use nebula_common::{EndpointInfo, EndpointStatus, NodeStatus};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::docker_api::{EngineMetricSnapshot, NodeMetricsSnapshot, SharedNodeMetrics};
use crate::gpu::read_gpu_statuses;
use crate::reconcile::RunningModel;
use crate::util::now_ms;

/// Number of consecutive health-check failures before marking endpoint as Unhealthy.
const UNHEALTHY_THRESHOLD: u32 = 3;
/// Number of consecutive failures before attempting a container restart.
const RESTART_THRESHOLD: u32 = 5;
/// Cooldown period after a restart attempt (seconds). During this time health checks
/// are skipped to give the engine time to initialize.
const RESTART_COOLDOWN_SECS: u64 = 120;

pub async fn register_endpoint(
    store: &EtcdMetaStore,
    info: &EndpointInfo,
    ttl_ms: u64,
) -> anyhow::Result<()> {
    let key = format!("/endpoints/{}/{}", info.model_uid, info.replica_id);
    let bytes = serde_json::to_vec(info)?;
    let _ = store.put(&key, bytes, Some(ttl_ms)).await?;
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

pub async fn heartbeat_loop(
    store: EtcdMetaStore,
    node_id: String,
    ttl_ms: u64,
    interval_ms: u64,
    api_port: u16,
    endpoint: Arc<Mutex<HashMap<String, EndpointInfo>>>,
    running: Arc<Mutex<HashMap<String, RunningModel>>>,
    xtrace: Option<xtrace_client::Client>,
    shared_metrics: SharedNodeMetrics,
) {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();

    // Track consecutive health-check failures per model_uid
    let mut fail_counts: HashMap<String, u32> = HashMap::new();
    // Track last restart timestamp (ms) per model_uid for cooldown
    let mut restart_at: HashMap<String, u64> = HashMap::new();

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

        if let Err(e) = store.put(&key, bytes, Some(ttl_ms)).await {
            tracing::warn!(error=%e, "failed to write heartbeat");
        }

        // Refresh endpoint registrations (best-effort; never block heartbeat loop).
        if let Ok(mut guard) = endpoint.try_lock() {
            for info in guard.values_mut() {
                info.last_heartbeat_ms = now_ms();
                if let Err(e) = register_endpoint(&store, info, ttl_ms).await {
                    tracing::warn!(error=%e, "failed to refresh endpoint");
                }
            }
        } else {
            tracing::debug!("skipping endpoint refresh: endpoint state lock busy");
        }

        // Scrape engine metrics and write to etcd /stats/
        // Also perform health checks on each running engine
        {
            let Ok(running_guard) = running.try_lock() else {
                tracing::debug!("skipping engine health/stats: running state lock busy");
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
                continue;
            };
            let now = now_ms();
            for rm in running_guard.values() {
                // Skip health check during restart cooldown
                if let Some(&last_restart) = restart_at.get(&rm.model_uid) {
                    let elapsed_secs = (now.saturating_sub(last_restart)) / 1000;
                    if elapsed_secs < RESTART_COOLDOWN_SECS {
                        tracing::debug!(
                            model_uid=%rm.model_uid,
                            remaining_secs=RESTART_COOLDOWN_SECS - elapsed_secs,
                            "skipping health check during restart cooldown"
                        );
                        continue;
                    }
                }

                let healthy = rm.engine.health_check(&rm.handle).await;

                let count = fail_counts.entry(rm.model_uid.clone()).or_insert(0);

                if healthy {
                    // Recovered: reset counter and ensure endpoint is Ready
                    if *count > 0 {
                        tracing::info!(model_uid=%rm.model_uid, prev_failures=*count, "engine recovered");
                        *count = 0;
                        restart_at.remove(&rm.model_uid);
                        // Mark endpoint back to Ready
                        let mut ep_guard = endpoint.lock().await;
                        if let Some(info) = ep_guard.get_mut(&rm.model_uid) {
                            if info.status == EndpointStatus::Unhealthy {
                                info.status = EndpointStatus::Ready;
                                let _ = register_endpoint(&store, info, ttl_ms).await;
                                tracing::info!(model_uid=%rm.model_uid, "endpoint marked Ready again");
                            }
                        }
                    }

                    // Scrape metrics only when healthy
                    if let Some(stats) = rm
                        .engine
                        .scrape_stats(&http, &rm.handle, &rm.model_uid, rm.replica_id)
                        .await
                    {
                        // Collect engine stats for xtrace
                        if xtrace.is_some() {
                            let ts = Utc::now();
                            let labels = HashMap::from([
                                ("node_id".to_string(), node_id.clone()),
                                ("model_uid".to_string(), rm.model_uid.clone()),
                                ("replica_id".to_string(), rm.replica_id.to_string()),
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

                        // Collect for Prometheus /metrics snapshot
                        let kv_usage = match (stats.kv_cache_used_bytes, stats.kv_cache_free_bytes)
                        {
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
                            model_uid: rm.model_uid.clone(),
                            replica_id: rm.replica_id,
                            pending_requests: stats.pending_requests,
                            kv_cache_usage: kv_usage,
                            prefix_cache_hit_rate: stats.prefix_cache_hit_rate,
                        });
                    }
                } else {
                    *count += 1;
                    tracing::warn!(
                        model_uid=%rm.model_uid,
                        consecutive_failures=*count,
                        "engine health check failed"
                    );

                    // Mark Unhealthy after threshold
                    if *count == UNHEALTHY_THRESHOLD {
                        let mut ep_guard = endpoint.lock().await;
                        if let Some(info) = ep_guard.get_mut(&rm.model_uid) {
                            info.status = EndpointStatus::Unhealthy;
                            let _ = register_endpoint(&store, info, ttl_ms).await;
                            tracing::warn!(model_uid=%rm.model_uid, "endpoint marked Unhealthy");
                        }
                    }

                    // Attempt engine restart after higher threshold
                    if *count >= RESTART_THRESHOLD {
                        tracing::warn!(model_uid=%rm.model_uid, "attempting engine restart");
                        rm.engine.try_restart(&rm.handle).await;
                        // Keep count at 1 (not 0) so that recovery is detected after cooldown
                        *count = 1;
                        restart_at.insert(rm.model_uid.clone(), now_ms());
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
