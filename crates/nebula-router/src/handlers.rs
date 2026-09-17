use std::sync::atomic::Ordering;

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use nebula_common::ExecutionContext;
use nebula_router::proxy::{
    forward_routed, route_and_send, to_reqwest_headers, ProxyConfig, ProxyError,
};

use crate::state::AppState;

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub fn build_execution_context(headers: &HeaderMap) -> ExecutionContext {
    // Prefer Gateway-injected tenant; clients cannot spoof past auth binding.
    nebula_common::build_execution_context(headers, None, None)
}

pub async fn proxy_chat_completions(
    State(st): State<AppState>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    let ctx = build_execution_context(&headers);
    let request_id = ctx.request_id.clone();
    let request_start = std::time::Instant::now();
    tracing::debug!(
        request_id = %request_id,
        service = "nebula-router",
        "router proxy start"
    );

    let method = req.method().clone();
    let uri_path = req.uri().path().to_string();
    let uri_query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    let (method_reqwest, body_bytes) = match method {
        axum::http::Method::GET => (reqwest::Method::GET, None),
        axum::http::Method::POST => {
            let body_bytes =
                match axum::body::to_bytes(req.into_body(), st.max_request_body_bytes).await {
                    Ok(b) => b,
                    Err(_) => {
                        st.metrics
                            .request_too_large_total
                            .fetch_add(1, Ordering::Relaxed);
                        st.metrics.record_model_status(&st.model_uid, 413);
                        st.dual_write.emit_request_outcome(
                            "nebula_router",
                            Some(&st.model_uid),
                            413,
                            None,
                            false,
                        );
                        return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large")
                            .into_response();
                    }
                };
            (reqwest::Method::POST, Some(body_bytes))
        }
        _ => {
            return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
        }
    };

    let cfg = ProxyConfig {
        retry_max: st.retry_max,
        retry_backoff_ms: st.retry_backoff_ms,
        fallback_model_uid: st.model_uid.clone(),
        metric_prefix: "nebula_router",
    };
    let req_headers = to_reqwest_headers(&headers);

    let routed = route_and_send(
        &st.router,
        &st.http,
        &cfg,
        &st.metrics,
        &st.dual_write,
        &ctx,
        req_headers,
        method_reqwest,
        &uri_path,
        &uri_query,
        body_bytes,
    )
    .await;

    match routed {
        Ok(r) => {
            forward_routed(
                &st.metrics,
                &st.dual_write,
                &cfg,
                &request_id,
                request_start,
                r.model_uid,
                r.endpoint.replica_id,
                r.response,
            )
            .await
        }
        Err(ProxyError::Overloaded { model_uid }) => Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Retry-After", "5")
            .body(Body::from(format!(
                "all endpoints overloaded for model '{}'",
                model_uid
            )))
            .unwrap_or_else(|_| Response::new(Body::empty())),
        Err(ProxyError::NoEndpoint { model_uid }) => (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("no ready endpoint for model '{}'", model_uid),
        )
            .into_response(),
        Err(ProxyError::UpstreamFailed { .. }) => {
            (StatusCode::BAD_GATEWAY, "upstream request failed").into_response()
        }
    }
}

pub async fn metrics_handler(State(st): State<AppState>) -> impl IntoResponse {
    let body = nebula_router::metrics::render(&st.metrics, &st.router);
    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

pub async fn track_requests(
    State(st): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, std::convert::Infallible> {
    st.metrics.requests_inflight.fetch_add(1, Ordering::Relaxed);
    let resp = next.run(req).await;
    st.metrics.requests_inflight.fetch_sub(1, Ordering::Relaxed);
    st.metrics.requests_total.fetch_add(1, Ordering::Relaxed);

    let status = resp.status().as_u16();
    if status >= 500 {
        st.metrics.status_5xx.fetch_add(1, Ordering::Relaxed);
    } else if status >= 400 {
        st.metrics.status_4xx.fetch_add(1, Ordering::Relaxed);
    } else if status >= 200 {
        st.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
    }

    Ok(resp)
}
