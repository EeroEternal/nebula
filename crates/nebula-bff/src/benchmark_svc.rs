//! P5 benchmark ingest, recommend, and canary APIs.

use serde::Deserialize;

use nebula_common::{
    build_profile_from_runs, builtin_workloads, canary_should_rollback, recommend_from_profiles,
    BenchmarkRun, BenchmarkWorkload, CanaryRelease, CanaryState, PerformanceProfile, ProfileKey,
    RecommendRequest, RecommendResponse,
};
use nebula_meta::MetaStore;

use crate::service::{now_ms, ServiceError};

fn run_key(run_id: &str) -> String {
    format!("/benchmarks/runs/{run_id}")
}

fn profile_key_hash(key: &ProfileKey) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    format!("{:x}", h.finish())
}

fn profile_etcd_key(key: &ProfileKey) -> String {
    format!("/benchmarks/profiles/{}", profile_key_hash(key))
}

fn canary_key(id: &str) -> String {
    format!("/canaries/{id}")
}

pub async fn list_workloads() -> Vec<BenchmarkWorkload> {
    builtin_workloads()
}

pub async fn list_runs(store: &dyn MetaStore) -> Result<Vec<BenchmarkRun>, ServiceError> {
    let entries = store.list_prefix("/benchmarks/runs/").await?;
    let mut out: Vec<BenchmarkRun> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| b.finished_at_ms.cmp(&a.finished_at_ms));
    Ok(out)
}

pub async fn get_run(store: &dyn MetaStore, run_id: &str) -> Result<BenchmarkRun, ServiceError> {
    match store.get(&run_key(run_id)).await? {
        Some((data, _)) => Ok(serde_json::from_slice(&data)?),
        None => Err(ServiceError::NotFound(format!("benchmark run {run_id} not found"))),
    }
}

pub async fn ingest_run(
    store: &dyn MetaStore,
    mut run: BenchmarkRun,
) -> Result<BenchmarkRun, ServiceError> {
    if run.run_id.trim().is_empty() {
        return Err(ServiceError::BadRequest("run_id required".into()));
    }
    if run.profile_key.model_name.trim().is_empty() {
        return Err(ServiceError::BadRequest("profile_key.model_name required".into()));
    }
    if run.finished_at_ms == 0 {
        run.finished_at_ms = now_ms();
    }
    store
        .put(&run_key(&run.run_id), serde_json::to_vec(&run)?, None)
        .await?;

    // Rebuild profile from all runs sharing this key.
    let all = list_runs(store).await?;
    let profile = build_profile_from_runs(&run.profile_key, &all, now_ms());
    store
        .put(
            &profile_etcd_key(&run.profile_key),
            serde_json::to_vec(&profile)?,
            None,
        )
        .await?;
    Ok(run)
}

pub async fn list_profiles(store: &dyn MetaStore) -> Result<Vec<PerformanceProfile>, ServiceError> {
    let entries = store.list_prefix("/benchmarks/profiles/").await?;
    let mut out: Vec<PerformanceProfile> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    Ok(out)
}

pub async fn recommend(
    store: &dyn MetaStore,
    req: RecommendRequest,
) -> Result<RecommendResponse, ServiceError> {
    if req.model_name.trim().is_empty() {
        return Err(ServiceError::BadRequest("model_name required".into()));
    }
    let profiles = list_profiles(store).await?;
    let runs = list_runs(store).await?;
    Ok(recommend_from_profiles(&req, &profiles, &runs))
}

#[derive(Debug, Deserialize)]
pub struct CreateCanaryRequest {
    pub model_uid: String,
    pub candidate_image_id: String,
    #[serde(default)]
    pub stable_image_id: Option<String>,
    #[serde(default)]
    pub traffic_weight_percent: Option<u32>,
    #[serde(default)]
    pub workload_id: Option<String>,
    #[serde(default)]
    pub canary_id: Option<String>,
}

pub async fn list_canaries(store: &dyn MetaStore) -> Result<Vec<CanaryRelease>, ServiceError> {
    let entries = store.list_prefix("/canaries/").await?;
    let mut out: Vec<CanaryRelease> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    Ok(out)
}

pub async fn create_canary(
    store: &dyn MetaStore,
    req: CreateCanaryRequest,
) -> Result<CanaryRelease, ServiceError> {
    if req.model_uid.trim().is_empty() || req.candidate_image_id.trim().is_empty() {
        return Err(ServiceError::BadRequest(
            "model_uid and candidate_image_id required".into(),
        ));
    }
    // Model must exist — recommend failure must not block manual deploy, but canary needs a model.
    if store
        .get(&format!("/models/{}/spec", req.model_uid))
        .await?
        .is_none()
    {
        return Err(ServiceError::NotFound(format!(
            "model {} not found",
            req.model_uid
        )));
    }
    let now = now_ms();
    let canary = CanaryRelease {
        canary_id: req
            .canary_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("canary-{}", uuid::Uuid::new_v4())),
        model_uid: req.model_uid,
        stable_image_id: req.stable_image_id,
        candidate_image_id: req.candidate_image_id,
        traffic_weight_percent: req.traffic_weight_percent.unwrap_or(10).min(100),
        state: CanaryState::Running,
        workload_id: req.workload_id,
        evidence_run_id: None,
        slo_breach: None,
        rollback_reason: None,
        created_at_ms: now,
        updated_at_ms: now,
    };
    store
        .put(&canary_key(&canary.canary_id), serde_json::to_vec(&canary)?, None)
        .await?;
    Ok(canary)
}

