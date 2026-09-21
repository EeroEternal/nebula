//! Shared Gateway→Router admission, header injection, and POST helpers.

use axum::{
    body::Body,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use nebula_common::{
    build_execution_context, inject_execution_context, parse_and_sanitize_inference_hint,
    peek_json_model_field, Tenant, TenantDenyCode, HEADER_HINT_TRUSTED, HEADER_INFERENCE_HINT,
};
use nebula_meta::MetaStore;
use serde_json::Value;

use crate::auth::AuthContext;
use crate::interface::{maybe_normalize_router_error, upstream_transport_error};
use crate::state::AppState;

pub struct PreparedUpstream {
    #[allow(dead_code)] // retained for downstream adapters that inspect the upstream model
    pub model: Option<String>,
    pub request_id: String,
    pub headers: reqwest::header::HeaderMap,
    /// Execution context built from auth + headers; used by embedded routing.
    pub ctx: nebula_common::ExecutionContext,
    /// Held for the request lifetime when multi-tenant admission is active.
    pub _conc_guard: Option<nebula_common::admission::ConcurrencyGuard>,
}

pub async fn prepare_upstream(
    st: &AppState,
    auth: &AuthContext,
    headers: &HeaderMap,
    body_bytes: &[u8],
) -> Result<PreparedUpstream, Response> {
    let model = peek_json_model_field(body_bytes);
    if headers.contains_key(HEADER_INFERENCE_HINT) {
        st.metrics
            .hint_received_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let ctx = build_execution_context(headers, auth.tenant_id.as_deref(), None);

    let _conc_guard = if st.auth.env.multi_tenant {
        if let Some(ref tenant_id) = ctx.tenant_id {
            match load_tenant(&*st.store, tenant_id).await {
                Ok(Some(tenant)) => {
                    if ctx.pinned_replica_id.is_some() {
                        if let Err(code) = st.tenant_admission.try_admit_pin(&tenant).await {
                            st.metrics.record_tenant_deny(code.as_str());
                            return Err(deny_response(code));
                        }
                    }
                    let est = ctx.budget_tokens.unwrap_or(0);
                    match st
                        .tenant_admission
                        .try_admit(&tenant, model.as_deref(), est)
                        .await
                    {
                        Ok(g) => Some(g),
                        Err(code) => {
                            st.metrics.record_tenant_deny(code.as_str());
                            return Err(deny_response(code));
                        }
                    }
                }
                Ok(None) => None,
                Err(e) => {
                    tracing::warn!(error=%e, tenant_id=%tenant_id, "tenant lookup failed; allowing request");
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let hint_trusted = matches!(
        auth.role,
        nebula_common::auth::Role::Operator | nebula_common::auth::Role::Admin
    );
    let mut outbound = headers.clone();
    if let Some(raw_hint) = headers
        .get(HEADER_INFERENCE_HINT)
        .and_then(|v| v.to_str().ok())
    {
        if let Some(hint) = parse_and_sanitize_inference_hint(raw_hint) {
            if hint_trusted {
                if let Ok(hint_text) = serde_json::to_string(&hint) {
                    if let Ok(v) = HeaderValue::from_str(&hint_text) {
                        outbound.insert(HeaderName::from_static(HEADER_INFERENCE_HINT), v);
                        st.metrics
                            .hint_forwarded_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            } else {
                outbound.remove(HeaderName::from_static(HEADER_INFERENCE_HINT));
                st.metrics
                    .hint_untrusted_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        } else {
            outbound.remove(HeaderName::from_static(HEADER_INFERENCE_HINT));
            st.metrics
                .hint_rejected_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    outbound.insert(
        HeaderName::from_static(HEADER_HINT_TRUSTED),
        HeaderValue::from_static(if hint_trusted { "1" } else { "0" }),
    );

    if let Some(ref model) = model {
        if let Ok(v) = HeaderValue::from_str(model) {
            outbound.insert(
                HeaderName::from_static(nebula_common::HEADER_NEBULA_MODEL),
                v,
            );
        }
    }
    inject_execution_context(&mut outbound, &ctx);
    nebula_common::telemetry::inject_trace_context(&mut outbound);
    let req_headers = to_reqwest_headers(&outbound);

    Ok(PreparedUpstream {
        model,
        request_id: ctx.request_id.clone(),
        headers: req_headers,
        ctx,
        _conc_guard,
    })
}

pub async fn prepare_upstream_from_json(
    st: &AppState,
    auth: &AuthContext,
    headers: &HeaderMap,
    chat_body: &Value,
) -> Result<PreparedUpstream, Response> {
    let bytes = serde_json::to_vec(chat_body).unwrap_or_default();
    prepare_upstream(st, auth, headers, &bytes).await
}

pub async fn post_router_path(
    st: &AppState,
    url: &str,
    headers: reqwest::header::HeaderMap,
    body: Bytes,
) -> Result<reqwest::Response, Response> {
    match st.http.post(url).headers(headers).body(body).send().await {
        Ok(r) => Ok(r),
        Err(e) => {
            let kind = classify_reqwest_error(&e);
            st.metrics.record_upstream_error(kind);
            tracing::error!(error=%e, "upstream request failed");
            Err(upstream_transport_error(
                kind,
                format!("upstream request failed: {kind}"),
            ))
        }
    }
}

pub async fn post_router_chat(
    st: &AppState,
    prepared: &PreparedUpstream,
    chat_body: &Value,
) -> Result<reqwest::Response, Response> {
    let body = Bytes::from(serde_json::to_vec(chat_body).unwrap_or_default());
    if st.router.is_some() {
        return forward_embedded(
            st,
            prepared,
            reqwest::Method::POST,
            "/v1/chat/completions",
            "",
            &body,
        )
        .await;
    }
    let url = format!(
        "{}/v1/chat/completions",
        st.router_base_url.trim_end_matches('/')
    );
    let mut req_headers = prepared.headers.clone();
    req_headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    post_router_path(st, &url, req_headers, body).await
}

pub async fn load_tenant(store: &dyn MetaStore, tenant_id: &str) -> anyhow::Result<Option<Tenant>> {
    match store.get(&format!("/tenants/{tenant_id}")).await? {
        Some((data, _)) => Ok(serde_json::from_slice(&data).ok()),
        None => Ok(None),
    }
}

pub fn deny_response(code: TenantDenyCode) -> Response {
    let mut resp = nebula_common::auth::tenant_denied(code.as_str(), code.message());
    if let Ok(v) = HeaderValue::from_str(code.as_str()) {
        resp.headers_mut()
            .insert(HeaderName::from_static("x-nebula-deny-code"), v);
    }
    resp
}

pub fn to_reqwest_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
    for (k, v) in headers.iter() {
        if k.as_str().eq_ignore_ascii_case("host")
            || k.as_str().eq_ignore_ascii_case("content-length")
        {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    out
}

pub fn append_headers(src: &reqwest::header::HeaderMap, dst: &mut Response) {
    for (k, v) in src.iter() {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_str().as_bytes()),
            HeaderValue::from_bytes(v.as_bytes()),
        ) {
            dst.headers_mut().insert(name, value);
        }
    }
}

pub fn classify_reqwest_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "timeout";
    }
    if error.is_connect() {
        return "connect";
    }
    "other"
}

/// Forward a non-SSE or SSE upstream response to the client (shared by proxy_post).
pub async fn forward_upstream_response(
    st: &AppState,
    resp: reqwest::Response,
    request_id: Option<&str>,
    req_start: Option<std::time::Instant>,
) -> Response {
    use std::convert::Infallible;
    use tokio::sync::mpsc;
    use tokio_stream::{wrappers::ReceiverStream, StreamExt};

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let is_sse = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("text/event-stream"))
        .unwrap_or(false);
    let resp_headers = resp.headers().clone();

    if is_sse {
        let mut upstream = resp.bytes_stream();
        let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(64);
        let metrics = st.metrics.clone();
        let stage_timing = st.stage_timing;
        let t_upstream = std::time::Instant::now();
        let request_id_owned = request_id.map(|s| s.to_string());
        tokio::spawn(async move {
            let mut first = true;
            loop {
                tokio::select! {
                    biased;
                    _ = tx.closed() => {
                        metrics
                            .requests_aborted_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tracing::info!("gateway SSE aborted: client disconnected");
                        break;
                    }
                    item = upstream.next() => {
                        match item {
                            Some(Ok(b)) => {
                                if first {
                                    first = false;
                                    if stage_timing {
                                        tracing::info!(
                                            target: "gateway_stage",
                                            request_id = %request_id_owned.as_deref().unwrap_or(""),
                                            upstream_hdr_to_first_chunk_us = t_upstream.elapsed().as_micros() as u64,
                                            recv_to_first_chunk_us = req_start.map(|t| t.elapsed().as_micros() as u64).unwrap_or(0),
                                            "stage timing (first chunk)"
                                        );
                                    }
                                }
                                if tx.send(Ok(b)).await.is_err() {
                                    metrics
                                        .requests_aborted_total
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    tracing::info!("gateway SSE aborted: client dropped body");
                                    break;
                                }
                            }
                            Some(Err(_)) | None => break,
                        }
                    }
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        let mut out = Response::builder()
            .status(status)
            .header("content-type", "text/event-stream")
            .body(Body::from_stream(stream))
            .unwrap_or_else(|_| Response::new(Body::empty()));
        append_headers(&resp_headers, &mut out);
        inject_nebula_echo_headers(&mut out, request_id);
        return out;
    }

    let bytes = match resp.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(error=%e, "failed to read upstream response body");
            Bytes::new()
        }
    };
    if let Some(normalized) = maybe_normalize_router_error(status, &bytes) {
        return normalized;
    }
    let mut out = Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    append_headers(&resp_headers, &mut out);
    inject_nebula_echo_headers(&mut out, request_id);
    out
}

fn inject_nebula_echo_headers(out: &mut Response, request_id: Option<&str>) {
    use axum::http::{HeaderName, HeaderValue};
    use nebula_common::HEADER_REQUEST_ID;

    if let Some(rid) = request_id {
        if let Ok(v) = HeaderValue::from_str(rid) {
            out.headers_mut()
                .insert(HeaderName::from_static(HEADER_REQUEST_ID), v);
        }
    }
}

/// Embedded-router resolution.
///
/// When `AppState.router` is set (`NEBULA_ROUTER_MODE=embedded`), resolve the
/// model, select a ready endpoint in-process, and return the engine URL plus a
/// body whose `model` field is rewritten to the engine's served name. Returns
/// `Ok(None)` in remote mode (caller forwards to a standalone router).
/// Embedded-router forwarding.
///
/// Selects an engine in-process and sends the request straight to it, retrying
/// on 5xx / transport error by excluding the failed endpoint — parity with the
/// standalone `nebula-router` proxy loop. Only called when `AppState.router` is
/// set (`NEBULA_ROUTER_MODE=embedded`).
pub async fn forward_embedded(
    st: &AppState,
    prepared: &PreparedUpstream,
    method: reqwest::Method,
    uri_path: &str,
    uri_query: &str,
    body_bytes: &[u8],
) -> Result<reqwest::Response, Response> {
    let Some(router) = st.router.as_ref() else {
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "embedded router not configured",
        )
            .into_response());
    };

    let raw_model = prepared.model.clone().unwrap_or_default();
    let model_uid = router.resolve_model(&raw_model);
    let engine_model = router
        .get_engine_model_name(&model_uid)
        .unwrap_or_else(|| raw_model.clone());
    let body = nebula_common::rewrite_json_model_field(body_bytes, &engine_model)
        .map(Bytes::from)
        .unwrap_or_else(|| Bytes::copy_from_slice(body_bytes));
    let plan_version = router.plan_version_for(&model_uid).filter(|&v| v > 0);

    let max_attempts = st.retry_max.saturating_add(1).max(1);
    let mut attempt: u32 = 0;
    let mut excluded: Option<(String, u32)> = None;

    loop {
        let routed = match (plan_version, excluded.as_ref()) {
            (Some(pv), Some((m, r))) => router.route_with_plan_version_excluding(
                &prepared.ctx,
                &model_uid,
                pv,
                (m.as_str(), *r),
            ),
            (Some(pv), None) => router.route_with_plan_version(&prepared.ctx, &model_uid, pv),
            (None, Some((m, r))) => {
                router.route_excluding(&prepared.ctx, &model_uid, (m.as_str(), *r))
            }
            (None, None) => router.route(&prepared.ctx, &model_uid),
        };
        let ep = match routed {
            Ok(ep) => ep,
            Err(nebula_router::RouteError::Overloaded) => {
                return Err((
                    axum::http::StatusCode::TOO_MANY_REQUESTS,
                    format!("all endpoints overloaded for model '{model_uid}'"),
                )
                    .into_response());
            }
            Err(_) => {
                return Err((
                    axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    format!("no ready endpoint for model '{model_uid}'"),
                )
                    .into_response());
            }
        };
        let Some(base) = ep.base_url.as_deref().map(|b| b.trim_end_matches('/')) else {
            return Err((
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "endpoint missing base_url",
            )
                .into_response());
        };

        let url = format!("{base}{uri_path}{uri_query}");
        let mut req_headers = prepared.headers.clone();
        req_headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        req_headers.insert(
            nebula_common::HEADER_INTERNAL_AUTH,
            HeaderValue::from_static("1"),
        );
        nebula_common::telemetry::inject_trace_context(&mut req_headers);

        match st
            .http
            .request(method.clone(), url)
            .headers(req_headers)
            .body(body.clone())
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_server_error() {
                    router.record_endpoint_failure(&ep.model_uid, ep.replica_id);
                    st.metrics.record_upstream_error("upstream_5xx");
                    if attempt + 1 < max_attempts {
                        attempt += 1;
                        excluded = Some((ep.model_uid.clone(), ep.replica_id));
                        st.metrics
                            .retry_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tokio::time::sleep(std::time::Duration::from_millis(st.retry_backoff_ms))
                            .await;
                        continue;
                    }
                } else {
                    router.record_endpoint_success(&ep.model_uid, ep.replica_id);
                    if attempt > 0 {
                        st.metrics
                            .retry_success_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                return Ok(resp);
            }
            Err(e) => {
                router.record_endpoint_failure(&ep.model_uid, ep.replica_id);
                let kind = classify_reqwest_error(&e);
                st.metrics.record_upstream_error(kind);
                if attempt + 1 < max_attempts {
                    attempt += 1;
                    excluded = Some((ep.model_uid.clone(), ep.replica_id));
                    st.metrics
                        .retry_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tokio::time::sleep(std::time::Duration::from_millis(st.retry_backoff_ms)).await;
                    continue;
                }
                return Err(upstream_transport_error(
                    kind,
                    format!("upstream request failed: {kind}"),
                ));
            }
        }
    }
}
