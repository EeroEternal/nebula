//! P3 compatibility matrix + P4 SLO / diagnostics service helpers.

use serde::{Deserialize, Serialize};

use nebula_common::{
    default_compatibility_rules, evaluate_compatibility, evaluate_slo, CapacitySnapshot,
    CompatCheckInput, CompatibilityRule, DiagnosticEvent, EngineImage, ModelSlo, NodeStatus,
    PlacementPlan, PlacementRejectReason, SloEvaluation, DesiredState,
};
use nebula_meta::MetaStore;

use crate::service::{now_ms, ServiceError};

fn compat_key(id: &str) -> String {
    format!("/compat/{id}")
}

fn slo_key(model_uid: &str) -> String {
    format!("/slos/{model_uid}")
}

pub async fn list_compat_rules(store: &dyn MetaStore) -> Result<Vec<CompatibilityRule>, ServiceError> {
    let entries = store.list_prefix("/compat/").await?;
    let mut out: Vec<CompatibilityRule> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    if out.is_empty() {
        out = default_compatibility_rules(now_ms());
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

pub async fn put_compat_rule(
    store: &dyn MetaStore,
    mut rule: CompatibilityRule,
) -> Result<CompatibilityRule, ServiceError> {
    if rule.id.trim().is_empty() {
        return Err(ServiceError::BadRequest("compat rule id required".into()));
    }
    if rule.engine_type.trim().is_empty() {
        return Err(ServiceError::BadRequest("engine_type required".into()));
    }
    rule.updated_at_ms = now_ms();
    store
        .put(&compat_key(&rule.id), serde_json::to_vec(&rule)?, None)
        .await?;
    Ok(rule)
}

pub async fn delete_compat_rule(store: &dyn MetaStore, id: &str) -> Result<(), ServiceError> {
    let key = compat_key(id);
    if store.get(&key).await?.is_none() {
        return Err(ServiceError::NotFound(format!("compat rule {id} not found")));
    }
    store.delete(&key).await?;
    Ok(())
}

pub async fn seed_default_compat_rules(
    store: &dyn MetaStore,
) -> Result<Vec<CompatibilityRule>, ServiceError> {
    let rules = default_compatibility_rules(now_ms());
    for rule in &rules {
        store
            .put(&compat_key(&rule.id), serde_json::to_vec(rule)?, None)
            .await?;
    }
    Ok(rules)
}

async fn load_image(
    store: &dyn MetaStore,
    image_id: Option<&str>,
    docker_image: Option<&str>,
) -> Result<Option<EngineImage>, ServiceError> {
    let wanted = image_id
        .or(docker_image)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(wanted) = wanted else {
        return Ok(None);
    };
    let entries = store.list_prefix("/images/").await?;
    for (_, v, _) in entries {
        let Ok(img) = serde_json::from_slice::<EngineImage>(&v) else {
            continue;
        };
        if img.id == wanted || img.image == wanted {
            return Ok(Some(img));
        }
    }
    Ok(None)
}

/// Validate deploy intent against compatibility matrix (+ optional target node).
pub async fn validate_deploy_compat(
    store: &dyn MetaStore,
    engine_type: &str,
    engine_version: Option<&str>,
    image_id: Option<&str>,
    docker_image: Option<&str>,
    node_id: Option<&str>,
    override_reason: Option<&str>,
) -> Result<Vec<String>, ServiceError> {
    let rules = list_compat_rules(store).await?;
    let image = load_image(store, image_id, docker_image).await?;
    let platforms = image
        .as_ref()
        .map(|i| i.platforms.clone())
        .unwrap_or_default();

    let node = if let Some(nid) = node_id {
        store
            .get(&format!("/nodes/{nid}"))
            .await?
            .and_then(|(data, _)| serde_json::from_slice::<NodeStatus>(&data).ok())
    } else {
        None
    };

    let input = CompatCheckInput {
        engine_type,
        engine_version,
        platforms: &platforms,
        node: node.as_ref(),
        image_id: image_id.or(docker_image),
    };

    match evaluate_compatibility(&rules, &input) {
        Ok(ids) => Ok(ids),
        Err(reason) => {
            if override_reason.map(str::trim).filter(|s| !s.is_empty()).is_some()
                && reason.code == "compat_denied"
            {
                // Operator override: allow with empty matched ids but keep audit via reason note.
                tracing::warn!(
                    override_reason = ?override_reason,
                    rule = ?reason.rule_id,
                    "compat deny overridden by operator"
                );
                return Ok(vec![format!("override:{}", reason.rule_id.unwrap_or_default())]);
            }
            Err(ServiceError::BadRequest(reason.format_error()))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HardwareInventory {
    pub nodes: Vec<NodeInventory>,
    pub placements: Vec<PlacementOccupancy>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeInventory {
    pub node_id: String,
    pub platform: Option<String>,
    pub last_heartbeat_ms: u64,
    pub gpus: Vec<GpuInventory>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuInventory {
    pub index: u32,
    pub name: Option<String>,
    pub driver_version: Option<String>,
    pub cuda_version: Option<String>,
    pub memory_total_mb: u64,
    pub memory_used_mb: u64,
    pub temperature_c: Option<u32>,
    pub utilization_gpu: Option<u32>,
    pub occupied_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlacementOccupancy {
    pub model_uid: String,
    pub replica_id: u32,
    pub node_id: String,
    pub gpu_indices: Vec<u32>,
}

pub async fn hardware_inventory(store: &dyn MetaStore) -> Result<HardwareInventory, ServiceError> {
    let nodes_raw = store.list_prefix("/nodes/").await?;
    let placements_raw = store.list_prefix("/placements/").await?;

    let mut occupancy: Vec<PlacementOccupancy> = Vec::new();
    for (_, v, _) in &placements_raw {
        let Ok(plan) = serde_json::from_slice::<PlacementPlan>(v) else {
            continue;
        };
        for a in plan.assignments {
            occupancy.push(PlacementOccupancy {
                model_uid: plan.model_uid.clone(),
                replica_id: a.replica_id,
                node_id: a.node_id.clone(),
                gpu_indices: a.effective_gpu_indices().unwrap_or_default(),
            });
        }
    }

    let mut nodes = Vec::new();
    for (_, v, _) in nodes_raw {
        let Ok(n) = serde_json::from_slice::<NodeStatus>(&v) else {
            continue;
        };
        let gpus = n
            .gpus
            .iter()
            .map(|g| {
                let occupied_by = occupancy
                    .iter()
                    .find(|o| o.node_id == n.node_id && o.gpu_indices.contains(&g.index))
                    .map(|o| o.model_uid.clone());
                GpuInventory {
                    index: g.index,
                    name: g.name.clone(),
                    driver_version: g.driver_version.clone(),
                    cuda_version: g.cuda_version.clone(),
                    memory_total_mb: g.memory_total_mb,
                    memory_used_mb: g.memory_used_mb,
                    temperature_c: g.temperature_c,
                    utilization_gpu: g.utilization_gpu,
                    occupied_by,
                }
            })
            .collect();
        nodes.push(NodeInventory {
            node_id: n.node_id,
            platform: n.platform,
            last_heartbeat_ms: n.last_heartbeat_ms,
            gpus,
        });
    }
    nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    Ok(HardwareInventory {
        nodes,
        placements: occupancy,
    })
}

pub async fn capacity_snapshot(store: &dyn MetaStore) -> Result<CapacitySnapshot, ServiceError> {
    use nebula_common::{
        build_capacity_snapshot, EndpointInfo, EndpointStats, ModelDeployment,
    };

    let inv = hardware_inventory(store).await?;
    let gpu_total = inv.nodes.iter().map(|n| n.gpus.len() as u32).sum::<u32>();
    let gpu_free = inv
        .nodes
        .iter()
        .flat_map(|n| n.gpus.iter())
        .filter(|g| g.occupied_by.is_none())
        .count() as u32;

    let deps_raw = store.list_prefix("/deployments/").await?;
    let mut deployments = Vec::new();
    for (_, v, _) in deps_raw {
        if let Ok(d) = serde_json::from_slice::<ModelDeployment>(&v) {
            deployments.push(d);
        }
    }

    let eps_raw = store.list_prefix("/endpoints/").await?;
    let mut endpoints = Vec::new();
    for (_, v, _) in eps_raw {
        if let Ok(e) = serde_json::from_slice::<EndpointInfo>(&v) {
            endpoints.push(e);
        }
    }

    let stats_raw = store.list_prefix("/stats/").await?;
    let mut stats = Vec::new();
    for (_, v, _) in stats_raw {
        if let Ok(s) = serde_json::from_slice::<EndpointStats>(&v) {
            stats.push(s);
        }
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(build_capacity_snapshot(
        &deployments,
        &endpoints,
        &stats,
        gpu_total,
        gpu_free,
        now_ms,
    ))
}

// ── SLO ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpsertSloRequest {
    pub availability_target: Option<f64>,
    pub ttft_p95_ms: Option<f64>,
    pub tpot_p95_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub throughput_tps: Option<f64>,
    pub window: Option<String>,
    pub exclude_abort_from_error_budget: Option<bool>,
    pub exclude_drain_from_error_budget: Option<bool>,
    pub notes: Option<String>,
}

pub async fn get_slo(store: &dyn MetaStore, model_uid: &str) -> Result<Option<ModelSlo>, ServiceError> {
    match store.get(&slo_key(model_uid)).await? {
        Some((data, _)) => Ok(Some(serde_json::from_slice(&data)?)),
        None => Ok(None),
    }
}

pub async fn list_slos(store: &dyn MetaStore) -> Result<Vec<ModelSlo>, ServiceError> {
    let entries = store.list_prefix("/slos/").await?;
    let mut out: Vec<ModelSlo> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| a.model_uid.cmp(&b.model_uid));
    Ok(out)
}

pub async fn upsert_slo(
    store: &dyn MetaStore,
    model_uid: &str,
    req: UpsertSloRequest,
) -> Result<ModelSlo, ServiceError> {
    // Model must exist.
    if store.get(&format!("/models/{model_uid}/spec")).await?.is_none() {
        return Err(ServiceError::NotFound(format!("model {model_uid} not found")));
    }
    let mut slo = get_slo(store, model_uid).await?.unwrap_or(ModelSlo {
        model_uid: model_uid.to_string(),
        availability_target: Some(0.99),
        ttft_p95_ms: Some(2000.0),
        tpot_p95_ms: None,
        latency_p95_ms: Some(30000.0),
        throughput_tps: None,
        window: "15m".into(),
        exclude_abort_from_error_budget: true,
        exclude_drain_from_error_budget: true,
        notes: None,
        updated_at_ms: 0,
    });
    if req.availability_target.is_some() {
        slo.availability_target = req.availability_target;
    }
    if req.ttft_p95_ms.is_some() {
        slo.ttft_p95_ms = req.ttft_p95_ms;
    }
    if req.tpot_p95_ms.is_some() {
        slo.tpot_p95_ms = req.tpot_p95_ms;
    }
    if req.latency_p95_ms.is_some() {
        slo.latency_p95_ms = req.latency_p95_ms;
    }
    if req.throughput_tps.is_some() {
        slo.throughput_tps = req.throughput_tps;
    }
    if let Some(w) = req.window {
        slo.window = w;
    }
    if let Some(v) = req.exclude_abort_from_error_budget {
        slo.exclude_abort_from_error_budget = v;
    }
    if let Some(v) = req.exclude_drain_from_error_budget {
        slo.exclude_drain_from_error_budget = v;
    }
    if req.notes.is_some() {
        slo.notes = req.notes;
    }
    slo.updated_at_ms = now_ms();
    store
        .put(&slo_key(model_uid), serde_json::to_vec(&slo)?, None)
        .await?;
    Ok(slo)
}

pub async fn delete_slo(store: &dyn MetaStore, model_uid: &str) -> Result<(), ServiceError> {
    let key = slo_key(model_uid);
    if store.get(&key).await?.is_none() {
        return Err(ServiceError::NotFound(format!("slo for {model_uid} not found")));
    }
    store.delete(&key).await?;
    Ok(())
}

/// Evaluate SLO using Router metrics text. Low/no traffic → insufficient_data.
pub fn evaluate_slo_from_router_metrics(
    slo: &ModelSlo,
    metrics_text: &str,
) -> SloEvaluation {
    use crate::service::{normalize_zero, parse_histogram_quantile, parse_metric_sum};
    let window_secs = match slo.window.as_str() {
        "5m" => 300.0,
        "15m" => 900.0,
        "1h" => 3600.0,
        "6h" => 21600.0,
        "24h" | "1d" => 86400.0,
        _ => 900.0,
    };
    let req = parse_metric_sum(metrics_text, "nebula_router_requests_total");
    let err5 = parse_metric_sum(metrics_text, "nebula_router_responses_5xx");
    // Abort is intentionally NOT subtracted into the error budget view.
    let _abort = parse_metric_sum(metrics_text, "nebula_router_requests_aborted_total");
    let availability = if req > 0.0 {
        Some(1.0 - (err5 / req))
    } else {
        None
    };
    let request_rate = if window_secs > 0.0 {
        Some(req / window_secs)
    } else {
        None
    };
    let ttft = {
        let s = parse_histogram_quantile(metrics_text, "nebula_route_ttft_seconds", 0.95);
        if s > 0.0 {
            Some(normalize_zero(s * 1000.0))
        } else {
            None
        }
    };
    let latency = {
        let s = parse_histogram_quantile(metrics_text, "nebula_route_latency_seconds", 0.95);
        if s > 0.0 {
            Some(normalize_zero(s * 1000.0))
        } else {
            None
        }
    };
    evaluate_slo(
        slo,
        availability,
        ttft,
        latency,
        request_rate,
        now_ms(),
    )
}

pub async fn list_diagnostic_events(
    store: &dyn MetaStore,
    model_uid: Option<&str>,
) -> Result<Vec<DiagnosticEvent>, ServiceError> {
    let mut events = Vec::new();

    // Deployments
    let deps = store.list_prefix("/deployments/").await?;
    for (key, v, _) in deps {
        let Ok(dep) = serde_json::from_slice::<nebula_common::ModelDeployment>(&v) else {
            continue;
        };
        if let Some(uid) = model_uid {
            if dep.model_uid != uid {
                continue;
            }
        }
        let state = match dep.desired_state {
            DesiredState::Running => "running",
            DesiredState::Stopped => "stopped",
        };
        events.push(DiagnosticEvent {
            ts_ms: dep.updated_at_ms,
            kind: "deployment".into(),
            summary: format!(
                "deployment v{} desired={state} replicas={} image_id={:?}",
                dep.version, dep.replicas, dep.image_id
            ),
            model_uid: Some(dep.model_uid),
            node_id: dep.node_affinity,
            data_source: Some("etcd".into()),
        });
        let _ = key;
    }

    // Placements
    let plans = store.list_prefix("/placements/").await?;
    for (_, v, _) in plans {
        let Ok(plan) = serde_json::from_slice::<PlacementPlan>(&v) else {
            continue;
        };
        if let Some(uid) = model_uid {
            if plan.model_uid != uid {
                continue;
            }
        }
        events.push(DiagnosticEvent {
            ts_ms: plan.updated_at_ms,
            kind: "placement".into(),
            summary: format!(
                "placement v{} assignments={}",
                plan.version,
                plan.assignments.len()
            ),
            model_uid: Some(plan.model_uid),
            node_id: plan.assignments.first().map(|a| a.node_id.clone()),
            data_source: Some("etcd".into()),
        });
    }

    // SLOs
    let slos = store.list_prefix("/slos/").await?;
    for (_, v, _) in slos {
        let Ok(slo) = serde_json::from_slice::<ModelSlo>(&v) else {
            continue;
        };
        if let Some(uid) = model_uid {
            if slo.model_uid != uid {
                continue;
            }
        }
        events.push(DiagnosticEvent {
            ts_ms: slo.updated_at_ms,
            kind: "slo".into(),
            summary: format!(
                "SLO updated window={} availability={:?}",
                slo.window, slo.availability_target
            ),
            model_uid: Some(slo.model_uid),
            node_id: None,
            data_source: Some("etcd".into()),
        });
    }

    events.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms));
    events.truncate(200);
    Ok(events)
}

/// Re-export for callers that need the reject type.
pub type DeployReject = PlacementRejectReason;
