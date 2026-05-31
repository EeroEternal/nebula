use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::Ordering;
use std::collections::HashMap;

use crate::auth::{require_role, AuthContext, Role};
use crate::service::{
    self, CreateModelRequest, CreateTemplateRequest, DeployTemplateRequest, ListModelsQuery,
    SaveAsTemplateRequest, ScaleModelRequest, ServiceError, StartModelRequest, UpdateModelRequest,
    UpdateTemplateRequest,
};
use crate::state::AppState;

use nebula_meta::MetaStore;
use uuid::Uuid;

use nebula_common::{
    DesiredState, DiskAlert, ModelCacheEntry, ModelDeployment, ModelRequest, ModelRequestStatus,
    ModelSource, ModelSpec, NodeDiskStatus,
};

// ---------------------------------------------------------------------------
// Model CRUD Handlers (Delegated to service.rs)
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
    let deployment = service::start_model(&*st.store, &model_uid, req).await?;
    Ok((StatusCode::OK, Json(json!(deployment))).into_response())
}

pub async fn stop_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let deployment = service::stop_model(&*st.store, &model_uid).await?;
    Ok((StatusCode::OK, Json(json!(deployment))).into_response())
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
    let deployment = service::scale_model(&*st.store, &model_uid, req).await?;
    Ok((StatusCode::OK, Json(json!(deployment))).into_response())
}

// ---------------------------------------------------------------------------
// Template CRUD Handlers (Delegated to service.rs)
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
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    service::delete_template(&*st.store, &id).await?;
    Ok((
        StatusCode::OK,
        Json(json!({"template_id": id, "status": "deleted"})),
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
// Cache / Disk / Alerts Handlers
// ---------------------------------------------------------------------------

pub async fn node_cache(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(node_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }

    let kvs = st
        .store
        .list_prefix(&format!("/model_cache/{node_id}/"))
        .await?;

    let caches: Vec<ModelCacheEntry> = kvs
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

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

    match st.store.get(&format!("/node_disk/{node_id}")).await? {
        Some((data, _)) => {
            let d: NodeDiskStatus = serde_json::from_slice(&data)?;
            Ok((StatusCode::OK, Json(json!(d))).into_response())
        }
        None => Err(ServiceError::NotFound(
            "disk status not found for node".to_string(),
        )),
    }
}

pub async fn cache_summary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }

    let caches: Vec<ModelCacheEntry> = st
        .store
        .list_prefix("/model_cache/")
        .await?
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let nodes: Vec<NodeDiskStatus> = st
        .store
        .list_prefix("/node_disk/")
        .await?
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    let total_size: u64 = caches.iter().map(|c| c.size_bytes).sum();

    let summary = service::CacheSummary {
        total_cached_models: caches.len(),
        total_cache_size_bytes: total_size,
        nodes,
        caches,
    };

    Ok((StatusCode::OK, Json(summary)).into_response())
}

pub async fn list_alerts(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }

    let kvs = st.store.list_prefix("/alerts/").await?;
    let alerts: Vec<DiskAlert> = kvs
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();

    Ok((StatusCode::OK, Json(alerts)).into_response())
}

// ---------------------------------------------------------------------------
// Migration Handlers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MigrationDetail {
    model_uid: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    desired_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Serialize)]
struct MigrationResult {
    total: usize,
    migrated: usize,
    skipped: usize,
    failed: usize,
    details: Vec<MigrationDetail>,
}

pub async fn migrate_v1_to_v2(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }

    let requests_raw = st.store.list_prefix("/model_requests/").await?;
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

        match st.store.get(&format!("/models/{model_uid}/spec")).await {
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

        let now = service::now_ms();

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

        if let Err(e) = service::put_model_spec(&*st.store, model_uid, &spec).await {
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
            version: 1,
            updated_at_ms: now,
        };

        if let Err(e) = service::put_model_deployment(&*st.store, model_uid, &deployment).await {
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

    let result = MigrationResult {
        total,
        migrated,
        skipped,
        failed,
        details,
    };

    Ok((StatusCode::OK, Json(result)).into_response())
}

// ---------------------------------------------------------------------------
// Observability Handlers & Helpers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct GatewayOverviewQuery {
    pub window: Option<String>,
}

#[derive(Serialize)]
pub struct GatewayOverviewResponse {
    pub window: String,
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
    pub series: GatewayReliabilitySeries,
}

#[derive(Serialize)]
pub struct GatewayProtectionResponse {
    pub window: String,
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
    pub series: GatewayLatencySeries,
}

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = json!({
        "error": {
            "code": code.to_string(),
            "message": message.to_string(),
            "request_id": format!("req_{}", Uuid::new_v4()),
        }
    });
    (status, Json(body)).into_response()
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
    match line.as_bytes().get(metric.len()) {
        Some(b' ') | Some(b'{') => true,
        _ => false,
    }
}

