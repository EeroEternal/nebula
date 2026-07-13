use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashMap;
use uuid::Uuid;

use nebula_common::{
    parse_cell_ingress_metrics, validate_engine_and_config, CellHealthStatus, CellIngress,
    CellIngressStats, CellScrapeStatus, DesiredState, DiskAlert, DownloadPhase, DownloadProgress,
    EndpointInfo, EndpointStats, InternalTopologyVisibility, ModelCacheEntry, ModelConfig,
    ModelDeployment, ModelRequest, ModelRequestStatus, ModelSource, ModelSpec, ModelTemplate,
    NodeDiskStatus, PlacementPlan, ServingTopology, ServingTopologyKind, TemplateCategory,
    TemplateSource,
};
use nebula_meta::MetaStore;

// ---------------------------------------------------------------------------
// Service Errors & IntoResponse
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Etcd error: {0}")]
    Etcd(#[from] anyhow::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Upstream error: {0}")]
    Upstream(String),
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ServiceError::Etcd(ref e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "etcd_error",
                e.to_string(),
            ),
            ServiceError::Serialization(ref e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "serialization_error",
                e.to_string(),
            ),
            ServiceError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            ServiceError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg),
            ServiceError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            ServiceError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Unauthorized".to_string(),
            ),
            ServiceError::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg)
            }
            ServiceError::Upstream(msg) => (StatusCode::BAD_GATEWAY, "upstream_error", msg),
        };

        let body = json!({
            "error": {
                "code": code,
                "message": message,
                "request_id": format!("req_{}", Uuid::new_v4()),
            }
        });

        (status, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// Domain Structs & Views
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AggregatedModelState {
    Stopped,
    Downloading,
    Starting,
    Running,
    Degraded,
    Failed,
    Stopping,
}

const FAILED_THRESHOLD_MS: u64 = 5 * 60 * 1000; // 5 minutes

#[derive(Serialize)]
pub struct ReplicaCount {
    pub desired: u32,
    pub ready: u32,
    pub unhealthy: u32,
}

#[derive(Serialize)]
pub struct ModelView {
    pub model_uid: String,
    pub model_name: String,
    pub engine_type: Option<String>,
    pub state: AggregatedModelState,
    pub replicas: ReplicaCount,
    pub endpoints: Vec<EndpointInfo>,
    pub labels: HashMap<String, String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Serialize)]
pub struct DownloadProgressView {
    pub replicas: Vec<DownloadProgress>,
}

#[derive(Serialize)]
pub struct CacheStatusView {
    pub cached_on_nodes: Vec<String>,
    pub total_size_bytes: u64,
}

#[derive(Serialize)]
pub struct ModelDetailView {
    pub model_uid: String,
    pub model_name: String,
    pub engine_type: Option<String>,
    pub state: AggregatedModelState,
    pub replicas: ReplicaCount,
    pub labels: HashMap<String, String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub spec: ModelSpec,
    pub deployment: Option<ModelDeployment>,
    pub placement: Option<PlacementPlan>,
    pub endpoints: Vec<EndpointInfo>,
    pub stats: Vec<EndpointStats>,
    pub capabilities: Vec<nebula_common::ReplicaCapability>,
    pub download_progress: Option<DownloadProgressView>,
    pub cache_status: Option<CacheStatusView>,
}

#[derive(Serialize)]
pub struct CacheSummary {
    pub total_cached_models: usize,
    pub total_cache_size_bytes: u64,
    pub nodes: Vec<NodeDiskStatus>,
    pub caches: Vec<ModelCacheEntry>,
}

