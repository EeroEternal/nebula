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

/// `lmcache_controller` lifecycle API for one model (Batch 6.3: warm & pin / clear / lookup).
///
/// Disabled (503) unless a controller base URL is configured; see
/// [`nebula_control::LmcacheController`]. Tokens are supplied by the caller because the
/// control plane has no tokenizer - `warmup` takes a raw prompt and lets the engine do the
/// tokenizing work it would do anyway.
fn cache_controller() -> Result<nebula_control::LmcacheController, ServiceError> {
    nebula_control::LmcacheController::from_env().ok_or_else(|| {
        ServiceError::BadRequest(
            "KV cache control is disabled: set NEBULA_LMCACHE_CONTROLLER_URL (host:port) or NEBULA_LMCACHE_CONTROLLER_BASE_URL"
                .into(),
        )
    })
}

/// Instances this model's replicas registered with (`{model_uid}-{replica_id}`).
///
/// `pin`/`clear` are addressed per instance, and the id must be the exact string the engine
/// registered with - [`nebula_common::lmcache_instance_id`] is the one definition both sides use.
async fn lmcache_instances_for(
    store: &dyn nebula_meta::MetaStore,
    model_uid: &str,
) -> Result<Vec<String>, ServiceError> {
    let raw = store
        .list_prefix(&format!("/endpoints/{model_uid}/"))
        .await
        .map_err(ServiceError::Etcd)?;
    let mut out = Vec::new();
    for (_, v, _) in raw {
        let Ok(ep) = serde_json::from_slice::<nebula_common::EndpointInfo>(&v) else {
            continue;
        };
        if ep.status != nebula_common::EndpointStatus::Ready {
            continue;
        }
        out.push(nebula_common::lmcache_instance_id(model_uid, ep.replica_id));
    }
    out.sort();
    out.dedup();
    Ok(out)
}

#[derive(Debug, serde::Deserialize)]
pub struct CacheLookupRequest {
    /// Prompt tokens to probe. Required: the control plane does not tokenize.
    #[serde(default)]
    pub tokens: Vec<u32>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CachePinRequest {
    #[serde(default)]
    pub tokens: Vec<u32>,
    /// KV tier to pin in; defaults to the L1 host-DRAM backend.
    #[serde(default)]
    pub location: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CacheClearRequest {
    #[serde(default)]
    pub location: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CacheWarmupRequest {
    /// Prompt whose KV should be computed and stored (tokenized by the engine).
    pub prompt: String,
    /// Also pin the prefix after warming it.
    #[serde(default)]
    pub pin: bool,
    #[serde(default)]
    pub location: Option<String>,
}

/// `POST /api/v2/models/{uid}/cache/lookup` — how many tokens of this prefix the store holds.
pub async fn cache_lookup(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<CacheLookupRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp.into_response());
    }
    if req.tokens.is_empty() {
        return Err(ServiceError::BadRequest(
            "tokens must not be empty (the control plane does not tokenize)".into(),
        ));
    }
    let client = cache_controller()?;
    let matched = client.lookup(&model_uid, &req.tokens).await?;
    let instances = lmcache_instances_for(&*st.store, &model_uid).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "matched_tokens": matched, "instances": instances })),
    )
        .into_response())
}

/// `POST /api/v2/models/{uid}/cache/pin` — lock a hot prefix so eviction cannot drop it.
pub async fn cache_pin(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<CachePinRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp.into_response());
    }
    if req.tokens.is_empty() {
        return Err(ServiceError::BadRequest(
            "tokens must not be empty (the control plane does not tokenize)".into(),
        ));
    }
    let location = req
        .location
        .unwrap_or_else(|| nebula_control::DEFAULT_KV_LOCATION.to_string());
    let instances = lmcache_instances_for(&*st.store, &model_uid).await?;
    let client = cache_controller()?;
    let (num_tokens, failures) = client.pin(&instances, &location, &req.tokens).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "pinned_tokens": num_tokens,
            "location": location,
            "instances": instances,
            "failures": failures,
        })),
    )
        .into_response())
}

/// `POST /api/v2/models/{uid}/cache/clear` — wipe this model's cache in a tier.
pub async fn cache_clear(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<CacheClearRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp.into_response());
    }
    let location = req
        .location
        .unwrap_or_else(|| nebula_control::DEFAULT_KV_LOCATION.to_string());
    let instances = lmcache_instances_for(&*st.store, &model_uid).await?;
    let client = cache_controller()?;
    let (num_tokens, failures) = client.clear(&instances, &location).await?;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "cleared_tokens": num_tokens,
            "location": location,
            "instances": instances,
            "failures": failures,
        })),
    )
        .into_response())
}