fn parse_metric_sum(metrics_text: &str, metric: &str) -> f64 {
    metrics_text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter(|line| metric_line_matches(line, metric))
        .filter_map(|line| line.split_whitespace().last())
        .filter_map(|value| value.parse::<f64>().ok())
        .sum()
}

fn parse_metric_sum_with_label(metrics_text: &str, metric: &str, label: &str, value: &str) -> f64 {
    let token = format!(r#"{label}=\"{value}\""#);
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
    let token = format!(r#"{label}=\""#);
    let start = line.find(&token)? + token.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_histogram_quantile(metrics_text: &str, metric: &str, quantile: f64) -> f64 {
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

async fn fetch_router_metrics_text(st: &AppState) -> Result<String, Response> {
    let metrics_url = format!("{}/metrics", st.router_url.trim_end_matches('/'));
    let resp = match st.http.get(metrics_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            return Err(error_response(
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                &format!("router request failed: {e}"),
            ))
        }
    };

    if !resp.status().is_success() {
        return Err(error_response(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            &format!(
                "router metrics responded with status {}",
                resp.status().as_u16()
            ),
        ));
    }

    match resp.text().await {
        Ok(text) => Ok(text),
        Err(e) => Err(error_response(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            &format!("failed to read router response: {e}"),
        )),
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn normalize_zero(value: f64) -> f64 {
    if value.abs() < 1e-12 {
        0.0
    } else {
        value
    }
}

pub async fn gateway_overview(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return resp;
    }

    let window = query.window.unwrap_or_else(|| "15m".to_string());
    let window_seconds = match parse_window_seconds(&window) {
        Some(v) => v,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "window must be one of: 5m, 15m, 1h, 6h, 24h",
            )
        }
    };

    let text = match fetch_router_metrics_text(&st).await {
        Ok(text) => text,
        Err(resp) => return resp,
    };

    let requests_total = parse_metric_sum(&text, "nebula_router_requests_total");
    let responses_5xx = parse_metric_sum(&text, "nebula_router_responses_5xx");
    let retry_total = parse_metric_sum(&text, "nebula_router_retry_total");
    let retry_success_total = parse_metric_sum(&text, "nebula_router_retry_success_total");
    let circuit_open_total = parse_metric_sum(&text, "nebula_router_circuit_open_total");

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

    let response = GatewayOverviewResponse {
        window,
        rps: normalize_zero(requests_total / window_seconds as f64),
        error_5xx_ratio: normalize_zero(error_5xx_ratio),
        retry_success_ratio: normalize_zero(retry_success_ratio),
        circuit_open_count: circuit_open_total as u64,
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn gateway_traffic(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return resp;
    }

    let window = query.window.unwrap_or_else(|| "1h".to_string());
    let window_seconds = match parse_window_seconds(&window) {
        Some(v) => v,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "window must be one of: 5m, 15m, 1h, 6h, 24h",
            )
        }
    };

    let text = match fetch_router_metrics_text(&st).await {
        Ok(text) => text,
        Err(resp) => return resp,
    };

    let ts = now_rfc3339();
    let to_point = |value: f64| TimePoint {
        ts: ts.clone(),
        value,
    };

    let requests_total = normalize_zero(
        parse_metric_sum(&text, "nebula_router_requests_total") / window_seconds as f64,
    );
    let responses_2xx = normalize_zero(
        parse_metric_sum(&text, "nebula_router_responses_2xx") / window_seconds as f64,
    );
    let responses_4xx = normalize_zero(
        parse_metric_sum(&text, "nebula_router_responses_4xx") / window_seconds as f64,
    );
    let responses_5xx = normalize_zero(
        parse_metric_sum(&text, "nebula_router_responses_5xx") / window_seconds as f64,
    );

    let response = GatewayTrafficResponse {
        window,
        series: GatewayTrafficSeries {
            requests_total: vec![to_point(requests_total)],
            responses_2xx: vec![to_point(responses_2xx)],
            responses_4xx: vec![to_point(responses_4xx)],
            responses_5xx: vec![to_point(responses_5xx)],
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn gateway_reliability(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return resp;
    }

    let window = query.window.unwrap_or_else(|| "1h".to_string());
    let window_seconds = match parse_window_seconds(&window) {
        Some(v) => v,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "window must be one of: 5m, 15m, 1h, 6h, 24h",
            )
        }
    };

    let text = match fetch_router_metrics_text(&st).await {
        Ok(text) => text,
        Err(resp) => return resp,
    };

    let ts = now_rfc3339();
    let to_point = |value: f64| TimePoint {
        ts: ts.clone(),
        value,
    };

    let retry_total = normalize_zero(
        parse_metric_sum(&text, "nebula_router_retry_total") / window_seconds as f64,
    );
    let retry_success_total = normalize_zero(
        parse_metric_sum(&text, "nebula_router_retry_success_total") / window_seconds as f64,
    );
    let upstream_error_connect = normalize_zero(
        parse_metric_sum_with_label(
            &text,
            "nebula_router_upstream_error_total",
            "kind",
            "connect",
        ) / window_seconds as f64,
    );
    let upstream_error_timeout = normalize_zero(
        parse_metric_sum_with_label(
            &text,
            "nebula_router_upstream_error_total",
            "kind",
            "timeout",
        ) / window_seconds as f64,
    );
    let upstream_error_5xx = normalize_zero(
        parse_metric_sum_with_label(&text, "nebula_router_upstream_error_total", "kind", "5xx")
            / window_seconds as f64,
    );
    let upstream_error_other = normalize_zero(
        parse_metric_sum_with_label(&text, "nebula_router_upstream_error_total", "kind", "other")
            / window_seconds as f64,
    );

    let response = GatewayReliabilityResponse {
        window,
        series: GatewayReliabilitySeries {
            retry_total: vec![to_point(retry_total)],
            retry_success_total: vec![to_point(retry_success_total)],
            upstream_error_connect: vec![to_point(upstream_error_connect)],
            upstream_error_timeout: vec![to_point(upstream_error_timeout)],
            upstream_error_5xx: vec![to_point(upstream_error_5xx)],
            upstream_error_other: vec![to_point(upstream_error_other)],
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn gateway_protection(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return resp;
    }

    let window = query.window.unwrap_or_else(|| "15m".to_string());
    if parse_window_seconds(&window).is_none() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "window must be one of: 5m, 15m, 1h, 6h, 24h",
        );
    }

    let text = match fetch_router_metrics_text(&st).await {
        Ok(text) => text,
        Err(resp) => return resp,
    };

    let request_too_large_count =
        parse_metric_sum(&text, "nebula_router_request_too_large_total") as u64;
    let circuit_skipped_count =
        parse_metric_sum(&text, "nebula_router_route_circuit_skipped_total") as u64;
    let circuit_open_count = parse_metric_sum(&text, "nebula_router_circuit_open_total") as u64;

    let response = GatewayProtectionResponse {
        window,
        request_too_large_count,
        circuit_skipped_count,
        circuit_open_count,
    };

    (StatusCode::OK, Json(response)).into_response()
}

pub async fn gateway_latency(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(query): Query<GatewayOverviewQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return resp;
    }

    let window = query.window.unwrap_or_else(|| "1h".to_string());
    if parse_window_seconds(&window).is_none() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "window must be one of: 5m, 15m, 1h, 6h, 24h",
        );
    }

    let text = match fetch_router_metrics_text(&st).await {
        Ok(text) => text,
        Err(resp) => return resp,
    };

    let latency_p50_ms = normalize_zero(
        parse_histogram_quantile(&text, "nebula_route_latency_seconds", 0.50) * 1000.0,
    );
    let latency_p95_ms = normalize_zero(
        parse_histogram_quantile(&text, "nebula_route_latency_seconds", 0.95) * 1000.0,
    );
    let latency_p99_ms = normalize_zero(
        parse_histogram_quantile(&text, "nebula_route_latency_seconds", 0.99) * 1000.0,
    );
    let ttft_p50_ms =
        normalize_zero(parse_histogram_quantile(&text, "nebula_route_ttft_seconds", 0.50) * 1000.0);
    let ttft_p95_ms =
        normalize_zero(parse_histogram_quantile(&text, "nebula_route_ttft_seconds", 0.95) * 1000.0);

    let ts = now_rfc3339();
    let to_point = |value: f64| TimePoint {
        ts: ts.clone(),
        value,
    };

    let response = GatewayLatencyResponse {
        window,
        series: GatewayLatencySeries {
            latency_p50_ms: vec![to_point(latency_p50_ms)],
            latency_p95_ms: vec![to_point(latency_p95_ms)],
            latency_p99_ms: vec![to_point(latency_p99_ms)],
            ttft_p50_ms: vec![to_point(ttft_p50_ms)],
            ttft_p95_ms: vec![to_point(ttft_p95_ms)],
        },
    };

    (StatusCode::OK, Json(response)).into_response()
}
