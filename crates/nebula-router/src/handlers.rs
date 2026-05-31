use std::convert::Infallible;

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header::HeaderMap as ReqwestHeaderMap;
use tokio_stream::wrappers::ReceiverStream;

use nebula_common::ExecutionContext;

use crate::state::AppState;

fn classify_reqwest_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "timeout";
    }
    if error.is_connect() {
        return "connect";
    }
    "other"
}

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub fn build_execution_context(headers: &HeaderMap) -> ExecutionContext {
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    ExecutionContext {
        request_id: format!("req_{}", uuid::Uuid::new_v4()),
        session_id,
        tenant_id: None,
        priority: None,
        deadline_ms: None,
        budget_tokens: None,
    }
}

fn to_reqwest_headers(headers: &HeaderMap) -> ReqwestHeaderMap {
    let mut out = ReqwestHeaderMap::new();
    for (k, v) in headers.iter() {
        if k.as_str().eq_ignore_ascii_case("host")
            || k.as_str().eq_ignore_ascii_case("content-length")
        {
            continue;
        }
        out.insert(k, v.clone());
    }
    out
}

fn copy_response_headers(src: &ReqwestHeaderMap, dst: &mut Response) {
    for (k, v) in src.iter() {
        if k.as_str().eq_ignore_ascii_case("transfer-encoding")
            || k.as_str().eq_ignore_ascii_case("connection")
            || k.as_str().eq_ignore_ascii_case("keep-alive")
            || k.as_str().eq_ignore_ascii_case("proxy-authenticate")
            || k.as_str().eq_ignore_ascii_case("proxy-authorization")
            || k.as_str().eq_ignore_ascii_case("te")
            || k.as_str().eq_ignore_ascii_case("trailer")
            || k.as_str().eq_ignore_ascii_case("upgrade")
        {
            continue;
        }

        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_str().as_bytes()),
            HeaderValue::from_bytes(v.as_bytes()),
        ) {
            dst.headers_mut().insert(name, value);
        }
    }
}