/// `POST /api/v2/models/{uid}/cache/warmup` — make the engine compute and store this
/// prompt's KV, optionally pinning it afterwards.
///
/// The warm-up request goes through the router like any other inference call: that is the
/// only way to *produce* KV (the controller can only pin what already exists).
pub async fn cache_warmup(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<CacheWarmupRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp.into_response());
    }
    if req.prompt.trim().is_empty() {
        return Err(ServiceError::BadRequest("prompt must not be empty".into()));
    }
    let body = serde_json::json!({
        "model": model_uid,
        "prompt": req.prompt,
        "max_tokens": 1,
        "temperature": 0,
        "stream": false,
    });
    let url = format!("{}/v1/completions", st.router_url.trim_end_matches('/'));
    let resp = st
        .http
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| ServiceError::Upstream(format!("warmup request failed: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(ServiceError::Upstream(format!(
            "warmup inference -> {status}: {}",
            text.chars().take(200).collect::<String>()
        )));
    }
    // Warm-up is a real inference call: it populates the shared store as a side effect.
    // Pinning needs the token ids, which only the engine has, so a pin request is reported
    // as "not applied" rather than silently claimed.
    let location = req
        .location
        .unwrap_or_else(|| nebula_control::DEFAULT_KV_LOCATION.to_string());
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "warmed": true,
            "pinned": false,
            "pin_requested": req.pin,
            "location": location,
            "note": if req.pin {
                "pin requires token ids the control plane does not have; call /cache/pin with tokens"
            } else {
                ""
            },
        })),
    )
        .into_response())
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
    let templates = service::list_templates_db(&st.db).await?;
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
    let template = service::get_model_template_db(&st.db, &id).await?;
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
    let template = service::create_template_db(&st.db, req).await?;
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
    let template = service::update_template_db(&st.db, &id, req).await?;
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
    service::delete_template_db(&st.db, &id).await?;
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
    let spec =
        service::deploy_template(&*st.store, &st.db, ctx.principal.clone(), &id, req).await?;
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
    let template = service::save_as_template(&*st.store, &st.db, &model_uid, req).await?;
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
    let alerts = service::list_alerts(&*st.store).await?;
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

// ---------------------------------------------------------------------------
// P3 Compat / Inventory + P4 SLO / Diagnostics
// ---------------------------------------------------------------------------

pub async fn list_compat_rules(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let rules = crate::compat_slo::list_compat_rules(&*st.store).await?;
    Ok((StatusCode::OK, Json(rules)).into_response())
}

pub async fn put_compat_rule(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(rule): Json<nebula_common::CompatibilityRule>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    let rule = crate::compat_slo::put_compat_rule(&*st.store, rule).await?;
    Ok((StatusCode::OK, Json(rule)).into_response())
}

pub async fn delete_compat_rule(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    crate::compat_slo::delete_compat_rule(&*st.store, &id).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!({}))).into_response())
}

pub async fn seed_compat_rules(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    let rules = crate::compat_slo::seed_default_compat_rules(&*st.store).await?;
    Ok((StatusCode::OK, Json(rules)).into_response())
}

pub async fn hardware_inventory(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let inv = crate::compat_slo::hardware_inventory(&*st.store).await?;
    Ok((StatusCode::OK, Json(inv)).into_response())
}

pub async fn capacity_snapshot(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let snap = crate::compat_slo::capacity_snapshot(&*st.store).await?;
    Ok((StatusCode::OK, Json(snap)).into_response())
}

pub async fn list_slos(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let slos = crate::compat_slo::list_slos(&*st.store).await?;
    Ok((StatusCode::OK, Json(slos)).into_response())
}

pub async fn get_slo(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    match crate::compat_slo::get_slo(&*st.store, &model_uid).await? {
        Some(slo) => Ok((StatusCode::OK, Json(slo)).into_response()),
        None => Err(ServiceError::NotFound(format!(
            "slo for {model_uid} not found"
        ))),
    }
}

pub async fn upsert_slo(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
    Json(req): Json<crate::compat_slo::UpsertSloRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let slo = crate::compat_slo::upsert_slo(&*st.store, &model_uid, req).await?;
    Ok((StatusCode::OK, Json(slo)).into_response())
}

