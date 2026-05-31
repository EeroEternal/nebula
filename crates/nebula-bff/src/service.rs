use std::collections::HashMap;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use nebula_common::{
    DesiredState, DownloadPhase, DownloadProgress, EndpointInfo, EndpointStats,
    ModelCacheEntry, ModelConfig, ModelDeployment, ModelSource,
    ModelSpec, ModelTemplate, NodeDiskStatus, PlacementPlan, TemplateCategory, TemplateSource,
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
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ServiceError::Etcd(ref e) => (StatusCode::INTERNAL_SERVER_ERROR, "etcd_error", e.to_string()),
            ServiceError::Serialization(ref e) => (StatusCode::INTERNAL_SERVER_ERROR, "serialization_error", e.to_string()),
            ServiceError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            ServiceError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg),
            ServiceError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            ServiceError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", "Unauthorized".to_string()),
            ServiceError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg),
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

pub async fn get_model_spec(store: &dyn MetaStore, model_uid: &str) -> Result<ModelSpec, ServiceError> {
    match store.get(&format!("/models/{model_uid}/spec")).await? {
        Some((data, _)) => serde_json::from_slice(&data).map_err(Into::into),
        None => Err(ServiceError::NotFound("model not found".to_string())),
    }
}

pub async fn get_model_deployment(store: &dyn MetaStore, model_uid: &str) -> Result<Option<ModelDeployment>, ServiceError> {
    match store.get(&format!("/deployments/{model_uid}")).await? {
        Some((data, _)) => Ok(Some(serde_json::from_slice(&data)?)),
        None => Ok(None),
    }
}

pub async fn get_model_template(store: &dyn MetaStore, id: &str) -> Result<ModelTemplate, ServiceError> {
    match store.get(&format!("/templates/{id}")).await? {
        Some((data, _)) => serde_json::from_slice(&data).map_err(Into::into),
        None => Err(ServiceError::NotFound("template not found".to_string())),
    }
}

pub async fn put_model_spec(store: &dyn MetaStore, model_uid: &str, spec: &ModelSpec) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(spec)?;
    store.put(&format!("/models/{model_uid}/spec"), val, None).await?;
    Ok(())
}

pub async fn put_model_deployment(store: &dyn MetaStore, model_uid: &str, dep: &ModelDeployment) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(dep)?;
    store.put(&format!("/deployments/{model_uid}"), val, None).await?;
    Ok(())
}

pub async fn put_model_template(store: &dyn MetaStore, id: &str, tpl: &ModelTemplate) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(tpl)?;
    store.put(&format!("/templates/{id}"), val, None).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Service Implementation
// ---------------------------------------------------------------------------

pub async fn create_model(
    store: &dyn MetaStore,
    principal: String,
    req: CreateModelRequest,
) -> Result<ModelSpec, ServiceError> {
    let uid = match req.model_uid {
        Some(ref uid) => {
            if !is_valid_model_uid(uid) {
                return Err(ServiceError::BadRequest("model_uid must match [a-z0-9][a-z0-9-]* and be at most 63 chars".to_string()));
            }
            uid.clone()
        }
        None => generate_model_uid(&req.model_name),
    };

    if store.get(&format!("/models/{uid}/spec")).await?.is_some() {
        return Err(ServiceError::Conflict(format!("model with uid '{uid}' already exists")));
    }

    let now = now_ms();
    let spec = ModelSpec {
        model_uid: uid.clone(),
        model_name: req.model_name,
        model_source: req.model_source.unwrap_or(ModelSource::HuggingFace),
        model_path: req.model_path,
        engine_type: req.engine_type,
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

pub async fn get_model_detail(store: &dyn MetaStore, model_uid: &str) -> Result<ModelDetailView, ServiceError> {
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
                    if store.put(&gc_key, payload.clone(), None).await.is_ok() {
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
    if let Ok(kvs) = store.list_prefix(&format!("/download_progress/{model_uid}/")).await {
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
    // Verify spec exists
    get_model_spec(store, model_uid).await?;

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
            version: 1,
            updated_at_ms: now,
        },
    };

    put_model_deployment(store, model_uid, &deployment).await?;
    Ok(deployment)
}

pub async fn stop_model(store: &dyn MetaStore, model_uid: &str) -> Result<ModelDeployment, ServiceError> {
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
        None => return Err(ServiceError::NotFound("deployment not found (model may not be started)".to_string())),
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
        return Err(ServiceError::Conflict(format!("template with id '{tid}' already exists")));
    }

    let now = now_ms();
    let template = ModelTemplate {
        template_id: tid.clone(),
        name: req.name,
        description: req.description,
        category: req.category,
        model_name: req.model_name,
        model_source: req.model_source,
        engine_type: req.engine_type,
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
        return Err(ServiceError::Conflict(format!("model with uid '{uid}' already exists")));
    }

    let now = now_ms();
    let spec = ModelSpec {
        model_uid: uid.clone(),
        model_name: tpl.model_name,
        model_source: tpl.model_source.unwrap_or(ModelSource::HuggingFace),
        model_path: None,
        engine_type: tpl.engine_type,
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
