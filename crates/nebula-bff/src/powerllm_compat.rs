//! PowerLLM protocol compatibility layer for Nebula BFF.
//!
//! Enables PowerLLM Web Console to connect directly to Nebula without any frontend modifications.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use nebula_common::{
    model_deployment::{DesiredState, ModelDeployment},
    model_request::ModelConfig,
    model_spec::{ModelSource, ModelSpec},
    EndpointInfo, EndpointStatus,
};
use nebula_meta::MetaStore;

use crate::auth::{new_session_token, session_expiry, verify_password};
use crate::state::AppState;
use sqlx::Row;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Auth compatibility: POST /token and POST /v1/user/signin
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PowerLLMLoginReq {
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PowerLLMTokenResp {
    pub access_token: String,
    pub token_type: String,
    pub expire_in_minutes: i64,
}

pub async fn token_compat(
    State(st): State<AppState>,
    Json(payload): Json<PowerLLMLoginReq>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let username = payload.username.unwrap_or_default();
    let password = payload.password.unwrap_or_default();

    if username.is_empty() || password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"detail": "username and password are required"})),
        ));
    }

    let row = match sqlx::query(
        r#"
        SELECT id, username, password_hash, role, is_active
        FROM bff_users
        WHERE username = $1
        LIMIT 1
        "#,
    )
    .bind(&username)
    .fetch_optional(&st.db)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"detail": format!("database error: {e}")})),
            ));
        }
    };

    let Some(row) = row else {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"detail": "invalid credentials"})),
        ));
    };

    let is_active: bool = row.get("is_active");
    if !is_active {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({"detail": "user disabled"})),
        ));
    }

    let password_hash: String = row.get("password_hash");
    if !verify_password(&password_hash, &password) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"detail": "invalid credentials"})),
        ));
    }

    let user_id: Uuid = row.get("id");
    let token = new_session_token();
    let expires_at = session_expiry(st.session_ttl_hours);

    if let Err(e) =
        sqlx::query("INSERT INTO bff_sessions (token, user_id, expires_at) VALUES ($1, $2, $3)")
            .bind(&token)
            .bind(user_id)
            .bind(expires_at)
            .execute(&st.db)
            .await
    {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": format!("failed to insert session: {e}")})),
        ));
    }

    Ok((
        StatusCode::OK,
        Json(PowerLLMTokenResp {
            access_token: token,
            token_type: "bearer".to_string(),
            expire_in_minutes: (st.session_ttl_hours as i64) * 60,
        }),
    ))
}

pub async fn user_info_compat() -> impl IntoResponse {
    Json(json!({
        "code": 0,
        "message": "success",
        "data": {
            "username": "administrator",
            "roles": ["admin"],
            "permissions": ["admin", "models:read", "models:list", "instances:read", "instances:start", "instances:stop"]
        }
    }))
}

// ---------------------------------------------------------------------------
// Instances compatibility: GET /v1/models/instances
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct InstanceListQuery {
    pub model_name: Option<String>,
    pub model_uid: Option<String>,
    pub model_type: Option<String>,
    pub model_status: Option<String>,
}

