//! `/platform/v1/*` Control API handlers (I1+).

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{
        sse::{Event, KeepAlive},
        IntoResponse, Response, Sse,
    },
    Extension, Json,
};
use serde_json::json;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use nebula_control::{
    cluster_counts, create_model, create_operation, etcd_health, evaluate_slo_from_router_metrics,
    filter_canaries_by_model, get_canary, get_model, get_model_deployment, get_operation, get_slo,
    list_canaries, list_models, list_nodes, list_replicas, load_model, scale_model, start_model,
    stop_model, ComponentHealth, ComponentStatus, CreateModelRequest, HealthSummary, OperationKind,
    OperationStatus, ScaleDeploymentRequest, ServiceError, StartDeploymentRequest,
};

use crate::audit::{fetch_audit_logs, AuditLogQuery};
use crate::auth::{require_role, AuthContext, Role};
use crate::control::control_error;
use crate::platform_idempotency::{check_idempotency, record_idempotency};
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

async fn finish_write(
    st: &AppState,
    ctx: &AuthContext,
    headers: &HeaderMap,
    path: &str,
    body: &[u8],
    op: nebula_control::Operation,
) -> Response {
    if let Err(e) = record_idempotency(&st.store, &ctx.principal, headers, path, body, &op).await {
        return control_error(e);
    }
    operation_accepted(op)
}

pub async fn platform_health_summary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }

    let etcd = etcd_health(&*st.store).await;
    let cluster = match cluster_counts(&*st.store).await {
        Ok(c) => c,
        Err(e) => return control_error(e),
    };

    let router_url = format!(
        "{}/healthz",
        st.router_base_url.trim_end_matches('/')
    );
    let router = match st.http.get(&router_url).send().await {
        Ok(r) if r.status().is_success() => ComponentHealth {
            status: ComponentStatus::Ok,
            message: None,
        },
        Ok(r) => ComponentHealth {
            status: ComponentStatus::Degraded,
            message: Some(format!("router returned {}", r.status())),
        },
        Err(e) => ComponentHealth {
            status: ComponentStatus::Unavailable,
            message: Some(format!("router unreachable: {e}")),
        },
    };

    let summary = HealthSummary {
        gateway: ComponentHealth {
            status: ComponentStatus::Ok,
            message: None,
        },
        etcd,
        router,
        cluster,
    };
    (StatusCode::OK, Json(summary)).into_response()
}

pub async fn platform_audit_logs(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<AuditLogQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Admin) {
        return resp;
    }
    match fetch_audit_logs(&st, &query).await {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(resp) => resp,
    }
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
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    let path = "/platform/v1/models";
    match check_idempotency(&st.store, &ctx.principal, &headers, path, &body).await {
        Ok(Some(r)) => return r,
        Ok(None) => {}
        Err(e) => return control_error(e),
    }
    let req: CreateModelRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return control_error(ServiceError::BadRequest(format!("invalid json: {e}")));
        }
    };
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
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    let path = format!("/platform/v1/models/{model_uid}/deployment");
    match check_idempotency(&st.store, &ctx.principal, &headers, &path, &body).await {
        Ok(Some(r)) => return r,
        Ok(None) => {}
        Err(e) => return control_error(e),
    }
    let req: StartDeploymentRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return control_error(ServiceError::BadRequest(format!("invalid json: {e}")));
        }
    };
    match start_model(&*st.store, &model_uid, req).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Deploy, &dep).await {
            Ok(op) => finish_write(&st, &ctx, &headers, &path, &body, op).await,
            Err(e) => control_error(e),
        },
        Err(e) => control_error(e),
    }
}

pub async fn platform_stop_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    let path = format!("/platform/v1/models/{model_uid}/stop");
    let body: &[u8] = &[];
    match check_idempotency(&st.store, &ctx.principal, &headers, &path, body).await {
        Ok(Some(r)) => return r,
        Ok(None) => {}
        Err(e) => return control_error(e),
    }
    match stop_model(&*st.store, &model_uid).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Stop, &dep).await {
            Ok(op) => finish_write(&st, &ctx, &headers, &path, body, op).await,
            Err(e) => control_error(e),
        },
        Err(e) => control_error(e),
    }
}

