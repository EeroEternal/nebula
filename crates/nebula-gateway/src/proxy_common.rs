//! Shared Gateway→Router admission, header injection, and POST helpers.

use axum::{
    body::Body,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::Response,
};
use bytes::Bytes;
use nebula_common::{
    build_execution_context, inject_execution_context, peek_json_model_field, Tenant, TenantDenyCode,
};
use nebula_meta::MetaStore;
use serde_json::Value;

use crate::auth::AuthContext;
use crate::interface::{maybe_normalize_router_error, upstream_transport_error};
use crate::state::AppState;

pub struct PreparedUpstream {
    pub model: Option<String>,
    pub headers: reqwest::header::HeaderMap,
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
    let ctx = build_execution_context(headers, auth.tenant_id.as_deref(), None);

    let _conc_guard = if st.auth.multi_tenant {
        if let Some(ref tenant_id) = ctx.tenant_id {
            match load_tenant(&*st.store, tenant_id).await {
                Ok(Some(tenant)) => {
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

    let mut outbound = headers.clone();
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
        headers: req_headers,
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
    headers: reqwest::header::HeaderMap,
    chat_body: &Value,
) -> Result<reqwest::Response, Response> {
    let url = format!(
        "{}/v1/chat/completions",
        st.router_base_url.trim_end_matches('/')
    );
    let mut req_headers = headers;
    req_headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let body = Bytes::from(serde_json::to_vec(chat_body).unwrap_or_default());
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
        resp.headers_mut().insert(
            HeaderName::from_static("x-nebula-deny-code"),
            v,
        );
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
pub async fn forward_upstream_response(st: &AppState, resp: reqwest::Response) -> Response {
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
        tokio::spawn(async move {
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
    out
}