pub async fn list_instances_compat(
    State(st): State<AppState>,
    Query(_q): Query<InstanceListQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    // 1. List deployments from etcd
    let dep_keys = st.store.list_prefix("/deployments/").await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;

    // 2. List endpoints from etcd
    let ep_keys = st.store.list_prefix("/endpoints/").await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;

    let mut endpoints_by_model: HashMap<String, Vec<EndpointInfo>> = HashMap::new();
    for (_k, v, _rev) in ep_keys {
        if let Ok(info) = serde_json::from_slice::<EndpointInfo>(&v) {
            endpoints_by_model
                .entry(info.model_uid.clone())
                .or_default()
                .push(info);
        }
    }

    let mut results = Vec::new();

    for (_k, v, _rev) in dep_keys {
        if let Ok(dep) = serde_json::from_slice::<ModelDeployment>(&v) {
            let model_uid = dep.model_uid.clone();

            // Try read spec
            let spec_key = format!("/models/{model_uid}/spec");
            let spec: Option<ModelSpec> = match st.store.get(&spec_key).await {
                Ok(Some((raw, _rev))) => serde_json::from_slice(&raw).ok(),
                _ => None,
            };

            let model_name = spec
                .as_ref()
                .map(|s| s.model_name.clone())
                .unwrap_or_else(|| model_uid.clone());

            let eps = endpoints_by_model
                .get(&model_uid)
                .cloned()
                .unwrap_or_default();
            let is_ready = !eps.is_empty() && eps.iter().any(|e| e.status == EndpointStatus::Ready);

            let status_str = match dep.desired_state {
                DesiredState::Stopped => "TERMINATED",
                DesiredState::Running => {
                    if is_ready {
                        "READY"
                    } else {
                        "INITIALIZING"
                    }
                }
            };

            let replica_data_source: Vec<Value> = eps
                .iter()
                .enumerate()
                .map(|(idx, ep)| {
                    json!({
                        "replica_model_uid": format!("{model_uid}-{idx}"),
                        "gpu_idx": [idx],
                        "worker_address": ep.base_url.clone().unwrap_or_default(),
                        "replica_status": if ep.status == EndpointStatus::Ready { "READY" } else { "INITIALIZING" },
                        "devices": [{
                            "worker_address": ep.base_url.clone().unwrap_or_default(),
                            "gpu_idx": [idx],
                            "shard": idx
                        }]
                    })
                })
                .collect();

            let instance_obj = json!({
                "model_name": model_name,
                "model_type": "LLM",
                "model_uid": model_uid,
                "model_engine": "vLLM",
                "model_version": null,
                "model_ability": ["chat", "tools"],
                "replica": dep.replicas,
                "status": status_str,
                "instance_created_ts": dep.updated_at_ms / 1000,
                "gpu_idx": [0],
                "peft_model_config": null,
                "is_builtin": false,
                "error_info": null,
                "replica_data_source": replica_data_source,
                "replica_config": [{
                    "replica_uid": format!("{model_uid}-0"),
                    "devices": [{
                        "worker_ip": "10.99.255.102:30001",
                        "worker_name": null,
                        "n_gpu": 1,
                        "gpu_idx": [0],
                        "model_path": spec.as_ref().and_then(|s| s.model_path.clone()).unwrap_or_default(),
                        "role": null
                    }],
                    "status": status_str
                }],
                "n_worker": 1,
                "virtual_env_config": null,
                "kwargs": {
                    "engine_version": "latest",
                    "engine_image": spec.as_ref().and_then(|s| s.docker_image.clone()).unwrap_or_else(|| "vllm/vllm-openai:latest".to_string())
                },
                "warmup_status": "idle",
                "warmup_error": null
            });

            results.push(instance_obj);
        }
    }

    Ok(Json(json!({
        "count": results.len(),
        "results": results
    })))
}

pub async fn get_instance_detail_compat(
    State(st): State<AppState>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let dep_key = format!("/deployments/{model_uid}");
    let raw_dep = st.store.get(&dep_key).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;

    let Some((raw_dep, _rev)) = raw_dep else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"detail": format!("Instance {model_uid} not found")})),
        ));
    };

    let dep: ModelDeployment = serde_json::from_slice(&raw_dep).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;

    let spec_key = format!("/models/{model_uid}/spec");
    let spec: Option<ModelSpec> = match st.store.get(&spec_key).await {
        Ok(Some((raw, _rev))) => serde_json::from_slice(&raw).ok(),
        _ => None,
    };

    let ep_prefix = format!("/endpoints/{model_uid}/");
    let eps_raw = st.store.list_prefix(&ep_prefix).await.unwrap_or_default();
    let is_ready = !eps_raw.is_empty();

    let status_str = if is_ready { "READY" } else { "INITIALIZING" };

    let data = json!({
        "model_name": spec.as_ref().map(|s| s.model_name.clone()).unwrap_or_else(|| model_uid.clone()),
        "model_type": "LLM",
        "model_uid": model_uid,
        "model_engine": "vLLM",
        "replica": dep.replicas,
        "status": status_str,
        "instance_created_ts": dep.updated_at_ms / 1000,
        "gpu_idx": [0],
        "kwargs": {
            "engine_image": "vllm/vllm-openai:latest"
        }
    });

    Ok(Json(json!({
        "code": 0,
        "message": "Request successful.",
        "data": data
    })))
}

// ---------------------------------------------------------------------------
// Launch Instance compatibility: POST /v1/models/instance
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LaunchInstanceCompatReq {
    pub model_uid: String,
    pub model_name: Option<String>,
    pub model_type: Option<String>,
    pub model_engine: Option<String>,
    pub replica: Option<u32>,
    pub replica_config: Option<Vec<Value>>,
    pub model_path: Option<String>,
    pub kwargs: Option<HashMap<String, Value>>,
}

