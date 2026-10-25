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

use nebula_common::{ExecutionContext, UpstreamRequest, UpstreamResponse};

use crate::state::AppState;

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub fn build_execution_context(headers: &HeaderMap) -> ExecutionContext {
    // Prefer Gateway-injected tenant; clients cannot spoof past auth binding.
    nebula_common::build_execution_context(headers, None, None)
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

/// Upstream body state after the pre-first-byte gate.
enum UpstreamBody {
    /// SSE whose first chunk has already been read (nothing sent to the client yet).
    Streamed {
        rest: std::pin::Pin<
            Box<
                dyn futures_util::Stream<Item = Result<Bytes, nebula_common::UpstreamError>> + Send,
            >,
        >,
    },
    /// Non-SSE response, untouched (the body is read by the caller).
    Whole(UpstreamResponse),
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

    let (method_reqwest, body_bytes, model_uid) = match method {
        axum::http::Method::GET => (reqwest::Method::GET, None, st.model_uid.clone()),
        axum::http::Method::POST => {
            let body_bytes =
                match axum::body::to_bytes(req.into_body(), st.max_request_body_bytes).await {
                    Ok(b) => b,
                    Err(_) => {
                        st.metrics
                            .request_too_large_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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

            // Prefer Gateway-injected header (C1); fall back to lightweight peek.
            let raw_model = headers
                .get(nebula_common::HEADER_NEBULA_MODEL_UID)
                .or_else(|| headers.get(nebula_common::HEADER_NEBULA_MODEL))
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string())
                .or_else(|| nebula_common::peek_json_model_field(&body_bytes))
                .unwrap_or_else(|| st.model_uid.clone());

            let model_uid = st.router.resolve_model(&raw_model);

            // Rewrite only the model string for the engine — no full JSON DOM.
            let model_name = st
                .router
                .get_engine_model_name(&model_uid)
                .unwrap_or_else(|| raw_model.clone());
            let body_bytes = nebula_common::rewrite_json_model_field(&body_bytes, &model_name)
                .map(Bytes::from)
                .unwrap_or(body_bytes);

            (reqwest::Method::POST, Some(body_bytes), model_uid)
        }
        _ => {
            return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
        }
    };

    let plan_version = st.router.plan_version_for(&model_uid).filter(|&v| v > 0);

    // P1c: hash the prompt's prefix blocks so the prefix_affinity strategy
    // can route to the replica that already holds the KV. No-op unless the
    // strategy is active; chat bodies are not hashed (template gap — falls
    // back visibly via the strategy fallback counters).
    let (prefix_hashes, pd_warm) = match body_bytes.as_deref() {
        Some(b) => {
            st.router
                .body_route_hints(
                    &model_uid,
                    b,
                    ctx.tenant_id.as_deref(),
                    ctx.session_id.as_deref(),
                )
                .await
        }
        None => (None, false),
    };
    let prefix_hashes = prefix_hashes.as_deref();

    let mut attempt: u32 = 0;
    let max_attempts = st.retry_max.saturating_add(1).max(1);
    let mut excluded_endpoint: Option<(String, u32)> = None;
    // Pre-first-byte commitment (issue #42): the client must not see a status before
    // the first chunk, so a stalling replica is retryable instead of merely reportable.
    // Per attempt: first-byte budget; across attempts: this ceiling, or a broken
    // cluster multiplies latency by the attempt count.
    let first_byte_budget = nebula_common::first_byte_timeout();
    let commit_deadline = std::time::Instant::now() + nebula_common::first_byte_total_budget();
    let mut retried_before_first_byte = 0u32;

    let (_selected_ep, status, resp_headers, body) = loop {
        let ep = match (plan_version, excluded_endpoint.as_ref()) {
            (Some(pv), Some((exclude_model_uid, exclude_replica_id))) => {
                st.router.route_with_plan_version_excluding(
                    &ctx,
                    &model_uid,
                    pv,
                    (exclude_model_uid.as_str(), *exclude_replica_id),
                    prefix_hashes,
                    pd_warm,
                )
            }
            (Some(pv), None) => {
                st.router
                    .route_with_plan_version(&ctx, &model_uid, pv, prefix_hashes, pd_warm)
            }
            (None, Some((exclude_model_uid, exclude_replica_id))) => st.router.route_excluding(
                &ctx,
                &model_uid,
                (exclude_model_uid.as_str(), *exclude_replica_id),
                prefix_hashes,
                pd_warm,
            ),
            (None, None) => st.router.route(&ctx, &model_uid, prefix_hashes, pd_warm),
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
        req_headers.insert(
            nebula_common::HEADER_INTERNAL_AUTH,
            reqwest::header::HeaderValue::from_static("1"),
        );

        let url = format!("{base}{uri_path}{uri_query}");
        let mut upstream_request = UpstreamRequest {
            method: method_reqwest.clone(),
            url,
            headers: req_headers,
            body: None,
        };
        if let Some(b) = body_bytes.clone() {
            upstream_request.body = Some(b);
        }

        match st.upstream.execute(upstream_request).await {
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
                        tokio::time::sleep(std::time::Duration::from_millis(st.retry_backoff_ms))
                            .await;
                        continue;
                    }
                }

                let status =
                    StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
                let resp_headers = resp.headers().clone();
                let is_sse = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v: &reqwest::header::HeaderValue| v.to_str().ok())
                    .map(|s: &str| s.contains("text/event-stream"))
                    .unwrap_or(false);

                if !is_sse {
                    st.router
                        .record_endpoint_success(&ep.model_uid, ep.replica_id);
                    if attempt > 0 {
                        st.metrics
                            .retry_success_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    break (ep, status, resp_headers, UpstreamBody::Whole(resp));
                }

                // SSE: nothing has been sent to the client yet. Wait for the first byte
                // before committing a status; a silent replica is retryable.
                let remaining =
                    commit_deadline.saturating_duration_since(std::time::Instant::now());
                let budget = match first_byte_budget {
                    Some(b) => b.min(remaining),
                    None => remaining,
                };
                let first = if budget.is_zero() {
                    None
                } else {
                    nebula_router::first_byte::await_first_chunk(resp.bytes_stream(), budget).await
                };
                match first {
                    // `rest` replays `head` first, so no byte is lost.
                    Some((_head, rest)) => {
                        st.router
                            .record_endpoint_success(&ep.model_uid, ep.replica_id);
                        let ttft = request_start.elapsed().as_secs_f64();
                        st.metrics.observe_ttft(&model_uid, ttft);
                        st.dual_write.emit_ttft("nebula_router", &model_uid, ttft);
                        if retried_before_first_byte > 0 {
                            st.metrics
                                .first_byte_retry_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            st.metrics
                                .retry_success_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        break (
                            ep,
                            status,
                            resp_headers,
                            UpstreamBody::Streamed {
                                rest: Box::pin(rest),
                            },
                        );
                    }
                    None => {
                        st.router
                            .record_endpoint_failure(&ep.model_uid, ep.replica_id);
                        st.metrics.record_upstream_error("first_byte_timeout");
                        let budget_left = std::time::Instant::now() < commit_deadline;
                        if attempt + 1 < max_attempts && budget_left {
                            attempt += 1;
                            retried_before_first_byte += 1;
                            excluded_endpoint = Some((ep.model_uid.clone(), ep.replica_id));
                            st.metrics
                                .retry_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            tracing::warn!(
                                model_uid = %ep.model_uid,
                                replica_id = ep.replica_id,
                                attempt,
                                "no first byte before the deadline; retrying another replica"
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(
                                st.retry_backoff_ms,
                            ))
                            .await;
                            continue;
                        }
                        st.metrics.record_model_status(&model_uid, 503);
                        let e2e = request_start.elapsed().as_secs_f64();
                        st.metrics.observe_e2e_latency(&model_uid, e2e);
                        st.dual_write.emit_request_outcome(
                            "nebula_router",
                            Some(&model_uid),
                            503,
                            Some(e2e),
                            false,
                        );
                        tracing::error!(
                            model_uid = %model_uid,
                            attempt,
                            "no replica produced a first byte within the budget"
                        );
                        return (
                            StatusCode::SERVICE_UNAVAILABLE,
                            [(reqwest::header::RETRY_AFTER.as_str(), "1")],
                            "upstream produced no first byte",
                        )
                            .into_response();
                    }
                }
            }
            Err(e) => {
                if e.is_caller_fault() {
                    // A request we could not build fails identically on every replica:
                    // do not count it as an upstream error, do not blame this replica,
                    // do not retry.
                    tracing::error!(
                        error = %e,
                        model_uid = %ep.model_uid,
                        "upstream request rejected by our transport"
                    );
                    let e2e = request_start.elapsed().as_secs_f64();
                    st.metrics.record_model_status(&model_uid, 500);
                    st.metrics.observe_e2e_latency(&model_uid, e2e);
                    st.dual_write.emit_request_outcome(
                        "nebula_router",
                        Some(&model_uid),
                        500,
                        Some(e2e),
                        false,
                    );
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "upstream request invalid",
                    )
                        .into_response();
                }
                st.router
                    .record_endpoint_failure(&ep.model_uid, ep.replica_id);
                let kind = e.label();
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

                let e2e = request_start.elapsed().as_secs_f64();
                st.metrics.record_model_status(&model_uid, 502);
                st.metrics.observe_e2e_latency(&model_uid, e2e);
                st.dual_write.emit_request_outcome(
                    "nebula_router",
                    Some(&model_uid),
                    502,
                    Some(e2e),
                    false,
                );
                return (StatusCode::BAD_GATEWAY, "upstream request failed").into_response();
            }
        }
    };

    if let UpstreamBody::Streamed { rest } = body {
        let mut upstream = rest;
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(64);
        let stream_idle = nebula_common::stream_idle_timeout();
        // The first byte already arrived (and TTFT is recorded above), so every
        // remaining chunk gets the idle budget.
        let mut chunk_deadline = nebula_common::ChunkDeadline::new(None, stream_idle);
        let metrics = st.metrics.clone();
        let dual = st.dual_write.clone();
        let router_for_stream = st.router.clone();
        let tenant_for_stream = ctx.tenant_id.clone();
        let session_for_stream = ctx.session_id.clone();
        let replica_for_stream = _selected_ep.replica_id;
        let model_uid_for_stream = model_uid.clone();
        let status_code = status.as_u16();
        let request_id_for_task = request_id.clone();
        tokio::spawn(async move {
            let mut decode_tracker = nebula_router::SseDecodeTracker::default();
            let mut aborted = false;
            let mut tool_pause_marked = false;
            let mut tool_scanner = nebula_router::ToolCallScanner::default();
            loop {
                tokio::select! {
                    biased;
                    _ = tx.closed() => {
                        aborted = true;
                        break;
                    }
                    item = tokio::time::timeout(chunk_deadline.next_deadline().0, upstream.next()) => {
                        match item {
                            Ok(Some(Ok(b))) => {
                                decode_tracker.feed(&b);
                                if !tool_pause_marked {
                                    if let Some(ref sid) = session_for_stream {
                                        if tool_scanner.feed(&b) {
                                            router_for_stream.mark_tool_pause_for_tenant(
                                                tenant_for_stream.as_deref(),
                                                sid,
                                                &model_uid_for_stream,
                                                replica_for_stream,
                                            );
                                            tool_pause_marked = true;
                                        }
                                    }
                                }
                                if tx.send(Ok(b)).await.is_err() {
                                    aborted = true;
                                    break;
                                }
                            }
                            // A mid-stream upstream error (engine aborted the response,
                            // KV retrieve failed, connection reset) used to `break`
                            // silently: the client got a truncated 200 with no [DONE]
                            // and nothing was logged or counted (issue #37).
                            Ok(Some(Err(e))) => {
                                metrics
                                    .upstream_stream_error_total
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                tracing::warn!(
                                    model_uid = %model_uid_for_stream,
                                    replica_id = replica_for_stream,
                                    error = %e,
                                    "router upstream stream error; ending stream early"
                                );
                                let _ = tx
                                    .send(Ok(nebula_router::sse_error_frame(
                                        "upstream_stream_error",
                                        "upstream stream failed before completion",
                                    )))
                                    .await;
                                break;
                            }
                            Ok(None) => break,
                            Err(_) => {
                                tracing::warn!(
                                    model_uid = %model_uid_for_stream,
                                    idle_ms = stream_idle.as_millis() as u64,
                                    "router upstream SSE deadline exceeded; ending stream"
                                );
                                let _ = tx
                                    .send(Ok(nebula_router::sse_error_frame(
                                        "upstream_stream_timeout",
                                        "upstream stream stalled past the deadline",
                                    )))
                                    .await;
                                break;
                            }
                        }
                    }
                }
            }
            let e2e = request_start.elapsed().as_secs_f64();
            metrics.observe_e2e_latency(&model_uid_for_stream, e2e);
            if !aborted {
                if let Some(tpot) = decode_tracker.finish_tpot() {
                    metrics.observe_tpot(&model_uid_for_stream, tpot);
                }
            }
            if aborted {
                metrics
                    .requests_aborted_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                dual.emit_request_outcome(
                    "nebula_router",
                    Some(&model_uid_for_stream),
                    status_code,
                    Some(e2e),
                    true,
                );
                tracing::info!(
                    model_uid = %model_uid_for_stream,
                    request_id = %request_id_for_task,
                    "router SSE aborted: client disconnected"
                );
                // Abort is not a 5xx / model error.
            } else {
                metrics.record_model_status(&model_uid_for_stream, status_code);
                dual.emit_request_outcome(
                    "nebula_router",
                    Some(&model_uid_for_stream),
                    status_code,
                    Some(e2e),
                    false,
                );
            }
            // Dropping `upstream` closes the connection to the engine.
        });

        let stream = ReceiverStream::new(rx);
        let mut out = Response::builder()
            .status(status)
            .header("content-type", "text/event-stream")
            .body(Body::from_stream(stream))
            .unwrap_or_else(|_| Response::new(Body::empty()));
        copy_response_headers(&resp_headers, &mut out);
        inject_router_echo_headers(&mut out, &request_id, _selected_ep.replica_id);
        return out;
    }

    let resp = match body {
        UpstreamBody::Whole(resp) => resp,
        UpstreamBody::Streamed { .. } => unreachable!("streamed body handled above"),
    };
    let bytes = match tokio::time::timeout(nebula_common::stream_idle_timeout(), resp.bytes()).await
    {
        Ok(Ok(b)) => b,
        Ok(Err(_)) | Err(_) => Bytes::new(),
    };
    if let Some(ref sid) = ctx.session_id {
        if nebula_router::response_has_tool_calls(&bytes) {
            st.router.mark_tool_pause_for_tenant(
                ctx.tenant_id.as_deref(),
                sid,
                &model_uid,
                _selected_ep.replica_id,
            );
        }
    }

    let e2e = request_start.elapsed().as_secs_f64();
    st.metrics.observe_e2e_latency(&model_uid, e2e);
    st.metrics.record_model_status(&model_uid, status.as_u16());
    st.dual_write.emit_request_outcome(
        "nebula_router",
        Some(&model_uid),
        status.as_u16(),
        Some(e2e),
        false,
    );

    let mut out = Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    copy_response_headers(&resp_headers, &mut out);
    inject_router_echo_headers(&mut out, &request_id, _selected_ep.replica_id);
    out
}

fn inject_router_echo_headers(out: &mut Response, request_id: &str, replica_id: u32) {
    use nebula_common::{HEADER_REPLICA_ID, HEADER_REQUEST_ID};

    if let Ok(v) = HeaderValue::from_str(request_id) {
        out.headers_mut()
            .insert(HeaderName::from_static(HEADER_REQUEST_ID), v);
    }
    if let Ok(v) = HeaderValue::from_str(&replica_id.to_string()) {
        out.headers_mut()
            .insert(HeaderName::from_static(HEADER_REPLICA_ID), v);
    }
}