pub async fn proxy_chat_completions(
    State(st): State<AppState>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    let _ctx = build_execution_context(&headers);
    let request_start = std::time::Instant::now();

    let method = req.method().clone();
    let uri_path = req.uri().path().to_string();
    let uri_query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    let (method_reqwest, body_bytes, model_uid) = match method {
        axum::http::Method::GET => (reqwest::Method::GET, None, st.model_uid.clone()),
        axum::http::Method::POST => {
            let body_bytes = match axum::body::to_bytes(req.into_body(), st.max_request_body_bytes).await {
                Ok(b) => b,
                Err(_) => {
                    st.metrics
                        .request_too_large_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    st.metrics.record_model_status(&st.model_uid, 413);
                    return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
                }
            };

            let raw_model =
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    json.get("model")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| st.model_uid.clone())
                } else {
                    st.model_uid.clone()
                };

            // Resolve model_name → model_uid (or pass through if already a uid)
            let model_uid = st.router.resolve_model(&raw_model);

            // Rewrite the body's "model" field using serde_json::Value
            let body_bytes = {
                let model_name = st.router.get_model_name(&model_uid)
                    .unwrap_or_else(|| raw_model.clone());
                if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    json["model"] = serde_json::Value::String(model_name);
                    Bytes::from(serde_json::to_vec(&json).unwrap_or_else(|_| body_bytes.to_vec()))
                } else {
                    body_bytes
                }
            };

            (reqwest::Method::POST, Some(body_bytes), model_uid)
        }
        _ => {
            return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
        }
    };

    let plan_version = st.plan_version.load(std::sync::atomic::Ordering::Relaxed);

    let mut attempt: u32 = 0;
    let max_attempts = st.retry_max.saturating_add(1).max(1);
    let mut excluded_endpoint: Option<(String, u32)> = None;

    let (_selected_ep, resp) = loop {
        let ep = if model_uid == st.model_uid && plan_version > 0 {
            if let Some((exclude_model_uid, exclude_replica_id)) = excluded_endpoint.as_ref() {
                st.router.route_with_plan_version_excluding(
                    &_ctx,
                    &model_uid,
                    plan_version,
                    (exclude_model_uid.as_str(), *exclude_replica_id),
                )
            } else {
                st.router
                    .route_with_plan_version(&_ctx, &model_uid, plan_version)
            }
        } else if let Some((exclude_model_uid, exclude_replica_id)) = excluded_endpoint.as_ref() {
            st.router
                .route_excluding(&_ctx, &model_uid, (exclude_model_uid.as_str(), *exclude_replica_id))
        } else {
            st.router.route(&_ctx, &model_uid)
        };

        let ep = match ep {
            Ok(ep) => ep,
            Err(nebula_router::RouteError::Overloaded) => {
                st.metrics.record_model_status(&model_uid, 429);
                return Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .header("Retry-After", "5")
                    .body(Body::from(format!(
                        "all endpoints overloaded for model '{}'",
                        model_uid
                    )))
                    .unwrap_or_else(|_| Response::new(Body::empty()));
            }
            Err(_) => {
                st.metrics.record_model_status(&model_uid, 503);
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("no ready endpoint for model '{}'", model_uid),
                )
                    .into_response();
            }
        };

        let base = match ep.base_url.as_deref() {
            Some(s) => s.trim_end_matches('/'),
            None => {
                st.metrics.record_model_status(&model_uid, 503);
                return (StatusCode::SERVICE_UNAVAILABLE, "endpoint missing base_url")
                    .into_response();
            }
        };

        let mut req_headers = to_reqwest_headers(&headers);
        nebula_common::telemetry::inject_trace_context(&mut req_headers);

        let url = format!("{base}{uri_path}{uri_query}");
        let mut builder = st
            .http
            .request(method_reqwest.clone(), url)
            .headers(req_headers);

        if let Some(b) = body_bytes.clone() {
            builder = builder.body(b);
        }

        match builder.send().await {
            Ok(resp) => {
                if resp.status().is_server_error() {
                    st.router
                        .record_endpoint_failure(&ep.model_uid, ep.replica_id);
                    st.metrics.record_upstream_error("upstream_5xx");
                    if attempt + 1 < max_attempts {
                        attempt += 1;
                        excluded_endpoint = Some((ep.model_uid.clone(), ep.replica_id));
                        st.metrics
                            .retry_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tokio::time::sleep(std::time::Duration::from_millis(st.retry_backoff_ms)).await;
                        continue;
                    }
                } else if attempt > 0 {
                    st.metrics
                        .retry_success_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }

                st.router
                    .record_endpoint_success(&ep.model_uid, ep.replica_id);

                break (ep, resp);
            }
            Err(e) => {
                st.router
                    .record_endpoint_failure(&ep.model_uid, ep.replica_id);
                let kind = classify_reqwest_error(&e);
                st.metrics.record_upstream_error(kind);
                tracing::error!(error=%e, retry_kind=%kind, attempt, "router upstream request failed");
                if attempt + 1 < max_attempts {
                    attempt += 1;
                    excluded_endpoint = Some((ep.model_uid.clone(), ep.replica_id));
                    st.metrics
                        .retry_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tokio::time::sleep(std::time::Duration::from_millis(st.retry_backoff_ms)).await;
                    continue;
                }

                st.metrics.record_model_status(&model_uid, 502);
                st.metrics.observe_e2e_latency(&model_uid, request_start.elapsed().as_secs_f64());
                return (StatusCode::BAD_GATEWAY, "upstream request failed").into_response();
            }
        }
    };

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = resp.headers().clone();
    let is_sse = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v: &reqwest::header::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.contains("text/event-stream"))
        .unwrap_or(false);

    if is_sse {
        let mut upstream = resp.bytes_stream();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(64);
        let metrics = st.metrics.clone();
        let model_uid_for_stream = model_uid.clone();
        let status_code = status.as_u16();
        tokio::spawn(async move {
            let mut first_chunk = true;
            while let Some(item) = upstream.next().await {
                match item {
                    Ok(b) => {
                        if first_chunk {
                            first_chunk = false;
                            let ttft = request_start.elapsed().as_secs_f64();
                            metrics.observe_ttft(&model_uid_for_stream, ttft);
                        }
                        let _ = tx.send(Ok(b)).await;
                    }
                    Err(_) => break,
                }
            }
            let e2e = request_start.elapsed().as_secs_f64();
            metrics.observe_e2e_latency(&model_uid_for_stream, e2e);
            metrics.record_model_status(&model_uid_for_stream, status_code);
        });

        let stream = ReceiverStream::new(rx);
        let mut out = Response::builder()
            .status(status)
            .header("content-type", "text/event-stream")
            .body(Body::from_stream(stream))
            .unwrap_or_else(|_| Response::new(Body::empty()));
        copy_response_headers(&resp_headers, &mut out);
        return out;
    }

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(_) => Bytes::new(),
    };

    let e2e = request_start.elapsed().as_secs_f64();
    st.metrics.observe_e2e_latency(&model_uid, e2e);
    st.metrics.record_model_status(&model_uid, status.as_u16());

    let mut out = Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    copy_response_headers(&resp_headers, &mut out);
    out
}