pub async fn platform_scale_deployment(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    let path = format!("/platform/v1/models/{model_uid}/deployment/scale");
    match check_idempotency(&st.store, &ctx.principal, &headers, &path, &body).await {
        Ok(Some(r)) => return r,
        Ok(None) => {}
        Err(e) => return control_error(e),
    }
    let req: ScaleDeploymentRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return control_error(ServiceError::BadRequest(format!("invalid json: {e}")));
        }
    };
    match scale_model(&*st.store, &model_uid, req).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Scale, &dep).await {
            Ok(op) => finish_write(&st, &ctx, &headers, &path, &body, op).await,
            Err(e) => control_error(e),
        },
        Err(e) => control_error(e),
    }
}

pub async fn platform_load_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Some(resp) = require_control_write(&ctx, &st) {
        return resp;
    }
    let path = "/platform/v1/models/load";
    match check_idempotency(&st.store, &ctx.principal, &headers, path, &body).await {
        Ok(Some(r)) => return r,
        Ok(None) => {}
        Err(e) => return control_error(e),
    }
    let req: nebula_common::ModelLoadRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return control_error(ServiceError::BadRequest(format!("invalid json: {e}")));
        }
    };
    match load_model(&*st.store, &ctx.principal, req).await {
        Ok(dep) => match create_operation(&*st.store, OperationKind::Deploy, &dep).await {
            Ok(op) => finish_write(&st, &ctx, &headers, path, &body, op).await,
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

pub async fn platform_operation_events(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(operation_id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(16);
    let store = st.store.clone();
    tokio::spawn(async move {
        loop {
            match get_operation(&*store, &operation_id).await {
                Ok(op) => {
                    let terminal = matches!(
                        op.status,
                        OperationStatus::Succeeded | OperationStatus::Failed
                    );
                    let payload = serde_json::to_string(&op).unwrap_or_else(|_| "{}".into());
                    if tx
                        .send(Ok(Event::default().event("operation").data(payload)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                    if terminal {
                        break;
                    }
                }
                Err(e) => {
                    let payload = json!({ "error": e.to_string() }).to_string();
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(payload)))
                        .await;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct CanariesQuery {
    pub model_uid: Option<String>,
}

async fn fetch_router_metrics(st: &AppState) -> String {
    let url = format!("{}/metrics", st.router_base_url.trim_end_matches('/'));
    match st.http.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => resp.text().await.unwrap_or_default(),
        _ => String::new(),
    }
}

pub async fn platform_get_slo(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match get_slo(&*st.store, &model_uid).await {
        Ok(Some(slo)) => (StatusCode::OK, Json(slo)).into_response(),
        Ok(None) => control_error(ServiceError::NotFound(format!(
            "slo for model '{model_uid}' not found"
        ))),
        Err(e) => control_error(e),
    }
}

pub async fn platform_evaluate_slo(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    let slo = match get_slo(&*st.store, &model_uid).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return control_error(ServiceError::NotFound(format!(
                "slo for model '{model_uid}' not found"
            )));
        }
        Err(e) => return control_error(e),
    };
    let metrics = fetch_router_metrics(&st).await;
    let evaluation = evaluate_slo_from_router_metrics(&slo, &metrics);
    (StatusCode::OK, Json(evaluation)).into_response()
}

pub async fn platform_list_canaries(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<CanariesQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match list_canaries(&*st.store).await {
        Ok(canaries) => {
            let filtered = filter_canaries_by_model(canaries, query.model_uid.as_deref());
            (StatusCode::OK, Json(json!({ "canaries": filtered }))).into_response()
        }
        Err(e) => control_error(e),
    }
}

pub async fn platform_get_canary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(canary_id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_control_read(&ctx, &st) {
        return resp;
    }
    match get_canary(&*st.store, &canary_id).await {
        Ok(canary) => (StatusCode::OK, Json(canary)).into_response(),
        Err(e) => control_error(e),
    }
}

/// RFC 8594 sunset for legacy `/v1/admin/*` (removal target v1.6.0).
pub const LEGACY_ADMIN_SUNSET: &str = "Thu, 11 Feb 2027 23:59:59 GMT";

pub fn apply_legacy_deprecation_headers(headers: &mut HeaderMap) {
    headers.insert(
        "Deprecation",
        "true".parse().expect("valid header value"),
    );
    headers.insert(
        "Sunset",
        LEGACY_ADMIN_SUNSET
            .parse()
            .expect("valid header value"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_deprecation_headers_include_sunset() {
        let mut headers = HeaderMap::new();
        apply_legacy_deprecation_headers(&mut headers);
        assert_eq!(headers.get("Deprecation").unwrap(), "true");
        assert_eq!(headers.get("Sunset").unwrap(), LEGACY_ADMIN_SUNSET);
        assert!(headers.get("Link").unwrap().to_str().unwrap().contains("/platform/v1/models"));
    }
}