#[derive(Debug, Deserialize)]
pub struct EvaluateCanaryRequest {
    /// When true, treat current window as SLO breach (from caller / evaluate_slo).
    pub slo_breaching: bool,
    #[serde(default)]
    pub evidence_run_id: Option<String>,
}

pub async fn evaluate_canary(
    store: &dyn MetaStore,
    canary_id: &str,
    req: EvaluateCanaryRequest,
) -> Result<CanaryRelease, ServiceError> {
    let key = canary_key(canary_id);
    let Some((data, _)) = store.get(&key).await? else {
        return Err(ServiceError::NotFound(format!("canary {canary_id} not found")));
    };
    let mut canary: CanaryRelease = serde_json::from_slice(&data)?;
    canary.slo_breach = Some(req.slo_breaching);
    if let Some(run_id) = req.evidence_run_id {
        canary.evidence_run_id = Some(run_id);
    }
    if canary_should_rollback(req.slo_breaching, canary.traffic_weight_percent) {
        canary.state = CanaryState::RolledBack;
        canary.traffic_weight_percent = 0;
        canary.rollback_reason = Some("SLO breach during canary — traffic weight set to 0".into());
        // Restore deployment image_id to stable if present.
        if let Some(ref stable) = canary.stable_image_id {
            if let Some((dep_data, _)) = store
                .get(&format!("/deployments/{}", canary.model_uid))
                .await?
            {
                if let Ok(mut dep) = serde_json::from_slice::<nebula_common::ModelDeployment>(&dep_data)
                {
                    dep.image_id = Some(stable.clone());
                    dep.image_override_reason =
                        Some(format!("canary {} rollback", canary.canary_id));
                    dep.version = dep.version.saturating_add(1);
                    dep.updated_at_ms = now_ms();
                    store
                        .put(
                            &format!("/deployments/{}", canary.model_uid),
                            serde_json::to_vec(&dep)?,
                            None,
                        )
                        .await?;
                }
            }
        }
    } else if !req.slo_breaching && canary.state == CanaryState::Running {
        // Hold state; promotion is explicit.
    }
    canary.updated_at_ms = now_ms();
    store
        .put(&key, serde_json::to_vec(&canary)?, None)
        .await?;
    Ok(canary)
}

pub async fn promote_canary(
    store: &dyn MetaStore,
    canary_id: &str,
) -> Result<CanaryRelease, ServiceError> {
    let key = canary_key(canary_id);
    let Some((data, _)) = store.get(&key).await? else {
        return Err(ServiceError::NotFound(format!("canary {canary_id} not found")));
    };
    let mut canary: CanaryRelease = serde_json::from_slice(&data)?;
    if canary.state == CanaryState::RolledBack {
        return Err(ServiceError::Conflict(
            "canary already rolled back; create a new canary".into(),
        ));
    }
    if canary.slo_breach == Some(true) {
        return Err(ServiceError::Conflict(
            "cannot promote canary with SLO breach".into(),
        ));
    }
    canary.state = CanaryState::Completed;
    canary.traffic_weight_percent = 100;
    canary.updated_at_ms = now_ms();

    if let Some((dep_data, _)) = store
        .get(&format!("/deployments/{}", canary.model_uid))
        .await?
    {
        if let Ok(mut dep) = serde_json::from_slice::<nebula_common::ModelDeployment>(&dep_data) {
            dep.image_id = Some(canary.candidate_image_id.clone());
            dep.image_override_reason = Some(format!("canary {} promote", canary.canary_id));
            dep.version = dep.version.saturating_add(1);
            dep.updated_at_ms = now_ms();
            store
                .put(
                    &format!("/deployments/{}", canary.model_uid),
                    serde_json::to_vec(&dep)?,
                    None,
                )
                .await?;
        }
    }

    store
        .put(&key, serde_json::to_vec(&canary)?, None)
        .await?;
    Ok(canary)
}

pub async fn rollback_canary(
    store: &dyn MetaStore,
    canary_id: &str,
    reason: Option<String>,
) -> Result<CanaryRelease, ServiceError> {
    let mut canary = evaluate_canary(
        store,
        canary_id,
        EvaluateCanaryRequest {
            slo_breaching: true,
            evidence_run_id: None,
        },
    )
    .await?;
    if let Some(r) = reason {
        canary.rollback_reason = Some(r);
        canary.updated_at_ms = now_ms();
        store
            .put(
                &canary_key(canary_id),
                serde_json::to_vec(&canary)?,
                None,
            )
            .await?;
    }
    Ok(canary)
}