pub async fn delete_slo(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    crate::compat_slo::delete_slo(&*st.store, &model_uid).await?;
    Ok((StatusCode::NO_CONTENT, Json(json!({}))).into_response())
}

pub async fn evaluate_slo(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(model_uid): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let slo = crate::compat_slo::get_slo(&*st.store, &model_uid)
        .await?
        .ok_or_else(|| ServiceError::NotFound(format!("slo for {model_uid} not found")))?;
    let text = service::fetch_router_metrics_text(&st.http, &st.router_url)
        .await
        .unwrap_or_default();
    let ev = crate::compat_slo::evaluate_slo_with_store(&*st.store, &slo, &text).await;
    Ok((StatusCode::OK, Json(ev)).into_response())
}

#[derive(Deserialize)]
pub struct DiagnosticQuery {
    pub model_uid: Option<String>,
}

pub async fn list_diagnostics(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Query(q): Query<DiagnosticQuery>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let events =
        crate::compat_slo::list_diagnostic_events(&*st.store, q.model_uid.as_deref()).await?;
    Ok((StatusCode::OK, Json(events)).into_response())
}

// ---------------------------------------------------------------------------
// P5 Benchmark / Recommend / Canary
// ---------------------------------------------------------------------------

pub async fn list_workloads(
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let workloads = crate::benchmark_svc::list_workloads().await;
    Ok((StatusCode::OK, Json(workloads)).into_response())
}

pub async fn list_benchmark_runs(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let runs = crate::benchmark_svc::list_runs_db(&st.db).await?;
    Ok((StatusCode::OK, Json(runs)).into_response())
}