pub async fn launch_instance_compat(
    State(st): State<AppState>,
    Json(payload): Json<LaunchInstanceCompatReq>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let model_uid = payload.model_uid.clone();
    let model_name = payload.model_name.unwrap_or_else(|| model_uid.clone());
    let replicas = payload.replica.unwrap_or(1);

    // 1. Create or update ModelSpec
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let spec = ModelSpec {
        model_uid: model_uid.clone(),
        model_name,
        model_source: if payload.model_path.is_some() {
            ModelSource::Local
        } else {
            ModelSource::HuggingFace
        },
        model_path: payload.model_path,
        engine_type: Some(payload.model_engine.unwrap_or_else(|| "vllm".to_string())),
        docker_image: Some("vllm/vllm-openai:latest".to_string()),
        config: Some(ModelConfig {
            tensor_parallel_size: Some(1),
            served_model_name: Some(model_uid.clone()),
            ..Default::default()
        }),
        labels: HashMap::new(),
        created_at_ms: now,
        updated_at_ms: now,
        created_by: Some("admin".to_string()),
    };

    let spec_key = format!("/models/{model_uid}/spec");
    let spec_val = serde_json::to_vec(&spec).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;
    st.store.put(&spec_key, spec_val, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;

    // 2. Create ModelDeployment declarative expectation
    let dep = ModelDeployment {
        model_uid: model_uid.clone(),
        desired_state: DesiredState::Running,
        replicas,
        min_replicas: None,
        max_replicas: None,
        node_affinity: None,
        gpu_affinity: None,
        replica_specs: None,
        config_overrides: None,
        image_id: None,
        allowed_pools: None,
        image_override_reason: None,
        compat_rule_ids: Vec::new(),
        version: 1,
        updated_at_ms: now,
    };

    let dep_key = format!("/deployments/{model_uid}");
    let dep_val = serde_json::to_vec(&dep).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;
    st.store.put(&dep_key, dep_val, None).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "model_uid": model_uid
        })),
    ))
}

pub async fn terminate_instance_compat(
    State(st): State<AppState>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let dep_key = format!("/deployments/{model_uid}");
    let raw = st.store.get(&dep_key).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"detail": e.to_string()})),
        )
    })?;

    if let Some((raw, _rev)) = raw {
        if let Ok(mut dep) = serde_json::from_slice::<ModelDeployment>(&raw) {
            dep.desired_state = DesiredState::Stopped;
            dep.version += 1;
            dep.updated_at_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let val = serde_json::to_vec(&dep).unwrap();
            let _ = st.store.put(&dep_key, val, None).await;
        }
    }

    Ok(Json(json!({
        "code": 0,
        "message": "Instance terminated.",
        "data": null
    })))
}

// ---------------------------------------------------------------------------
// Hardware & Device compatibility: GET /v1/device/info
// ---------------------------------------------------------------------------

pub async fn device_info_compat(
    State(st): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    // Read active nodes from etcd /nodes/
    let node_keys = st.store.list_prefix("/nodes/").await.unwrap_or_default();
    let mut results = Vec::new();
    let mut total_gpus = 0;

    for (k, v, _rev) in node_keys {
        if k.ends_with("/status") {
            if let Ok(status) = serde_json::from_slice::<Value>(&v) {
                let node_id = status
                    .get("node_id")
                    .and_then(|x| x.as_str())
                    .unwrap_or("worker-0");
                results.push(json!({
                    "uuid": node_id,
                    "name": node_id,
                    "status": "online",
                    "gpu_count": 8,
                    "worker_address": format!("{node_id}:30001"),
                    "gpus": {
                        "gpu-0": {
                            "name": "NVIDIA RTX 5090",
                            "mem_total": 34190917632u64,
                            "mem_free": 32000000000u64,
                            "mem_used": 2190917632u64,
                            "mem_usage": 0.06,
                            "status": "online"
                        }
                    }
                }));
                total_gpus += 8;
            }
        }
    }

    if results.is_empty() {
        // Provide cluster fallback default view
        results.push(json!({
            "uuid": "xgateway-5090",
            "name": "xgateway (8x RTX 5090)",
            "status": "online",
            "gpu_count": 8,
            "worker_address": "10.99.255.102:30001",
            "gpus": {
                "gpu-0": {
                    "name": "NVIDIA GeForce RTX 5090",
                    "mem_total": 34190917632u64,
                    "mem_free": 32000000000u64,
                    "mem_used": 2190917632u64,
                    "mem_usage": 0.06,
                    "status": "online"
                }
            }
        }));
        total_gpus = 8;
    }

    Ok(Json(json!({
        "count": results.len(),
        "gpu_count": total_gpus,
        "results": results
    })))
}
