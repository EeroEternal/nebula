//! `/platform/v1/*` Control API handlers (I1).

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde_json::json;

use nebula_control::{
    create_model, create_operation, get_model, get_model_deployment, get_operation, list_models,
    list_nodes, list_replicas, load_model, scale_model, start_model, stop_model, CreateModelRequest,
    OperationKind, ScaleDeploymentRequest, ServiceError, StartDeploymentRequest,
};

use crate::auth::{require_role, AuthContext, Role};
use crate::control::control_error;
use crate::state::AppState;

fn require_control_read(ctx: &AuthContext, st: &AppState) -> Option<Response> {
    require_role(&st.metrics, ctx, Role::Viewer)
}

fn require_control_write(ctx: &AuthContext, st: &AppState) -> Option<Response> {
    require_role(&st.metrics, ctx, Role::Operator)
}

fn operation_accepted(op: nebula_control::Operation) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "operation_id": op.operation_id,
            "model_uid": op.model_uid,
            "status": op.status,
        })),
    )
        .into_response()
}

pub async fn platform_list_models(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match list_models(&*st.store).await {
        Ok(models) => (StatusCode::OK, Json(json!({ "models": models }))).into_response(),
        Err(e) => control_error(e),
    }
}

pub async fn platform_create_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateModelRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    match create_model(&*st.store, &ctx.principal, req).await {
        Ok(spec) => (StatusCode::CREATED, Json(spec)).into_response(),
        Err(e) => control_error(e),
    }
}

pub async fn platform_get_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match get_model(&*st.store, &model_uid).await {
        Ok(spec) => (StatusCode::OK, Json(spec)).into_response(),
        Err(e) => control_error(e),
    }
}

pub async fn platform_get_deployment(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match get_model_deployment(&*st.store, &model_uid).await {
        Ok(Some(dep)) => (StatusCode::OK, Json(dep)).into_response(),
        Ok(None) => control_error(ServiceError::NotFound(format!(
            "deployment for model '{model_uid}' not found"
        ))),
        Err(e) => control_error(e),
    }
}

pub async fn platform_put_deployment(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<StartDeploymentRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    match start_model(&*st.store, &model_uid, req).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Deploy, &dep).await {
            Ok(op) => operation_accepted(op),
            Err(e) => control_error(e),
        },
        Err(e) => control_error(e),
    }
}

pub async fn platform_stop_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    match stop_model(&*st.store, &model_uid).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Stop, &dep).await {
            Ok(op) => operation_accepted(op),
            Err(e) => control_error(e),
        },
        Err(e) => control_error(e),
    }
}

pub async fn platform_scale_deployment(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<ScaleDeploymentRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    match scale_model(&*st.store, &model_uid, req).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Scale, &dep).await {
            Ok(op) => operation_accepted(op),
            Err(e) => control_error(e),
        },
        Err(e) => control_error(e),
    }
}

/// One-shot load: upsert spec + start deployment (compat-validated).
pub async fn platform_load_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<nebula_common::ModelLoadRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    match load_model(&*st.store, &ctx.principal, req).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Deploy, &dep).await {
            Ok(op) => operation_accepted(op),
            Err(e) => control_error(e),
        },
        Err(e) => control_error(e),
    }
}

pub async fn platform_list_replicas(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match list_replicas(&*st.store, &model_uid).await {
        Ok(replicas) => (StatusCode::OK, Json(json!({ "replicas": replicas }))).into_response(),
        Err(e) => control_error(e),
    }
}

pub async fn platform_list_nodes(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match list_nodes(&*st.store).await {
        Ok(nodes) => (StatusCode::OK, Json(json!({ "nodes": nodes }))).into_response(),
        Err(e) => control_error(e),
    }
}

pub async fn platform_get_operation(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(operation_id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match get_operation(&*st.store, &operation_id).await {
        Ok(op) => (StatusCode::OK, Json(op)).into_response(),
        Err(e) => control_error(e),
    }
}

pub fn apply_legacy_deprecation_headers(headers: &mut HeaderMap) {
    headers.insert(
        "Deprecation",
        "true".parse().expect("valid header value"),
    );
    headers.insert(
        "Link",
        "</platform/v1/models>; rel=\"successor-version\""
            .parse()
            .expect("valid header value"),
    );
}

pub async fn legacy_deprecation_middleware(
    req: axum::http::Request<Body>,
    next: Next,
) -> Response {
    let mut resp = next.run(req).await;
    apply_legacy_deprecation_headers(resp.headers_mut());
    resp
}