pub async fn get_benchmark_run(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(run_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let run = crate::benchmark_svc::get_run_db(&st.db, &run_id).await?;
    Ok((StatusCode::OK, Json(run)).into_response())
}

pub async fn ingest_benchmark_run(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(run): Json<nebula_common::BenchmarkRun>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let run = crate::benchmark_svc::ingest_run_db(&st.db, run).await?;
    Ok((StatusCode::CREATED, Json(run)).into_response())
}

pub async fn list_benchmark_profiles(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let profiles = crate::benchmark_svc::list_profiles_db(&st.db).await?;
    Ok((StatusCode::OK, Json(profiles)).into_response())
}

pub async fn recommend_engines(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<nebula_common::RecommendRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let resp = crate::benchmark_svc::recommend_db(&st.db, req).await?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

pub async fn list_canaries(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let items = crate::benchmark_svc::list_canaries(&*st.store).await?;
    Ok((StatusCode::OK, Json(items)).into_response())
}

// ---------------------------------------------------------------------------
// Hardware Pools & Node Drain
// ---------------------------------------------------------------------------

pub async fn list_pools(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let pools = nebula_control::list_pools(&*st.store).await?;
    Ok((StatusCode::OK, Json(pools)).into_response())
}

pub async fn get_pool(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(pool_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let pool = nebula_control::get_pool(&*st.store, &pool_id).await?;
    Ok((StatusCode::OK, Json(pool)).into_response())
}

pub async fn create_pool(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<nebula_control::CreatePoolRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let pool = nebula_control::create_pool(&*st.store, req).await?;
    Ok((StatusCode::CREATED, Json(pool)).into_response())
}

pub async fn update_pool(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(pool_id): Path<String>,
    Json(req): Json<nebula_control::UpdatePoolRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let pool = nebula_control::update_pool(&*st.store, &pool_id, req).await?;
    Ok((StatusCode::OK, Json(pool)).into_response())
}

pub async fn delete_pool(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(pool_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    nebula_control::delete_pool(&*st.store, &pool_id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn drain_node(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(node_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let resp = nebula_control::drain_node(&*st.store, &node_id).await?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

pub async fn create_canary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<crate::benchmark_svc::CreateCanaryRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let c = crate::benchmark_svc::create_canary(&*st.store, req).await?;
    Ok((StatusCode::CREATED, Json(c)).into_response())
}

pub async fn evaluate_canary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(canary_id): Path<String>,
    Json(req): Json<crate::benchmark_svc::EvaluateCanaryRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let c = crate::benchmark_svc::evaluate_canary(&*st.store, &canary_id, req).await?;
    Ok((StatusCode::OK, Json(c)).into_response())
}

pub async fn promote_canary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(canary_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let c = crate::benchmark_svc::promote_canary(&*st.store, &canary_id).await?;
    Ok((StatusCode::OK, Json(c)).into_response())
}

#[derive(Deserialize)]
pub struct RollbackCanaryBody {
    pub reason: Option<String>,
}

pub async fn rollback_canary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(canary_id): Path<String>,
    Json(body): Json<RollbackCanaryBody>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let c = crate::benchmark_svc::rollback_canary(&*st.store, &canary_id, body.reason).await?;
    Ok((StatusCode::OK, Json(c)).into_response())
}

// ---------------------------------------------------------------------------
// L3 Selection (Phase 1)
// ---------------------------------------------------------------------------

pub async fn put_model_profile(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(profile_id): Path<String>,
    Json(mut req): Json<nebula_common::ModelProfile>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    req.profile_id = profile_id;
    let p = crate::selection_svc::put_profile_db(&st.db, req).await?;
    Ok((StatusCode::OK, Json(p)).into_response())
}

pub async fn get_model_profile(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(profile_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let p = crate::selection_svc::get_profile_db(&st.db, &profile_id).await?;
    Ok((StatusCode::OK, Json(p)).into_response())
}

pub async fn selection_recommend(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<nebula_common::SelectionRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let resp = crate::selection_svc::recommend(&*st.store, &st.db, req).await?;
    Ok((StatusCode::OK, Json(resp)).into_response())
}

pub async fn selection_draft(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<nebula_common::DraftRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let draft = crate::selection_svc::draft(&*st.store, &st.db, req).await?;
    Ok((StatusCode::OK, Json(draft)).into_response())
}

pub async fn selection_apply(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<crate::selection_svc::ApplySelectionRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let draft = crate::selection_svc::apply(&*st.store, ctx.principal.clone(), req).await?;
    Ok((StatusCode::OK, Json(draft)).into_response())
}

// ---------------------------------------------------------------------------
// P6 Tenants / Pricing / Usage / Cost
// ---------------------------------------------------------------------------

pub async fn list_tenants(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let items = crate::tenant_svc::list_tenants(&*st.store).await?;
    Ok((StatusCode::OK, Json(items)).into_response())
}

pub async fn get_tenant(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(tenant_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let t = crate::tenant_svc::get_tenant(&*st.store, &tenant_id).await?;
    Ok((StatusCode::OK, Json(t)).into_response())
}

pub async fn upsert_tenant(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<crate::tenant_svc::UpsertTenantRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    let t = crate::tenant_svc::upsert_tenant(&*st.store, req).await?;
    Ok((StatusCode::OK, Json(t)).into_response())
}

pub async fn delete_tenant(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(tenant_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    crate::tenant_svc::delete_tenant(&*st.store, &tenant_id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn list_pricing(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let items = crate::tenant_svc::list_pricing_db(&st.db).await?;
    Ok((StatusCode::OK, Json(items)).into_response())
}

pub async fn upsert_pricing(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<crate::tenant_svc::UpsertPricingRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    let p = crate::tenant_svc::upsert_pricing_db(&st.db, req).await?;
    Ok((StatusCode::OK, Json(p)).into_response())
}

pub async fn delete_pricing(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(price_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Admin) {
        return Ok(resp);
    }
    crate::tenant_svc::delete_pricing_db(&st.db, &price_id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub async fn ingest_usage(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<crate::tenant_svc::IngestUsageRequest>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Operator) {
        return Ok(resp);
    }
    let w = crate::tenant_svc::ingest_usage_db(&st.db, req).await?;
    Ok((StatusCode::OK, Json(w)).into_response())
}

pub async fn list_tenant_usage(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(tenant_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let items = crate::tenant_svc::list_usage_db(&st.db, &tenant_id).await?;
    Ok((StatusCode::OK, Json(items)).into_response())
}

pub async fn tenant_cost_summary(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(tenant_id): Path<String>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let s = crate::tenant_svc::tenant_cost_summary_db(&st.db, &tenant_id).await?;
    Ok((StatusCode::OK, Json(s)).into_response())
}

pub async fn gateway_prefix_affinity(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<impl IntoResponse, ServiceError> {
    if let Some(resp) = require_role(&ctx, Role::Viewer) {
        return Ok(resp);
    }
    let text = service::fetch_router_metrics_text(&st.http, &st.router_url).await?;
    let response = service::prefix_affinity_from_metrics(&text)?;
    Ok((StatusCode::OK, Json(response)).into_response())
}
