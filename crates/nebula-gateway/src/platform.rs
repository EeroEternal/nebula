//! Integration I1: `/platform/v1` Control API handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use nebula_common::ModelLoadRequest;
use nebula_control::{
    ScaleDeploymentRequest, StartDeploymentRequest, UpsertModelSpecRequest,
};
use serde::Deserialize;
use serde_json::json;

use crate::auth::{require_role, AuthContext, Role};
use crate::state::AppState;

fn op_response(op: nebula_control::ControlOperation) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "operation_id": op.operation_id,
            "model_uid": op.model_uid,
            "kind": op.kind,
            "status": op.status,
            "message": op.message,
            "created_at_ms": op.created_at_ms,
            "finished_at_ms": op.finished_at_ms,
        })),
    )
}

pub async fn list_models(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    match nebula_control::list_model_specs(&*st.store).await {
        Ok(models) => (StatusCode::OK, Json(json!({ "models": models }))).into_response(),
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn create_or_update_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<UpsertModelSpecRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let model_uid = req.model_uid.clone();
    match nebula_control::upsert_model_spec(&*st.store, &ctx.principal, req).await {
        Ok(spec) => {
            let op = match nebula_control::record_succeeded_operation(
                &*st.store,
                &model_uid,
                "upsert_spec",
            )
            .await
            {
                Ok(op) => op,
                Err(e) => return crate::control::control_error(e),
            };
            (
                StatusCode::OK,
                Json(json!({
                    "model": spec,
                    "operation_id": op.operation_id,
                    "status": op.status,
                })),
            )
                .into_response()
        }
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn get_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    match nebula_control::get_model_spec(&*st.store, &model_uid).await {
        Ok(spec) => (StatusCode::OK, Json(spec)).into_response(),
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn get_deployment(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    if let Err(e) = nebula_control::require_model_spec(&*st.store, &model_uid).await {
        return crate::control::control_error(e);
    }
    match nebula_control::get_model_deployment(&*st.store, &model_uid).await {
        Ok(Some(dep)) => (StatusCode::OK, Json(dep)).into_response(),
        Ok(None) => crate::control::control_error(nebula_control::ServiceError::NotFound(
            format!("deployment for '{model_uid}' not found"),
        )),
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn put_deployment(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<StartDeploymentRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    match nebula_control::start_model(&*st.store, &model_uid, req).await {
        Ok(dep) => {
            let op = match nebula_control::record_succeeded_operation(
                &*st.store,
                &model_uid,
                "upsert_deployment",
            )
            .await
            {
                Ok(op) => op,
                Err(e) => return crate::control::control_error(e),
            };
            (
                StatusCode::OK,
                Json(json!({
                    "deployment": dep,
                    "operation_id": op.operation_id,
                    "status": op.status,
                })),
            )
                .into_response()
        }
        Err(e) => crate::control::control_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct DeployBody {
    pub model_name: Option<String>,
    #[serde(default = "default_replicas")]
    pub replicas: u32,
    #[serde(default)]
    pub config: Option<nebula_common::ModelConfig>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub gpu_index: Option<u32>,
    #[serde(default)]
    pub gpu_indices: Option<Vec<u32>>,
    #[serde(default)]
    pub min_replicas: Option<u32>,
    #[serde(default)]
    pub max_replicas: Option<u32>,
    #[serde(default)]
    pub engine_type: Option<String>,
    #[serde(default)]
    pub docker_image: Option<String>,
}

fn default_replicas() -> u32 {
    1
}

/// Convenience: upsert spec fields from existing + deploy running (same as Admin load).
pub async fn deploy_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(body): Json<DeployBody>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }

    let model_name = match body.model_name.clone() {
        Some(n) => n,
        None => match nebula_control::get_model_spec(&*st.store, &model_uid).await {
            Ok(spec) => spec.model_name,
            Err(e) => return crate::control::control_error(e),
        },
    };

    let req = ModelLoadRequest {
        model_name,
        model_uid: model_uid.clone(),
        replicas: body.replicas,
        config: body.config,
        node_id: body.node_id,
        gpu_index: body.gpu_index,
        gpu_indices: body.gpu_indices,
        min_replicas: body.min_replicas,
        max_replicas: body.max_replicas,
        engine_type: body.engine_type,
        docker_image: body.docker_image,
    };

    match nebula_control::load_model(&*st.store, &ctx.principal, req).await {
        Ok(dep) => {
            let op = match nebula_control::record_succeeded_operation(
                &*st.store,
                &model_uid,
                "deploy",
            )
            .await
            {
                Ok(op) => op,
                Err(e) => return crate::control::control_error(e),
            };
            (
                StatusCode::OK,
                Json(json!({
                    "deployment": dep,
                    "operation_id": op.operation_id,
                    "status": op.status,
                })),
            )
                .into_response()
        }
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn stop_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    match nebula_control::stop_model(&*st.store, &model_uid).await {
        Ok(dep) => {
            let op =
                match nebula_control::record_succeeded_operation(&*st.store, &model_uid, "stop")
                    .await
                {
                    Ok(op) => op,
                    Err(e) => return crate::control::control_error(e),
                };
            (
                StatusCode::OK,
                Json(json!({
                    "deployment": dep,
                    "operation_id": op.operation_id,
                    "status": op.status,
                })),
            )
                .into_response()
        }
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn scale_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(body): Json<ScaleDeploymentRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    match nebula_control::scale_model(&*st.store, &model_uid, body).await {
        Ok(dep) => {
            let op =
                match nebula_control::record_succeeded_operation(&*st.store, &model_uid, "scale")
                    .await
                {
                    Ok(op) => op,
                    Err(e) => return crate::control::control_error(e),
                };
            (
                StatusCode::OK,
                Json(json!({
                    "deployment": dep,
                    "operation_id": op.operation_id,
                    "status": op.status,
                })),
            )
                .into_response()
        }
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn list_replicas(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    if let Err(e) = nebula_control::require_model_spec(&*st.store, &model_uid).await {
        return crate::control::control_error(e);
    }
    match nebula_control::list_endpoints_for_model(&*st.store, &model_uid).await {
        Ok(replicas) => (StatusCode::OK, Json(json!({ "replicas": replicas }))).into_response(),
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn list_nodes(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    match nebula_control::list_node_statuses(&*st.store).await {
        Ok(nodes) => (StatusCode::OK, Json(json!({ "nodes": nodes }))).into_response(),
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn get_operation(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(operation_id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    match nebula_control::get_operation(&*st.store, &operation_id).await {
        Ok(op) => op_response(op).into_response(),
        Err(e) => crate::control::control_error(e),
    }
}