#[derive(Serialize)]
struct ModelGcRequest {
    model_uid: String,
    model_name: String,
    model_path: Option<String>,
    requested_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateModelRequest {
    pub model_name: String,
    pub model_uid: Option<String>,
    pub model_source: Option<ModelSource>,
    pub model_path: Option<String>,
    pub engine_type: Option<String>,
    pub docker_image: Option<String>,
    pub config: Option<ModelConfig>,
    pub labels: Option<HashMap<String, String>>,
    pub auto_start: Option<bool>,
    pub replicas: Option<u32>,
    pub node_id: Option<String>,
    pub gpu_indices: Option<Vec<u32>>,
}

#[derive(Deserialize)]
pub struct UpdateModelRequest {
    pub model_name: Option<String>,
    pub model_source: Option<ModelSource>,
    pub model_path: Option<String>,
    pub engine_type: Option<String>,
    pub docker_image: Option<String>,
    pub config: Option<ModelConfig>,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct StartModelRequest {
    pub replicas: Option<u32>,
    pub config_overrides: Option<ModelConfig>,
    pub node_id: Option<String>,
    pub gpu_indices: Option<Vec<u32>>,
    #[serde(default)]
    pub image_id: Option<String>,
    #[serde(default)]
    pub image_override_reason: Option<String>,
}

#[derive(Deserialize)]
pub struct ScaleModelRequest {
    pub replicas: u32,
}

#[derive(Deserialize)]
pub struct DeployTemplateRequest {
    pub model_uid: Option<String>,
    pub replicas: Option<u32>,
    pub config_overrides: Option<ModelConfig>,
    pub node_id: Option<String>,
    pub gpu_indices: Option<Vec<u32>>,
}

#[derive(Deserialize)]
pub struct SaveAsTemplateRequest {
    pub template_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<TemplateCategory>,
}

#[derive(Deserialize)]
pub struct CreateTemplateRequest {
    pub template_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<TemplateCategory>,
    pub model_name: String,
    pub model_source: Option<ModelSource>,
    pub engine_type: Option<String>,
    pub docker_image: Option<String>,
    pub config: Option<ModelConfig>,
    pub default_replicas: Option<u32>,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub category: Option<TemplateCategory>,
    pub model_name: Option<String>,
    pub model_source: Option<ModelSource>,
    pub engine_type: Option<String>,
    pub docker_image: Option<String>,
    pub config: Option<ModelConfig>,
    pub default_replicas: Option<u32>,
    pub labels: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct ListModelsQuery {
    pub state: Option<String>,
    pub label: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn generate_model_uid(model_name: &str) -> String {
    let uid: String = model_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let uid = uid.trim_matches('-').to_string();
    let mut result = String::new();
    let mut prev_dash = false;
    for c in uid.chars() {
        if c == '-' {
            if !prev_dash {
                result.push(c);
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    if result.len() > 63 {
        result.truncate(63);
    }
    result.trim_end_matches('-').to_string()
}

pub fn is_valid_model_uid(uid: &str) -> bool {
    if uid.is_empty() || uid.len() > 63 {
        return false;
    }
    let mut chars = uid.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return false;
        }
    }
    true
}

pub fn model_name_matches(cache_name: &str, spec_name: &str) -> bool {
    if cache_name == spec_name {
        return true;
    }
    let cache_lc = cache_name.to_lowercase();
    let spec_lc = spec_name.to_lowercase();
    if cache_lc == spec_lc {
        return true;
    }
    let cache_tail = cache_lc.rsplit('/').next().unwrap_or_default();
    let spec_tail = spec_lc.rsplit('/').next().unwrap_or_default();

    cache_tail == spec_tail
        || cache_tail == spec_lc
        || spec_tail == cache_lc
        || spec_lc.starts_with(&(cache_lc.clone() + "/"))
        || cache_lc.starts_with(&(spec_lc + "/"))
}

pub fn compute_aggregated_state(
    deployment: Option<&ModelDeployment>,
    placement: Option<&PlacementPlan>,
    endpoints: &[EndpointInfo],
    download_progress: &[DownloadProgress],
    spec_created_at_ms: u64,
) -> AggregatedModelState {
    let dep = match deployment {
        None => return AggregatedModelState::Stopped,
        Some(d) => d,
    };

    if dep.desired_state == DesiredState::Stopped {
        if !endpoints.is_empty() {
            return AggregatedModelState::Stopping;
        }
        return AggregatedModelState::Stopped;
    }

    if placement.is_none() {
        return AggregatedModelState::Starting;
    }

    let has_active_download = download_progress
        .iter()
        .any(|dp| dp.phase != DownloadPhase::Complete && dp.phase != DownloadPhase::Failed);
    if has_active_download {
        return AggregatedModelState::Downloading;
    }

    let ready_count = endpoints
        .iter()
        .filter(|ep| ep.status == nebula_common::EndpointStatus::Ready)
        .count();
    let total_count = endpoints.len();

    if total_count > 0 && ready_count == total_count {
        return AggregatedModelState::Running;
    }
    if ready_count > 0 {
        return AggregatedModelState::Degraded;
    }

    let base_ts = dep.updated_at_ms.max(spec_created_at_ms);
    let elapsed = now_ms().saturating_sub(base_ts);
    if total_count == 0 && elapsed > FAILED_THRESHOLD_MS {
        return AggregatedModelState::Failed;
    }

    AggregatedModelState::Starting
}

// ---------------------------------------------------------------------------
// Etcd DB Operations Helpers
// ---------------------------------------------------------------------------

pub async fn get_model_spec(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<ModelSpec, ServiceError> {
    match store.get(&format!("/models/{model_uid}/spec")).await? {
        Some((data, _)) => serde_json::from_slice(&data).map_err(Into::into),
        None => Err(ServiceError::NotFound("model not found".to_string())),
    }
}

pub async fn get_model_deployment(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<Option<ModelDeployment>, ServiceError> {
    match store.get(&format!("/deployments/{model_uid}")).await? {
        Some((data, _)) => Ok(Some(serde_json::from_slice(&data)?)),
        None => Ok(None),
    }
}

pub async fn get_model_template(
    store: &dyn MetaStore,
    id: &str,
) -> Result<ModelTemplate, ServiceError> {
    match store.get(&format!("/templates/{id}")).await? {
        Some((data, _)) => serde_json::from_slice(&data).map_err(Into::into),
        None => Err(ServiceError::NotFound("template not found".to_string())),
    }
}

pub async fn put_model_spec(
    store: &dyn MetaStore,
    model_uid: &str,
    spec: &ModelSpec,
) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(spec)?;
    store
        .put(&format!("/models/{model_uid}/spec"), val, None)
        .await?;
    Ok(())
}

pub async fn put_model_deployment(
    store: &dyn MetaStore,
    model_uid: &str,
    dep: &ModelDeployment,
) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(dep)?;
    store
        .put(&format!("/deployments/{model_uid}"), val, None)
        .await?;
    Ok(())
}

pub async fn put_model_template(
    store: &dyn MetaStore,
    id: &str,
    tpl: &ModelTemplate,
) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(tpl)?;
    store.put(&format!("/templates/{id}"), val, None).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Service Implementation
// ---------------------------------------------------------------------------

fn ensure_engine_config(
    engine_type: Option<&str>,
    config: Option<&ModelConfig>,
) -> Result<String, ServiceError> {
    validate_engine_and_config(engine_type, config).map_err(ServiceError::BadRequest)
}

pub async fn create_model(
    store: &dyn MetaStore,
    principal: String,
    req: CreateModelRequest,
) -> Result<ModelSpec, ServiceError> {
    let uid = match req.model_uid {
        Some(ref uid) => {
            if !is_valid_model_uid(uid) {
                return Err(ServiceError::BadRequest(
                    "model_uid must match [a-z0-9][a-z0-9-]* and be at most 63 chars".to_string(),
                ));
            }
            uid.clone()
        }
        None => generate_model_uid(&req.model_name),
    };

    if store.get(&format!("/models/{uid}/spec")).await?.is_some() {
        return Err(ServiceError::Conflict(format!(
            "model with uid '{uid}' already exists"
        )));
    }

    let resolved_engine = ensure_engine_config(req.engine_type.as_deref(), req.config.as_ref())?;

    let now = now_ms();
    let spec = ModelSpec {
        model_uid: uid.clone(),
        model_name: req.model_name,
        model_source: req.model_source.unwrap_or(ModelSource::HuggingFace),
        model_path: req.model_path,
        engine_type: Some(resolved_engine),
        docker_image: req.docker_image,
        config: req.config,
        labels: req.labels.unwrap_or_default(),
        created_at_ms: now,
        updated_at_ms: now,
        created_by: Some(principal),
    };

    put_model_spec(store, &uid, &spec).await?;

    if req.auto_start.unwrap_or(false) {
        let deployment = ModelDeployment {
            model_uid: uid.clone(),
            desired_state: DesiredState::Running,
            replicas: req.replicas.unwrap_or(1),
            min_replicas: None,
            max_replicas: None,
            node_affinity: req.node_id,
            gpu_affinity: req.gpu_indices,
            config_overrides: None,
            image_id: None,
            image_override_reason: None,
            compat_rule_ids: vec![],
            version: 1,
            updated_at_ms: now,
        };
        put_model_deployment(store, &uid, &deployment).await?;
    }

    Ok(spec)
}

pub async fn build_model_view(store: &dyn MetaStore, spec: &ModelSpec) -> ModelView {
    let uid = &spec.model_uid;

    let deployment = store
        .get(&format!("/deployments/{uid}"))
        .await
        .ok()
        .flatten()
        .and_then(|(data, _)| serde_json::from_slice::<ModelDeployment>(&data).ok());

    let placement = store
        .get(&format!("/placements/{uid}"))
        .await
        .ok()
        .flatten()
        .and_then(|(data, _)| serde_json::from_slice::<PlacementPlan>(&data).ok());

    let endpoints: Vec<EndpointInfo> = store
        .list_prefix(&format!("/endpoints/{uid}/"))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let download_progress: Vec<DownloadProgress> = store
        .list_prefix(&format!("/download_progress/{uid}/"))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let state = compute_aggregated_state(
        deployment.as_ref(),
        placement.as_ref(),
        &endpoints,
        &download_progress,
        spec.created_at_ms,
    );

    let desired = deployment.as_ref().map(|d| d.replicas).unwrap_or(0);
    let ready = endpoints
        .iter()
        .filter(|ep| ep.status == nebula_common::EndpointStatus::Ready)
        .count() as u32;
    let unhealthy = endpoints
        .iter()
        .filter(|ep| ep.status == nebula_common::EndpointStatus::Unhealthy)
        .count() as u32;

    ModelView {
        model_uid: spec.model_uid.clone(),
        model_name: spec.model_name.clone(),
        engine_type: spec.engine_type.clone(),
        state,
        replicas: ReplicaCount {
            desired,
            ready,
            unhealthy,
        },
        endpoints,
        labels: spec.labels.clone(),
        created_at_ms: spec.created_at_ms,
        updated_at_ms: spec.updated_at_ms,
    }
}

pub async fn list_models(
    store: &dyn MetaStore,
    params: ListModelsQuery,
) -> Result<Vec<ModelView>, ServiceError> {
    let specs_raw = store.list_prefix("/models/").await?;
    let specs: Vec<ModelSpec> = specs_raw
        .into_iter()
        .filter(|(k, _, _)| k.ends_with("/spec"))
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let mut views = Vec::with_capacity(specs.len());
    for spec in &specs {
        let view = build_model_view(store, spec).await;

        if let Some(ref state_filter) = params.state {
            let state_str = serde_json::to_string(&view.state).unwrap_or_default();
            let state_str = state_str.trim_matches('"');
            if state_str != state_filter {
                continue;
            }
        }

        if let Some(ref label_filter) = params.label {
            if let Some((k, v)) = label_filter.split_once('=') {
                if spec.labels.get(k) != Some(&v.to_string()) {
                    continue;
                }
            }
        }

        views.push(view);
    }

    Ok(views)
}

pub async fn get_model_detail(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<ModelDetailView, ServiceError> {
    let spec = get_model_spec(store, model_uid).await?;

    let deployment = get_model_deployment(store, model_uid).await?;

    let placement = store
        .get(&format!("/placements/{model_uid}"))
        .await
        .ok()
        .flatten()
        .and_then(|(data, _)| serde_json::from_slice::<PlacementPlan>(&data).ok());

    let endpoints: Vec<EndpointInfo> = store
        .list_prefix(&format!("/endpoints/{model_uid}/"))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let stats: Vec<EndpointStats> = store
        .list_prefix(&format!("/stats/{model_uid}/"))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let capabilities: Vec<nebula_common::ReplicaCapability> = store
        .list_prefix(&format!("/capabilities/{model_uid}/"))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let download_progress: Vec<DownloadProgress> = store
        .list_prefix(&format!("/download_progress/{model_uid}/"))
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let all_caches: Vec<ModelCacheEntry> = store
        .list_prefix("/model_cache/")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .filter(|c: &ModelCacheEntry| model_name_matches(&c.model_name, &spec.model_name))
        .collect();

    let state = compute_aggregated_state(
        deployment.as_ref(),
        placement.as_ref(),
        &endpoints,
        &download_progress,
        spec.created_at_ms,
    );

    let desired = deployment.as_ref().map(|d| d.replicas).unwrap_or(0);
    let ready = endpoints
        .iter()
        .filter(|ep| ep.status == nebula_common::EndpointStatus::Ready)
        .count() as u32;
    let unhealthy = endpoints
        .iter()
        .filter(|ep| ep.status == nebula_common::EndpointStatus::Unhealthy)
        .count() as u32;

    let cache_status = if all_caches.is_empty() {
        None
    } else {
        Some(CacheStatusView {
            cached_on_nodes: all_caches.iter().map(|c| c.node_id.clone()).collect(),
            total_size_bytes: all_caches.iter().map(|c| c.size_bytes).sum(),
        })
    };

    let dp_view = if download_progress.is_empty() {
        None
    } else {
        Some(DownloadProgressView {
            replicas: download_progress,
        })
    };

    Ok(ModelDetailView {
        model_uid: spec.model_uid.clone(),
        model_name: spec.model_name.clone(),
        engine_type: spec.engine_type.clone(),
        state,
        replicas: ReplicaCount {
            desired,
            ready,
            unhealthy,
        },
        labels: spec.labels.clone(),
        created_at_ms: spec.created_at_ms,
        updated_at_ms: spec.updated_at_ms,
        spec,
        deployment,
        placement,
        endpoints,
        stats,
        capabilities,
        download_progress: dp_view,
        cache_status,
    })
}

pub async fn update_model(
    store: &dyn MetaStore,
    model_uid: &str,
    req: UpdateModelRequest,
) -> Result<ModelSpec, ServiceError> {
    let mut spec = get_model_spec(store, model_uid).await?;

    if let Some(name) = req.model_name {
        spec.model_name = name;
    }
    if let Some(source) = req.model_source {
        spec.model_source = source;
    }
    if req.model_path.is_some() {
        spec.model_path = req.model_path;
    }
    if req.engine_type.is_some() {
        spec.engine_type = req.engine_type;
    }
    if req.docker_image.is_some() {
        spec.docker_image = req.docker_image;
    }
    if req.config.is_some() {
        spec.config = req.config;
    }
    if let Some(labels) = req.labels {
        spec.labels = labels;
    }

    let resolved = ensure_engine_config(spec.engine_type.as_deref(), spec.config.as_ref())?;
    spec.engine_type = Some(resolved);
    spec.updated_at_ms = now_ms();

    put_model_spec(store, model_uid, &spec).await?;

    Ok(spec)
}

pub async fn delete_model(store: &dyn MetaStore, model_uid: &str) -> Result<usize, ServiceError> {
    let spec = get_model_spec(store, model_uid).await?;

    let mut queued_gc_nodes = 0;
    if let Ok(nodes) = store.list_prefix("/node_disk/").await {
        let req = ModelGcRequest {
            model_uid: model_uid.to_string(),
            model_name: spec.model_name.clone(),
            model_path: spec.model_path.clone(),
            requested_at_ms: now_ms(),
        };
        if let Ok(payload) = serde_json::to_vec(&req) {
            for (key, _, _) in nodes {
                if let Some(node_id) = key.strip_prefix("/node_disk/").filter(|id| !id.is_empty()) {
                    let gc_key = format!("/model_gc_requests/{node_id}/{model_uid}");
                    // TTL so orphaned requests do not linger if the node never returns.
                    const MODEL_GC_TTL_MS: u64 = 24 * 60 * 60 * 1000;
                    if store
                        .put(&gc_key, payload.clone(), Some(MODEL_GC_TTL_MS))
                        .await
                        .is_ok()
                    {
                        queued_gc_nodes += 1;
                    }
                }
            }
        }
    }

    store.delete(&format!("/models/{model_uid}/spec")).await?;
    store.delete(&format!("/deployments/{model_uid}")).await?;
    store.delete(&format!("/placements/{model_uid}")).await?;

    if let Ok(kvs) = store.list_prefix(&format!("/endpoints/{model_uid}/")).await {
        for (k, _, _) in kvs {
            let _ = store.delete(&k).await;
        }
    }
    if let Ok(kvs) = store.list_prefix(&format!("/stats/{model_uid}/")).await {
        for (k, _, _) in kvs {
            let _ = store.delete(&k).await;
        }
    }
    if let Ok(kvs) = store
        .list_prefix(&format!("/download_progress/{model_uid}/"))
        .await
    {
        for (k, _, _) in kvs {
            let _ = store.delete(&k).await;
        }
    }

    Ok(queued_gc_nodes)
}

pub async fn start_model(
    store: &dyn MetaStore,
    model_uid: &str,
    req: StartModelRequest,
) -> Result<ModelDeployment, ServiceError> {
    // Verify spec exists and engine/config are still valid before scheduling.
    let spec = get_model_spec(store, model_uid).await?;
    let effective_config = req
        .config_overrides
        .as_ref()
        .or(spec.config.as_ref());
    ensure_engine_config(spec.engine_type.as_deref(), effective_config)?;

    let image_id = req
        .image_id
        .clone()
        .or_else(|| spec.docker_image.clone());
    let compat_ids = crate::compat_slo::validate_deploy_compat(
        store,
        spec.engine_type.as_deref().unwrap_or("vllm"),
        None,
        image_id.as_deref(),
        spec.docker_image.as_deref(),
        req.node_id.as_deref(),
        req.image_override_reason.as_deref(),
    )
    .await?;

    let now = now_ms();
    let deployment = match get_model_deployment(store, model_uid).await? {
        Some(mut dep) => {
            dep.desired_state = DesiredState::Running;
            if let Some(r) = req.replicas {
                dep.replicas = r;
            }
            if req.config_overrides.is_some() {
                dep.config_overrides = req.config_overrides;
            }
            if req.node_id.is_some() {
                dep.node_affinity = req.node_id;
            }
            if req.gpu_indices.is_some() {
                dep.gpu_affinity = req.gpu_indices;
            }
            if req.image_id.is_some() {
                dep.image_id = req.image_id;
            } else if dep.image_id.is_none() {
                dep.image_id = image_id.clone();
            }
            if req.image_override_reason.is_some() {
                dep.image_override_reason = req.image_override_reason;
            }
            dep.compat_rule_ids = compat_ids;
            dep.version += 1;
            dep.updated_at_ms = now;
            dep
        }
        None => ModelDeployment {
            model_uid: model_uid.to_string(),
            desired_state: DesiredState::Running,
            replicas: req.replicas.unwrap_or(1),
            min_replicas: None,
            max_replicas: None,
            node_affinity: req.node_id,
            gpu_affinity: req.gpu_indices,
            config_overrides: req.config_overrides,
            image_id,
            image_override_reason: req.image_override_reason,
            compat_rule_ids: compat_ids,
            version: 1,
            updated_at_ms: now,
        },
    };

    put_model_deployment(store, model_uid, &deployment).await?;
    Ok(deployment)
}

pub async fn stop_model(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<ModelDeployment, ServiceError> {
    // Verify spec exists
    get_model_spec(store, model_uid).await?;

    let now = now_ms();
    let deployment = match get_model_deployment(store, model_uid).await? {
        Some(mut dep) => {
            dep.desired_state = DesiredState::Stopped;
            dep.version += 1;
            dep.updated_at_ms = now;
            dep
        }
        None => ModelDeployment {
            model_uid: model_uid.to_string(),
            desired_state: DesiredState::Stopped,
            replicas: 0,
            min_replicas: None,
            max_replicas: None,
            node_affinity: None,
            gpu_affinity: None,
            config_overrides: None,
            image_id: None,
            image_override_reason: None,
            compat_rule_ids: vec![],
            version: 1,
            updated_at_ms: now,
        },
    };

    put_model_deployment(store, model_uid, &deployment).await?;
    Ok(deployment)
}

pub async fn scale_model(
    store: &dyn MetaStore,
    model_uid: &str,
    req: ScaleModelRequest,
) -> Result<ModelDeployment, ServiceError> {
    let mut dep = match get_model_deployment(store, model_uid).await? {
        Some(d) => d,
        None => {
            return Err(ServiceError::NotFound(
                "deployment not found (model may not be started)".to_string(),
            ))
        }
    };

    dep.replicas = req.replicas;
    dep.version += 1;
    dep.updated_at_ms = now_ms();

    put_model_deployment(store, model_uid, &dep).await?;
    Ok(dep)
}

// ---------------------------------------------------------------------------
// Template CRUD
// ---------------------------------------------------------------------------

pub async fn list_templates(store: &dyn MetaStore) -> Result<Vec<ModelTemplate>, ServiceError> {
    let kvs = store.list_prefix("/templates/").await?;
    let templates: Vec<ModelTemplate> = kvs
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    Ok(templates)
}

pub async fn create_template(
    store: &dyn MetaStore,
    req: CreateTemplateRequest,
) -> Result<ModelTemplate, ServiceError> {
    let tid = req
        .template_id
        .clone()
        .unwrap_or_else(|| format!("tpl-{}", Uuid::new_v4()));

    if store.get(&format!("/templates/{tid}")).await?.is_some() {
        return Err(ServiceError::Conflict(format!(
            "template with id '{tid}' already exists"
        )));
    }

    let now = now_ms();
    let resolved_engine = ensure_engine_config(req.engine_type.as_deref(), req.config.as_ref())?;
    let template = ModelTemplate {
        template_id: tid.clone(),
        name: req.name,
        description: req.description,
        category: req.category,
        model_name: req.model_name,
        model_source: req.model_source,
        engine_type: Some(resolved_engine),
        docker_image: req.docker_image,
        config: req.config,
        default_replicas: req.default_replicas.unwrap_or(1),
        labels: req.labels.unwrap_or_default(),
        source: TemplateSource::User,
        created_at_ms: now,
        updated_at_ms: now,
    };

    put_model_template(store, &tid, &template).await?;
    Ok(template)
}

pub async fn update_template(
    store: &dyn MetaStore,
    id: &str,
    req: UpdateTemplateRequest,
) -> Result<ModelTemplate, ServiceError> {
    let mut template = get_model_template(store, id).await?;

    if let Some(n) = req.name {
        template.name = n;
    }
    if req.description.is_some() {
        template.description = req.description;
    }
    if let Some(cat) = req.category {
        template.category = Some(cat);
    }
    if let Some(mn) = req.model_name {
        template.model_name = mn;
    }
    if req.model_source.is_some() {
        template.model_source = req.model_source;
    }
    if req.engine_type.is_some() {
        template.engine_type = req.engine_type;
    }
    if req.docker_image.is_some() {
        template.docker_image = req.docker_image;
    }
    if req.config.is_some() {
        template.config = req.config;
    }
    if let Some(dr) = req.default_replicas {
        template.default_replicas = dr;
    }
    if let Some(lbls) = req.labels {
        template.labels = lbls;
    }
    let resolved = ensure_engine_config(template.engine_type.as_deref(), template.config.as_ref())?;
    template.engine_type = Some(resolved);
    template.updated_at_ms = now_ms();

    put_model_template(store, id, &template).await?;
    Ok(template)
}

pub async fn delete_template(store: &dyn MetaStore, id: &str) -> Result<(), ServiceError> {
    get_model_template(store, id).await?;
    store.delete(&format!("/templates/{id}")).await?;
    Ok(())
}

pub async fn deploy_template(
    store: &dyn MetaStore,
    principal: String,
    id: &str,
    req: DeployTemplateRequest,
) -> Result<ModelSpec, ServiceError> {
    let tpl = get_model_template(store, id).await?;

    let uid = req
        .model_uid
        .clone()
        .unwrap_or_else(|| generate_model_uid(&tpl.model_name));

    if store.get(&format!("/models/{uid}/spec")).await?.is_some() {
        return Err(ServiceError::Conflict(format!(
            "model with uid '{uid}' already exists"
        )));
    }

    let now = now_ms();
    let resolved_engine = ensure_engine_config(
        tpl.engine_type.as_deref(),
        req.config_overrides.as_ref().or(tpl.config.as_ref()),
    )?;
    let spec = ModelSpec {
        model_uid: uid.clone(),
        model_name: tpl.model_name,
        model_source: tpl.model_source.unwrap_or(ModelSource::HuggingFace),
        model_path: None,
        engine_type: Some(resolved_engine),
        docker_image: tpl.docker_image,
        config: tpl.config,
        labels: tpl.labels,
        created_at_ms: now,
        updated_at_ms: now,
        created_by: Some(principal),
    };

    put_model_spec(store, &uid, &spec).await?;

    let deployment = ModelDeployment {
        model_uid: uid.clone(),
        desired_state: DesiredState::Running,
        replicas: req.replicas.unwrap_or(tpl.default_replicas),
        min_replicas: None,
        max_replicas: None,
        node_affinity: req.node_id,
        gpu_affinity: req.gpu_indices,
        config_overrides: req.config_overrides,
        image_id: None,
        image_override_reason: None,
        compat_rule_ids: vec![],
        version: 1,
        updated_at_ms: now,
    };
    put_model_deployment(store, &uid, &deployment).await?;

    Ok(spec)
}

pub async fn save_as_template(
    store: &dyn MetaStore,
    model_uid: &str,
    req: SaveAsTemplateRequest,
) -> Result<ModelTemplate, ServiceError> {
    let spec = get_model_spec(store, model_uid).await?;
    let deployment = get_model_deployment(store, model_uid).await?;

    let tid = req
        .template_id
        .clone()
        .unwrap_or_else(|| format!("tpl-{}", Uuid::new_v4()));

    let now = now_ms();
    let template = ModelTemplate {
        template_id: tid.clone(),
        name: req.name,
        description: req.description,
        category: req.category,
        model_name: spec.model_name,
        model_source: Some(spec.model_source),
        engine_type: spec.engine_type,
        docker_image: spec.docker_image,
        config: spec.config,
        default_replicas: deployment.as_ref().map(|d| d.replicas).unwrap_or(1),
        labels: spec.labels,
        source: TemplateSource::Saved,
        created_at_ms: now,
        updated_at_ms: now,
    };

    put_model_template(store, &tid, &template).await?;
    Ok(template)
}

// ---------------------------------------------------------------------------
// Shared HTTP error envelope (v1 + v2 handlers)
// ---------------------------------------------------------------------------

/// Uniform error JSON used by BFF HTTP handlers.
pub fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = json!({
        "error": {
            "code": code,
            "message": message,
            "request_id": format!("req_{}", Uuid::new_v4()),
        }
    });
    (status, Json(body)).into_response()
}

// ---------------------------------------------------------------------------
// Cache / disk / alerts
// ---------------------------------------------------------------------------

pub async fn list_node_cache(
    store: &dyn MetaStore,
    node_id: &str,
) -> Result<Vec<ModelCacheEntry>, ServiceError> {
    let kvs = store.list_prefix(&format!("/model_cache/{node_id}/")).await?;
    Ok(kvs
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect())
}

pub async fn get_node_disk(
    store: &dyn MetaStore,
    node_id: &str,
) -> Result<NodeDiskStatus, ServiceError> {
    match store.get(&format!("/node_disk/{node_id}")).await? {
        Some((data, _)) => Ok(serde_json::from_slice(&data)?),
        None => Err(ServiceError::NotFound(
            "disk status not found for node".to_string(),
        )),
    }
}

pub async fn build_cache_summary(store: &dyn MetaStore) -> Result<CacheSummary, ServiceError> {
    let caches: Vec<ModelCacheEntry> = store
        .list_prefix("/model_cache/")
        .await?
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let nodes: Vec<NodeDiskStatus> = store
        .list_prefix("/node_disk/")
        .await?
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let total_size: u64 = caches.iter().map(|c| c.size_bytes).sum();

    Ok(CacheSummary {
        total_cached_models: caches.len(),
        total_cache_size_bytes: total_size,
        nodes,
        caches,
    })
}

pub async fn list_disk_alerts(store: &dyn MetaStore) -> Result<Vec<DiskAlert>, ServiceError> {
    let kvs = store.list_prefix("/alerts/").await?;
    Ok(kvs
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect())
}

// ---------------------------------------------------------------------------
// v1 → v2 migration
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct MigrationDetail {
    pub model_uid: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct MigrationResult {
    pub total: usize,
    pub migrated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub details: Vec<MigrationDetail>,
}

pub async fn migrate_v1_to_v2(store: &dyn MetaStore) -> Result<MigrationResult, ServiceError> {
    let requests_raw = store.list_prefix("/model_requests/").await?;
    let model_requests: Vec<ModelRequest> = requests_raw
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let total = model_requests.len();
    let mut migrated = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;
    let mut details = Vec::new();

    for mr in &model_requests {
        let model_uid = &mr.request.model_uid;

        match store.get(&format!("/models/{model_uid}/spec")).await {
            Ok(Some(_)) => {
                skipped += 1;
                details.push(MigrationDetail {
                    model_uid: model_uid.clone(),
                    action: "skipped".to_string(),
                    desired_state: None,
                    reason: Some("already_exists".to_string()),
                });
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                failed += 1;
                details.push(MigrationDetail {
                    model_uid: model_uid.clone(),
                    action: "failed".to_string(),
                    desired_state: None,
                    reason: Some(format!("etcd get error: {e}")),
                });
                continue;
            }
        }

        let now = now_ms();

        let spec = ModelSpec {
            model_uid: model_uid.clone(),
            model_name: mr.request.model_name.clone(),
            model_source: ModelSource::HuggingFace,
            model_path: None,
            engine_type: mr.request.engine_type.clone(),
            docker_image: mr.request.docker_image.clone(),
            config: mr.request.config.clone(),
            labels: HashMap::new(),
            created_at_ms: mr.created_at_ms,
            updated_at_ms: now,
            created_by: Some("migration".to_string()),
        };

        if let Err(e) = put_model_spec(store, model_uid, &spec).await {
            failed += 1;
            details.push(MigrationDetail {
                model_uid: model_uid.clone(),
                action: "failed".to_string(),
                desired_state: None,
                reason: Some(format!("spec write error: {e}")),
            });
            continue;
        }

        let desired_state = match &mr.status {
            ModelRequestStatus::Running | ModelRequestStatus::Scheduled => DesiredState::Running,
            _ => DesiredState::Stopped,
        };

        let gpu_affinity = mr
            .request
            .gpu_indices
            .clone()
            .or_else(|| mr.request.gpu_index.map(|idx| vec![idx]));

        let deployment = ModelDeployment {
            model_uid: model_uid.clone(),
            desired_state: desired_state.clone(),
            replicas: mr.request.replicas,
            min_replicas: mr.request.min_replicas,
            max_replicas: mr.request.max_replicas,
            node_affinity: mr.request.node_id.clone(),
            gpu_affinity,
            config_overrides: mr.request.config.clone(),
            image_id: None,
            image_override_reason: None,
            compat_rule_ids: vec![],
            version: 1,
            updated_at_ms: now,
        };

        if let Err(e) = put_model_deployment(store, model_uid, &deployment).await {
            failed += 1;
            details.push(MigrationDetail {
                model_uid: model_uid.clone(),
                action: "failed".to_string(),
                desired_state: None,
                reason: Some(format!("deployment write error: {e}")),
            });
            continue;
        }

        let ds_str = match desired_state {
            DesiredState::Running => "running",
            DesiredState::Stopped => "stopped",
        };

        migrated += 1;
        details.push(MigrationDetail {
            model_uid: model_uid.clone(),
            action: "migrated".to_string(),
            desired_state: Some(ds_str.to_string()),
            reason: None,
        });
    }

    Ok(MigrationResult {
        total,
        migrated,
        skipped,
        failed,
        details,
    })
}

// ---------------------------------------------------------------------------
// Router metrics fetch + gateway observability aggregation
// ---------------------------------------------------------------------------

pub async fn fetch_router_metrics_text(
    http: &reqwest::Client,
    router_url: &str,
) -> Result<String, ServiceError> {
    let metrics_url = format!("{}/metrics", router_url.trim_end_matches('/'));
    let resp = http
        .get(metrics_url)
        .send()
        .await
        .map_err(|e| ServiceError::Upstream(format!("router request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(ServiceError::Upstream(format!(
            "router metrics responded with status {}",
            resp.status().as_u16()
        )));
    }

    resp.text()
        .await
        .map_err(|e| ServiceError::Upstream(format!("failed to read router response: {e}")))
}

fn parse_window_seconds(window: &str) -> Option<u64> {
    match window {
        "5m" => Some(5 * 60),
        "15m" => Some(15 * 60),
        "1h" => Some(60 * 60),
        "6h" => Some(6 * 60 * 60),
        "24h" => Some(24 * 60 * 60),
        _ => None,
    }
}

fn metric_line_matches(line: &str, metric: &str) -> bool {
    if !line.starts_with(metric) {
        return false;
    }
    matches!(line.as_bytes().get(metric.len()), Some(b' ') | Some(b'{'))
}

pub(crate) fn parse_metric_sum(metrics_text: &str, metric: &str) -> f64 {
    metrics_text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter(|line| metric_line_matches(line, metric))
        .filter_map(|line| line.split_whitespace().last())
        .filter_map(|value| value.parse::<f64>().ok())
        .sum()
}

fn parse_metric_sum_with_label(metrics_text: &str, metric: &str, label: &str, value: &str) -> f64 {
    let token = format!(r#"{label}="{value}""#);
    metrics_text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter(|line| metric_line_matches(line, metric))
        .filter(|line| line.contains(&token))
        .filter_map(|line| line.split_whitespace().last())
        .filter_map(|v| v.parse::<f64>().ok())
        .sum()
}

fn extract_label_value(line: &str, label: &str) -> Option<String> {
    let token = format!(r#"{label}=""#);
    let start = line.find(&token)? + token.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub(crate) fn parse_histogram_quantile(metrics_text: &str, metric: &str, quantile: f64) -> f64 {
    let bucket_metric = format!("{metric}_bucket");
    let mut buckets: Vec<(f64, f64)> = Vec::new();
    let mut total = 0.0;

    for line in metrics_text.lines().filter(|line| !line.starts_with('#')) {
        if !metric_line_matches(line, &bucket_metric) {
            continue;
        }

        let le = match extract_label_value(line, "le") {
            Some(v) => v,
            None => continue,
        };

        let value = match line
            .split_whitespace()
            .last()
            .and_then(|v| v.parse::<f64>().ok())
        {
            Some(v) => v,
            None => continue,
        };

        if le == "+Inf" {
            total += value;
            continue;
        }

        if let Ok(boundary) = le.parse::<f64>() {
            buckets.push((boundary, value));
        }
    }

    if total <= 0.0 || buckets.is_empty() {
        return 0.0;
    }

    buckets.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    let target = total * quantile.clamp(0.0, 1.0);

    for (boundary, cumulative) in buckets {
        if cumulative >= target {
            return boundary;
        }
    }

    0.0
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(crate) fn normalize_zero(value: f64) -> f64 {
    if value.abs() < 1e-12 {
        0.0
    } else {
        value
    }
}

#[derive(Serialize)]
pub struct GatewayOverviewResponse {
    pub window: String,
    /// Prometheus scrape source. Console path stays `/gateway/*` for compatibility.
    pub data_source: &'static str,
    pub rps: f64,
    pub error_5xx_ratio: f64,
    pub retry_success_ratio: f64,
    pub circuit_open_count: u64,
}

#[derive(Serialize)]
pub struct TimePoint {
    pub ts: String,
    pub value: f64,
}

#[derive(Serialize)]
pub struct GatewayTrafficSeries {
    pub requests_total: Vec<TimePoint>,
    pub responses_2xx: Vec<TimePoint>,
    pub responses_4xx: Vec<TimePoint>,
    pub responses_5xx: Vec<TimePoint>,
}

#[derive(Serialize)]
pub struct GatewayTrafficResponse {
    pub window: String,
    pub data_source: &'static str,
    pub series: GatewayTrafficSeries,
}

#[derive(Serialize)]
pub struct GatewayReliabilitySeries {
    pub retry_total: Vec<TimePoint>,
    pub retry_success_total: Vec<TimePoint>,
    pub upstream_error_connect: Vec<TimePoint>,
    pub upstream_error_timeout: Vec<TimePoint>,
    pub upstream_error_5xx: Vec<TimePoint>,
    pub upstream_error_other: Vec<TimePoint>,
}

#[derive(Serialize)]
pub struct GatewayReliabilityResponse {
    pub window: String,
    pub data_source: &'static str,
    pub series: GatewayReliabilitySeries,
}

#[derive(Serialize)]
pub struct GatewayProtectionResponse {
    pub window: String,
    pub data_source: &'static str,
    pub request_too_large_count: u64,
    pub circuit_skipped_count: u64,
    pub circuit_open_count: u64,
}

#[derive(Serialize)]
pub struct GatewayLatencySeries {
    pub latency_p50_ms: Vec<TimePoint>,
    pub latency_p95_ms: Vec<TimePoint>,
    pub latency_p99_ms: Vec<TimePoint>,
    pub ttft_p50_ms: Vec<TimePoint>,
    pub ttft_p95_ms: Vec<TimePoint>,
}

#[derive(Serialize)]
pub struct GatewayLatencyResponse {
    pub window: String,
    /// Prometheus scrape source for these series. Currently Router
    /// (`nebula_route_*`); the HTTP path remains `/gateway/latency` for
    /// console compatibility.
    pub data_source: &'static str,
    pub series: GatewayLatencySeries,
}

fn require_window(window: &str) -> Result<u64, ServiceError> {
    parse_window_seconds(window).ok_or_else(|| {
        ServiceError::BadRequest("window must be one of: 5m, 15m, 1h, 6h, 24h".to_string())
    })
}

pub fn gateway_overview_from_metrics(
    text: &str,
    window: String,
) -> Result<GatewayOverviewResponse, ServiceError> {
    let window_seconds = require_window(&window)?;
    let requests_total = parse_metric_sum(text, "nebula_router_requests_total");
    let responses_5xx = parse_metric_sum(text, "nebula_router_responses_5xx");
    let retry_total = parse_metric_sum(text, "nebula_router_retry_total");
    let retry_success_total = parse_metric_sum(text, "nebula_router_retry_success_total");
    let circuit_open_total = parse_metric_sum(text, "nebula_router_circuit_open_total");

    let error_5xx_ratio = if requests_total > 0.0 {
        responses_5xx / requests_total
    } else {
        0.0
    };
    let retry_success_ratio = if retry_total > 0.0 {
        retry_success_total / retry_total
    } else {
        0.0
    };

    Ok(GatewayOverviewResponse {
        window,
        data_source: "router",
        rps: normalize_zero(requests_total / window_seconds as f64),
        error_5xx_ratio: normalize_zero(error_5xx_ratio),
        retry_success_ratio: normalize_zero(retry_success_ratio),
        circuit_open_count: circuit_open_total as u64,
    })
}

pub fn gateway_traffic_from_metrics(
    text: &str,
    window: String,
) -> Result<GatewayTrafficResponse, ServiceError> {
    let window_seconds = require_window(&window)?;
    let ts = now_rfc3339();
    let to_point = |value: f64| TimePoint {
        ts: ts.clone(),
        value,
    };

    Ok(GatewayTrafficResponse {
        window,
        data_source: "router",
        series: GatewayTrafficSeries {
            requests_total: vec![to_point(normalize_zero(
                parse_metric_sum(text, "nebula_router_requests_total") / window_seconds as f64,
            ))],
            responses_2xx: vec![to_point(normalize_zero(
                parse_metric_sum(text, "nebula_router_responses_2xx") / window_seconds as f64,
            ))],
            responses_4xx: vec![to_point(normalize_zero(
                parse_metric_sum(text, "nebula_router_responses_4xx") / window_seconds as f64,
            ))],
            responses_5xx: vec![to_point(normalize_zero(
                parse_metric_sum(text, "nebula_router_responses_5xx") / window_seconds as f64,
            ))],
        },
    })
}

pub fn gateway_reliability_from_metrics(
    text: &str,
    window: String,
) -> Result<GatewayReliabilityResponse, ServiceError> {
    let window_seconds = require_window(&window)?;
    let ts = now_rfc3339();
    let to_point = |value: f64| TimePoint {
        ts: ts.clone(),
        value,
    };
    let rate = |metric: &str, label: Option<(&str, &str)>| {
        let v = match label {
            Some((k, val)) => parse_metric_sum_with_label(text, metric, k, val),
            None => parse_metric_sum(text, metric),
        };
        normalize_zero(v / window_seconds as f64)
    };

    Ok(GatewayReliabilityResponse {
        window,
        data_source: "router",
        series: GatewayReliabilitySeries {
            retry_total: vec![to_point(rate("nebula_router_retry_total", None))],
            retry_success_total: vec![to_point(rate("nebula_router_retry_success_total", None))],
            upstream_error_connect: vec![to_point(rate(
                "nebula_router_upstream_error_total",
                Some(("kind", "connect")),
            ))],
            upstream_error_timeout: vec![to_point(rate(
                "nebula_router_upstream_error_total",
                Some(("kind", "timeout")),
            ))],
            upstream_error_5xx: vec![to_point(rate(
                "nebula_router_upstream_error_total",
                Some(("kind", "upstream_5xx")),
            ))],
            upstream_error_other: vec![to_point(rate(
                "nebula_router_upstream_error_total",
                Some(("kind", "other")),
            ))],
        },
    })
}

pub fn gateway_protection_from_metrics(
    text: &str,
    window: String,
) -> Result<GatewayProtectionResponse, ServiceError> {
    require_window(&window)?;
    Ok(GatewayProtectionResponse {
        window,
        data_source: "router",
        request_too_large_count: parse_metric_sum(text, "nebula_router_request_too_large_total")
            as u64,
        circuit_skipped_count: parse_metric_sum(text, "nebula_router_route_circuit_skipped_total")
            as u64,
        circuit_open_count: parse_metric_sum(text, "nebula_router_circuit_open_total") as u64,
    })
}

/// Parse Router latency/TTFT histograms from Prometheus text.
///
/// HTTP API path remains `/observability/gateway/latency` for console
/// compatibility; the actual series come from Router (`nebula_route_*`).
pub fn gateway_latency_from_metrics(
    text: &str,
    window: String,
) -> Result<GatewayLatencyResponse, ServiceError> {
    require_window(&window)?;
    let ts = now_rfc3339();
    let to_point = |value: f64| TimePoint {
        ts: ts.clone(),
        value,
    };

    Ok(GatewayLatencyResponse {
        window,
        data_source: "router",
        series: GatewayLatencySeries {
            latency_p50_ms: vec![to_point(normalize_zero(
                parse_histogram_quantile(text, "nebula_route_latency_seconds", 0.50) * 1000.0,
            ))],
            latency_p95_ms: vec![to_point(normalize_zero(
                parse_histogram_quantile(text, "nebula_route_latency_seconds", 0.95) * 1000.0,
            ))],
            latency_p99_ms: vec![to_point(normalize_zero(
                parse_histogram_quantile(text, "nebula_route_latency_seconds", 0.99) * 1000.0,
            ))],
            ttft_p50_ms: vec![to_point(normalize_zero(
                parse_histogram_quantile(text, "nebula_route_ttft_seconds", 0.50) * 1000.0,
            ))],
            ttft_p95_ms: vec![to_point(normalize_zero(
                parse_histogram_quantile(text, "nebula_route_ttft_seconds", 0.95) * 1000.0,
            ))],
        },
    })
}

// ── Native Serving Cell (P2) ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterCellRequest {
    pub model_uid: String,
    #[serde(default)]
    pub cell_id: Option<String>,
    pub base_url: String,
    pub topology: ServingTopology,
    #[serde(default)]
    pub health_url: Option<String>,
    #[serde(default)]
    pub metrics_url: Option<String>,
    #[serde(default)]
    pub engine_type: Option<String>,
    #[serde(default)]
    pub engine_version: Option<String>,
    /// Skip OpenAI probe (tests / trusted offline registration only).
    #[serde(default)]
    pub skip_probe: bool,
}

fn cell_key(model_uid: &str, cell_id: &str) -> String {
    format!("/cells/{model_uid}/{cell_id}")
}

fn sanitize_cell_for_api(mut cell: CellIngress) -> CellIngress {
    cell.internal_topology = InternalTopologyVisibility::NotVisible;
    cell
}

async fn ensure_no_nebula_managed_running(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<(), ServiceError> {
    if let Some(dep) = get_model_deployment(store, model_uid).await? {
        if dep.desired_state == DesiredState::Running {
            return Err(ServiceError::Conflict(format!(
                "model '{model_uid}' is Nebula-managed (DesiredState::Running); \
                 stop it before registering an external Serving Cell"
            )));
        }
    }
    Ok(())
}

async fn probe_openai_compatible(
    http: &reqwest::Client,
    base_url: &str,
    health_url: Option<&str>,
) -> Result<(), ServiceError> {
    let base = base_url.trim_end_matches('/');
    if let Some(h) = health_url {
        let resp = http
            .get(h)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|e| ServiceError::BadRequest(format!("cell health probe failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ServiceError::BadRequest(format!(
                "cell health probe returned {}",
                resp.status()
            )));
        }
    }
    let models_url = format!("{base}/v1/models");
    let resp = http
        .get(&models_url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| {
            ServiceError::BadRequest(format!("OpenAI /v1/models probe failed: {e}"))
        })?;
    if !resp.status().is_success() {
        return Err(ServiceError::BadRequest(format!(
            "OpenAI /v1/models probe returned {}",
            resp.status()
        )));
    }
    Ok(())
}

pub async fn list_cells(store: &dyn MetaStore) -> Result<Vec<CellIngress>, ServiceError> {
    let entries = store.list_prefix("/cells/").await?;
    let mut out = Vec::with_capacity(entries.len());
    for (key, data, _) in entries {
        match serde_json::from_slice::<CellIngress>(&data) {
            Ok(cell) => out.push(sanitize_cell_for_api(cell)),
            Err(e) => tracing::warn!(%key, error = %e, "skip invalid cell entry"),
        }
    }
    out.sort_by(|a, b| {
        a.model_uid
            .cmp(&b.model_uid)
            .then_with(|| a.cell_id.cmp(&b.cell_id))
    });
    Ok(out)
}

pub async fn get_cell(
    store: &dyn MetaStore,
    model_uid: &str,
    cell_id: &str,
) -> Result<CellIngress, ServiceError> {
    match store.get(&cell_key(model_uid, cell_id)).await? {
        Some((data, _)) => {
            let cell: CellIngress = serde_json::from_slice(&data)?;
            Ok(sanitize_cell_for_api(cell))
        }
        None => Err(ServiceError::NotFound(format!(
            "cell {model_uid}/{cell_id} not found"
        ))),
    }
}

pub async fn register_cell(
    store: &dyn MetaStore,
    http: &reqwest::Client,
    req: RegisterCellRequest,
) -> Result<CellIngress, ServiceError> {
    let model_uid = req.model_uid.trim().to_string();
    if model_uid.is_empty() {
        return Err(ServiceError::BadRequest(
            "model_uid is required".to_string(),
        ));
    }
    let base_url = req.base_url.trim().to_string();
    if base_url.is_empty() {
        return Err(ServiceError::BadRequest("base_url is required".to_string()));
    }
    match req.topology.kind {
        ServingTopologyKind::NativeGateway | ServingTopologyKind::PdDisaggregated => {}
        ServingTopologyKind::Standalone | ServingTopologyKind::Replicated => {
            return Err(ServiceError::BadRequest(
                "Serving Cell requires topology kind native_gateway or pd_disaggregated"
                    .to_string(),
            ));
        }
    }
    ensure_no_nebula_managed_running(store, &model_uid).await?;

    let cell_id = req
        .cell_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = now_ms();
    let mut cell = CellIngress {
        cell_id: cell_id.clone(),
        model_uid: model_uid.clone(),
        base_url: base_url.clone(),
        health_url: req.health_url.clone(),
        metrics_url: req.metrics_url.clone(),
        topology: req.topology,
        engine_type: req.engine_type,
        engine_version: req.engine_version,
        status: CellHealthStatus::Unknown,
        internal_topology: InternalTopologyVisibility::NotVisible,
        last_checked_ms: 0,
        updated_at_ms: now,
    };

    if !req.skip_probe {
        let health = cell.resolved_health_url();
        match probe_openai_compatible(http, &base_url, Some(health.as_str())).await {
            Ok(()) => {
                cell.status = CellHealthStatus::Ready;
                cell.last_checked_ms = now;
            }
            Err(e) => {
                cell.status = CellHealthStatus::Unhealthy;
                cell.last_checked_ms = now;
                return Err(e);
            }
        }
    } else {
        cell.status = CellHealthStatus::Ready;
        cell.last_checked_ms = now;
    }

    let val = serde_json::to_vec(&cell)?;
    store
        .put(&cell_key(&model_uid, &cell_id), val, None)
        .await?;
    Ok(sanitize_cell_for_api(cell))
}

/// Remove Cell registration from etcd only. Never stops the external process.
pub async fn deregister_cell(
    store: &dyn MetaStore,
    model_uid: &str,
    cell_id: &str,
) -> Result<(), ServiceError> {
    let key = cell_key(model_uid, cell_id);
    match store.get(&key).await? {
        Some(_) => {
            store.delete(&key).await?;
            Ok(())
        }
        None => Err(ServiceError::NotFound(format!(
            "cell {model_uid}/{cell_id} not found"
        ))),
    }
}

/// Read-only Cell observation: health probe + Ingress `/metrics` scrape.
/// Persists refreshed `status` / `last_checked_ms` to etcd so Router can prefer Ready cells.
/// Never writes `/stats/` and never invents worker topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellObservation {
    pub cell: CellIngress,
    pub health_ok: bool,
    pub stats: CellIngressStats,
    /// Always `not_visible` in Batch 2 — no official worker read API wired.
    pub internal_topology: InternalTopologyVisibility,
}

async fn load_raw_cell(
    store: &dyn MetaStore,
    model_uid: &str,
    cell_id: &str,
) -> Result<CellIngress, ServiceError> {
    match store.get(&cell_key(model_uid, cell_id)).await? {
        Some((data, _)) => Ok(serde_json::from_slice(&data)?),
        None => Err(ServiceError::NotFound(format!(
            "cell {model_uid}/{cell_id} not found"
        ))),
    }
}

async fn probe_cell_health(http: &reqwest::Client, cell: &CellIngress) -> bool {
    let url = cell.resolved_health_url();
    match http
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn scrape_cell_metrics(
    http: &reqwest::Client,
    cell: &CellIngress,
    now: u64,
) -> CellIngressStats {
    let metrics_url = cell.resolved_metrics_url();
    match http
        .get(&metrics_url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(text) => parse_cell_ingress_metrics(&text, &metrics_url, now),
            Err(_) => CellIngressStats {
                scraped_at_ms: now,
                metrics_url,
                data_source: "cell_ingress".into(),
                pending_requests: None,
                kv_cache_usage: None,
                prefix_cache_hit_rate: None,
                scrape_status: CellScrapeStatus::Empty,
            },
        },
        Ok(_) => CellIngressStats {
            scraped_at_ms: now,
            metrics_url,
            data_source: "cell_ingress".into(),
            pending_requests: None,
            kv_cache_usage: None,
            prefix_cache_hit_rate: None,
            scrape_status: CellScrapeStatus::HttpError,
        },
        Err(_) => CellIngressStats {
            scraped_at_ms: now,
            metrics_url,
            data_source: "cell_ingress".into(),
            pending_requests: None,
            kv_cache_usage: None,
            prefix_cache_hit_rate: None,
            scrape_status: CellScrapeStatus::Unreachable,
        },
    }
}

pub async fn observe_cell(
    store: &dyn MetaStore,
    http: &reqwest::Client,
    model_uid: &str,
    cell_id: &str,
) -> Result<CellObservation, ServiceError> {
    let mut cell = load_raw_cell(store, model_uid, cell_id).await?;
    let now = now_ms();
    let health_ok = probe_cell_health(http, &cell).await;
    let stats = scrape_cell_metrics(http, &cell, now).await;

    cell.status = if health_ok {
        CellHealthStatus::Ready
    } else {
        CellHealthStatus::Unhealthy
    };
    cell.last_checked_ms = now;
    cell.updated_at_ms = now;
    cell.internal_topology = InternalTopologyVisibility::NotVisible;

    let val = serde_json::to_vec(&cell)?;
    store
        .put(&cell_key(model_uid, cell_id), val, None)
        .await?;

    Ok(CellObservation {
        cell: sanitize_cell_for_api(cell),
        health_ok,
        stats,
        internal_topology: InternalTopologyVisibility::NotVisible,
    })
}

#[cfg(test)]
mod gateway_metrics_tests {
    use super::*;

    #[test]
    fn parse_counter_sum() {
        let text = r#"
# HELP nebula_router_requests_total total
# TYPE nebula_router_requests_total counter
nebula_router_requests_total{route="chat"} 10
nebula_router_requests_total{route="embed"} 5
"#;
        assert_eq!(parse_metric_sum(text, "nebula_router_requests_total"), 15.0);
    }

    #[test]
    fn overview_rejects_bad_window() {
        assert!(matches!(
            gateway_overview_from_metrics("", "2m".into()),
            Err(ServiceError::BadRequest(_))
        ));
    }

    #[test]
    fn reliability_reads_upstream_5xx_label() {
        let text = r#"
nebula_router_upstream_error_total{kind="upstream_5xx"} 7
nebula_router_upstream_error_total{kind="5xx"} 99
nebula_router_upstream_error_total{kind="connect"} 1
"#;
        let resp = gateway_reliability_from_metrics(text, "5m".into()).unwrap();
        assert_eq!(resp.data_source, "router");
        // 7 / 300s window
        assert!((resp.series.upstream_error_5xx[0].value - (7.0 / 300.0)).abs() < 1e-9);
        assert!((resp.series.upstream_error_connect[0].value - (1.0 / 300.0)).abs() < 1e-9);
    }

    #[test]
    fn overview_marks_router_data_source() {
        let text = "nebula_router_requests_total 100\nnebula_router_responses_5xx 10\n";
        let resp = gateway_overview_from_metrics(text, "5m".into()).unwrap();
        assert_eq!(resp.data_source, "router");
    }
}

