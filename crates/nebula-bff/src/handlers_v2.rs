//! v2 HTTP handlers: thin envelopes over [`crate::service`].

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::auth::{require_role, AuthContext, Role};
use crate::service::{
    self, CreateModelRequest, CreateTemplateRequest, DeployTemplateRequest, ListModelsQuery,
    SaveAsTemplateRequest, ScaleModelRequest, ServiceError, StartModelRequest, UpdateModelRequest,
    UpdateTemplateRequest,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Model CRUD
// ---------------------------------------------------------------------------

pub async fn create_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateModelRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let spec = service::create_model(&*st.store, ctx.principal.clone(), req).await?;
    Ok((StatusCode::CREATED, Json(spec)).into_response())
}

pub async fn list_models(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(params): Query<ListModelsQuery>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let views = service::list_models(&*st.store, params).await?;
    Ok((StatusCode::OK, Json(views)).into_response())
}

pub async fn get_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let detail = service::get_model_detail(&*st.store, &model_uid).await?;
    Ok((StatusCode::OK, Json(detail)).into_response())
}

pub async fn update_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<UpdateModelRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let spec = service::update_model(&*st.store, &model_uid, req).await?;
    Ok((StatusCode::OK, Json(spec)).into_response())
}

pub async fn delete_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    let queued_gc_nodes = service::delete_model(&*st.store, &model_uid).await?;
    Ok((
        StatusCode::OK,
        Json(json!({
            "model_uid": model_uid,
            "status": "deleted",
            "queued_gc_nodes": queued_gc_nodes
        })),
    )
        .into_response())
}

pub async fn start_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<StartModelRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let dep = service::start_model(&*st.store, &model_uid, req).await?;
    Ok((StatusCode::OK, Json(dep)).into_response())
}

pub async fn stop_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let dep = service::stop_model(&*st.store, &model_uid).await?;
    Ok((StatusCode::OK, Json(dep)).into_response())
}

pub async fn scale_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<ScaleModelRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let dep = service::scale_model(&*st.store, &model_uid, req).await?;
    Ok((StatusCode::OK, Json(dep)).into_response())
}

// ---------------------------------------------------------------------------
// Templates
// ---------------------------------------------------------------------------

pub async fn list_templates(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let templates = service::list_templates(&*st.store).await?;
    Ok((StatusCode::OK, Json(templates)).into_response())
}

pub async fn get_template(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let template = service::get_model_template(&*st.store, &id).await?;
    Ok((StatusCode::OK, Json(template)).into_response())
}

pub async fn create_template(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateTemplateRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let template = service::create_template(&*st.store, req).await?;
    Ok((StatusCode::CREATED, Json(template)).into_response())
}

pub async fn update_template(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTemplateRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let template = service::update_template(&*st.store, &id, req).await?;
    Ok((StatusCode::OK, Json(template)).into_response())
}

pub async fn delete_template(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    service::delete_template(&*st.store, &id).await?;
    Ok((
        StatusCode::OK,
        Json(json!({"status": "deleted", "template_id": id})),
    )
        .into_response())
}

pub async fn deploy_template(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(req): Json<DeployTemplateRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let spec = service::deploy_template(&*st.store, ctx.principal.clone(), &id, req).await?;
    Ok((StatusCode::CREATED, Json(spec)).into_response())
}

pub async fn save_as_template(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<SaveAsTemplateRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let template = service::save_as_template(&*st.store, &model_uid, req).await?;
    Ok((StatusCode::CREATED, Json(template)).into_response())
}

// ---------------------------------------------------------------------------
// Cache / disk / alerts
// ---------------------------------------------------------------------------

pub async fn node_cache(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(node_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let caches = service::list_node_cache(&*st.store, &node_id).await?;
    Ok((StatusCode::OK, Json(caches)).into_response())
}

pub async fn node_disk(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(node_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let disk = service::get_node_disk(&*st.store, &node_id).await?;
    Ok((StatusCode::OK, Json(disk)).into_response())
}

pub async fn cache_summary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let summary = service::build_cache_summary(&*st.store).await?;
    Ok((StatusCode::OK, Json(summary)).into_response())
}

pub async fn list_alerts(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let alerts = service::list_disk_alerts(&*st.store).await?;
    Ok((StatusCode::OK, Json(alerts)).into_response())
}

// ---------------------------------------------------------------------------
// Migration
// ---------------------------------------------------------------------------

pub async fn migrate_v1_to_v2(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    let result = service::migrate_v1_to_v2(&*st.store).await?;
    Ok((StatusCode::OK, Json(result)).into_response())
}

// ---------------------------------------------------------------------------
// Gateway observability (envelope only)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct GatewayOverviewQuery {
    pub window: Option<String>,
}

pub async fn gateway_overview(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let window = query.window.unwrap_or_else(|| "15m".to_string());
    let text = service::fetch_router_metrics_text(&st.http, &st.router_url).await?;
    let response = service::gateway_overview_from_metrics(&text, window)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

pub async fn gateway_traffic(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let window = query.window.unwrap_or_else(|| "1h".to_string());
    let text = service::fetch_router_metrics_text(&st.http, &st.router_url).await?;
    let response = service::gateway_traffic_from_metrics(&text, window)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

pub async fn gateway_reliability(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let window = query.window.unwrap_or_else(|| "1h".to_string());
    let text = service::fetch_router_metrics_text(&st.http, &st.router_url).await?;
    let response = service::gateway_reliability_from_metrics(&text, window)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

pub async fn gateway_protection(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let window = query.window.unwrap_or_else(|| "15m".to_string());
    let text = service::fetch_router_metrics_text(&st.http, &st.router_url).await?;
    let response = service::gateway_protection_from_metrics(&text, window)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}

pub async fn gateway_latency(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let window = query.window.unwrap_or_else(|| "1h".to_string());
    let text = service::fetch_router_metrics_text(&st.http, &st.router_url).await?;
    let response = service::gateway_latency_from_metrics(&text, window)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}
