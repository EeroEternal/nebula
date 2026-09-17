//! In-process routing + upstream forwarding core.
//!
//! This is the shared data-plane hot path used by both the standalone `nebula-router` binary and
//! the embedded router inside `nebula-gateway` (Phase 1 of the data-plane perf optimization:
//! single-hop Gateway → Engine, no internal HTTP hop). It performs model peeking/resolution,
//! routing (plan_version / retry / circuit breaker) and returns the upstream `reqwest::Response`.
//! Response wrapping / streaming / echo headers are handled by [`forward_routed`].

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::Response,
};
use bytes::Bytes;
use futures_util::StreamExt;
use nebula_common::{DualWriteEmitter, ExecutionContext};
use tokio_stream::wrappers::ReceiverStream;

use crate::metrics::Metrics;
use crate::{EndpointInfo, RouteError, Router};

/// Tuning knobs for the routing/forwarding core (mirrors the standalone router env knobs).
#[derive(Clone)]
pub struct ProxyConfig {
    /// Number of retry attempts (beyond the first attempt) per request.
    pub retry_max: u32,
    /// Backoff between retry attempts.
    pub retry_backoff_ms: u64,
    /// Fallback model_uid when the request carries no resolvable model.
    pub fallback_model_uid: String,
    /// Dual-write metric prefix (`nebula_router_*`), shared by the standalone router and the
    /// embedded in-process router.
    pub metric_prefix: &'static str,
}

/// A routed, in-flight upstream response (already sent; body not yet consumed).
pub struct RoutedResponse {
    pub endpoint: EndpointInfo,
    pub model_uid: String,
    pub response: reqwest::Response,
}

#[derive(Debug)]
pub enum ProxyError {
    /// No ready endpoint for the model.
    NoEndpoint { model_uid: String },
    /// All endpoints overloaded (KV cache saturated).
    Overloaded { model_uid: String },
    /// Upstream transport failure after exhausting retries.
    UpstreamFailed {
        model_uid: String,
        kind: &'static str,
    },
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyError::NoEndpoint { model_uid } => {
                write!(f, "no ready endpoint for model '{}'", model_uid)
            }
            ProxyError::Overloaded { model_uid } => {
                write!(f, "all endpoints overloaded for model '{}'", model_uid)
            }
            ProxyError::UpstreamFailed { model_uid, kind } => {
                write!(
                    f,
                    "upstream request failed for model '{}' ({})",
                    model_uid, kind
                )
            }
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

/// Convert inbound axum headers into outbound reqwest headers (dropping hop-by-hop headers).
pub fn to_reqwest_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
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

/// Resolve the model string from trusted injected headers, falling back to a body peek.
fn resolve_model_from_headers(
    headers: &reqwest::header::HeaderMap,
    body: Option<&[u8]>,
    fallback: &str,
) -> String {
    headers
        .get(nebula_common::HEADER_NEBULA_MODEL_UID)
        .or_else(|| headers.get(nebula_common::HEADER_NEBULA_MODEL))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| body.and_then(nebula_common::peek_json_model_field))
        .unwrap_or_else(|| fallback.to_string())
}

/// Route to an endpoint and send the request, retrying on transport errors / upstream 5xx.
///
/// `req_headers` are the already-prepared outbound headers (execution context, model header and
/// trace context injected by the caller). `body` is the fully-buffered request body (POST) or
/// `None` (GET). Callers own body-size admission before invoking this.
#[allow(clippy::too_many_arguments)]
pub async fn route_and_send(
    router: &Arc<Router>,
    http: &reqwest::Client,
    cfg: &ProxyConfig,
    metrics: &Arc<Metrics>,
    dual_write: &DualWriteEmitter,
    ctx: &ExecutionContext,
    req_headers: reqwest::header::HeaderMap,
    method: reqwest::Method,
    uri_path: &str,
    uri_query: &str,
    body: Option<Bytes>,
) -> Result<RoutedResponse, ProxyError> {
    let request_start = Instant::now();

    let raw_model = resolve_model_from_headers(
        &req_headers,
        body.as_deref(),
        cfg.fallback_model_uid.as_str(),
    );
    let model_uid = router.resolve_model(&raw_model);

    // Rewrite only the model string for the engine — no full JSON DOM.
    let engine_model = router
        .get_engine_model_name(&model_uid)
        .unwrap_or_else(|| raw_model.clone());
    let body = body.map(|b| {
        nebula_common::rewrite_json_model_field(&b, &engine_model)
            .map(Bytes::from)
            .unwrap_or(b)
    });

    let plan_version = router.plan_version_for(&model_uid).filter(|&v| v > 0);

    let mut attempt: u32 = 0;
    let max_attempts = cfg.retry_max.saturating_add(1).max(1);
    let mut excluded_endpoint: Option<(String, u32)> = None;

    let (selected_ep, resp) = loop {
        let ep = match (plan_version, excluded_endpoint.as_ref()) {
            (Some(pv), Some((m, r))) => {
                router.route_with_plan_version_excluding(ctx, &model_uid, pv, (m.as_str(), *r))
            }
            (Some(pv), None) => router.route_with_plan_version(ctx, &model_uid, pv),
            (None, Some((m, r))) => router.route_excluding(ctx, &model_uid, (m.as_str(), *r)),
            (None, None) => router.route(ctx, &model_uid),
        };

        let ep = match ep {
            Ok(ep) => ep,
            Err(RouteError::Overloaded) => {
                metrics.record_model_status(&model_uid, 429);
                return Err(ProxyError::Overloaded { model_uid });
            }
            Err(RouteError::NoEndpoint) => {
                metrics.record_model_status(&model_uid, 503);
                return Err(ProxyError::NoEndpoint { model_uid });
            }
        };

        let base = match ep.base_url.as_deref() {
            Some(s) => s.trim_end_matches('/'),
            None => {
                metrics.record_model_status(&model_uid, 503);
                return Err(ProxyError::NoEndpoint { model_uid });
            }
        };

        let mut req_headers = req_headers.clone();
        nebula_common::telemetry::inject_trace_context(&mut req_headers);

        let url = format!("{base}{uri_path}{uri_query}");
        let mut builder = http.request(method.clone(), url).headers(req_headers);
        if let Some(b) = body.clone() {
            builder = builder.body(b);
        }

        match builder.send().await {
            Ok(resp) => {
                if resp.status().is_server_error() {
                    router.record_endpoint_failure(&ep.model_uid, ep.replica_id);
                    metrics.record_upstream_error("upstream_5xx");
                    if attempt + 1 < max_attempts {
                        attempt += 1;
                        excluded_endpoint = Some((ep.model_uid.clone(), ep.replica_id));
                        metrics
                            .retry_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tokio::time::sleep(std::time::Duration::from_millis(cfg.retry_backoff_ms))
                            .await;
                        continue;
                    }
                } else if attempt > 0 {
                    metrics
                        .retry_success_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }

                router.record_endpoint_success(&ep.model_uid, ep.replica_id);
                break (ep, resp);
            }
            Err(e) => {
                router.record_endpoint_failure(&ep.model_uid, ep.replica_id);
                let kind = classify_reqwest_error(&e);
                metrics.record_upstream_error(kind);
                tracing::error!(error=%e, retry_kind=%kind, attempt, "router upstream request failed");
                if attempt + 1 < max_attempts {
                    attempt += 1;
                    excluded_endpoint = Some((ep.model_uid.clone(), ep.replica_id));
                    metrics
                        .retry_total
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tokio::time::sleep(std::time::Duration::from_millis(cfg.retry_backoff_ms))
                        .await;
                    continue;
                }

                let e2e = request_start.elapsed().as_secs_f64();
                metrics.record_model_status(&model_uid, 502);
                metrics.observe_e2e_latency(&model_uid, e2e);
                dual_write.emit_request_outcome(
                    cfg.metric_prefix,
                    Some(&model_uid),
                    502,
                    Some(e2e),
                    false,
                );
                return Err(ProxyError::UpstreamFailed { model_uid, kind });
            }
        }
    };

    Ok(RoutedResponse {
        endpoint: selected_ep,
        model_uid,
        response: resp,
    })
}

fn copy_response_headers(src: &reqwest::header::HeaderMap, dst: &mut Response) {
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

/// RAII observer that records router-level E2E / status / TTFT metrics when a response body is
/// consumed outside of [`forward_routed`] — used by the gateway's protocol adapters (Responses /
/// Anthropic) which transform the upstream body instead of streaming it verbatim.
///
/// Construct only when the embedded router is active; the standalone router records these inside
/// [`forward_routed`]. Records on drop so every return path is covered.
pub struct ResponseObserver {
    metrics: Arc<Metrics>,
    dual_write: DualWriteEmitter,
    metric_prefix: &'static str,
    model_uid: String,
    status: u16,
    started: Instant,
}

impl ResponseObserver {
    pub fn new(
        metrics: Arc<Metrics>,
        dual_write: DualWriteEmitter,
        metric_prefix: &'static str,
        model_uid: String,
        status: u16,
        started: Instant,
    ) -> Self {
        Self {
            metrics,
            dual_write,
            metric_prefix,
            model_uid,
            status,
            started,
        }
    }

    /// Observe time-to-first-token (call once when the first body chunk is forwarded).
    pub fn record_ttft(&self, ttft: f64) {
        self.metrics.observe_ttft(&self.model_uid, ttft);
        self.dual_write
            .emit_ttft(self.metric_prefix, &self.model_uid, ttft);
    }
}

impl Drop for ResponseObserver {
    fn drop(&mut self) {
        let e2e = self.started.elapsed().as_secs_f64();
        self.metrics.observe_e2e_latency(&self.model_uid, e2e);
        self.metrics
            .record_model_status(&self.model_uid, self.status);
        self.dual_write.emit_request_outcome(
            self.metric_prefix,
            Some(&self.model_uid),
            self.status,
            Some(e2e),
            false,
        );
    }
}

/// Inject the router echo headers (`x-nebula-request-id`, `x-nebula-replica-id`).
pub fn inject_echo_headers(out: &mut Response, request_id: &str, replica_id: u32) {
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

/// Wrap a routed upstream response into an axum response, streaming SSE bodies and recording
/// TTFT / E2E / abort observability.
#[allow(clippy::too_many_arguments)]
pub async fn forward_routed(
    metrics: &Arc<Metrics>,
    dual_write: &DualWriteEmitter,
    cfg: &ProxyConfig,
    request_id: &str,
    request_start: Instant,
    model_uid: String,
    replica_id: u32,
    resp: reqwest::Response,
) -> Response {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = resp.headers().clone();
    let is_sse = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("text/event-stream"))
        .unwrap_or(false);

    if is_sse {
        let mut upstream = resp.bytes_stream();
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(64);
        let metrics = metrics.clone();
        let dual = dual_write.clone();
        let metric_prefix = cfg.metric_prefix;
        let model_uid_for_stream = model_uid.clone();
        let status_code = status.as_u16();
        let request_id_for_task = request_id.to_string();
        tokio::spawn(async move {
            let mut first_chunk = true;
            let mut aborted = false;
            loop {
                tokio::select! {
                    biased;
                    _ = tx.closed() => {
                        aborted = true;
                        break;
                    }
                    item = upstream.next() => {
                        match item {
                            Some(Ok(b)) => {
                                if first_chunk {
                                    first_chunk = false;
                                    let ttft = request_start.elapsed().as_secs_f64();
                                    metrics.observe_ttft(&model_uid_for_stream, ttft);
                                    dual.emit_ttft(metric_prefix, &model_uid_for_stream, ttft);
                                }
                                if tx.send(Ok(b)).await.is_err() {
                                    aborted = true;
                                    break;
                                }
                            }
                            Some(Err(_)) | None => break,
                        }
                    }
                }
            }
            let e2e = request_start.elapsed().as_secs_f64();
            metrics.observe_e2e_latency(&model_uid_for_stream, e2e);
            if aborted {
                metrics
                    .requests_aborted_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                dual.emit_request_outcome(
                    metric_prefix,
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
            } else {
                metrics.record_model_status(&model_uid_for_stream, status_code);
                dual.emit_request_outcome(
                    metric_prefix,
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
        inject_echo_headers(&mut out, request_id, replica_id);
        return out;
    }

    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(_) => Bytes::new(),
    };

    let e2e = request_start.elapsed().as_secs_f64();
    metrics.observe_e2e_latency(&model_uid, e2e);
    metrics.record_model_status(&model_uid, status.as_u16());
    dual_write.emit_request_outcome(
        cfg.metric_prefix,
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
    inject_echo_headers(&mut out, request_id, replica_id);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_common::{EndpointInfo, EndpointKind, EndpointStatus, ExecutionContext};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Minimal HTTP/1.1 server that answers any request with a fixed JSON body.
    async fn spawn_mock_engine() -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 8192];
                    // Request content is irrelevant; drain once so reqwest can send the body.
                    let _ = socket.read(&mut buf).await;
                    let body = r#"{"ok":true}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(resp.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        port
    }

    fn ready_ep(port: u16) -> EndpointInfo {
        EndpointInfo {
            model_uid: "m1".into(),
            replica_id: 0,
            plan_version: 1,
            node_id: "n1".into(),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".into(),
            status: EndpointStatus::Ready,
            last_heartbeat_ms: 0,
            status_detail: None,
            grpc_target: None,
            base_url: Some(format!("http://127.0.0.1:{port}")),
        }
    }

    fn test_ctx() -> ExecutionContext {
        ExecutionContext {
            request_id: "req_test".into(),
            session_id: None,
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: None,
            inference_hint: None,
            hint_trusted: false,
        }
    }

    fn test_cfg() -> ProxyConfig {
        ProxyConfig {
            retry_max: 0,
            retry_backoff_ms: 0,
            fallback_model_uid: "m1".into(),
            metric_prefix: "nebula_router",
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn routes_and_sends_in_process() {
        let port = spawn_mock_engine().await;
        let router = Router::new();
        router.upsert_endpoint(ready_ep(port));
        let metrics = Arc::new(crate::metrics::Metrics::default());
        let dual = DualWriteEmitter::from_env("test", None, None);
        let http = nebula_common::proxy_http_client().unwrap();

        let routed = route_and_send(
            &router,
            &http,
            &test_cfg(),
            &metrics,
            &dual,
            &test_ctx(),
            reqwest::header::HeaderMap::new(),
            reqwest::Method::POST,
            "/v1/chat/completions",
            "",
            Some(Bytes::from_static(br#"{"model":"m1","messages":[]}"#)),
        )
        .await
        .expect("route and send");

        assert_eq!(routed.model_uid, "m1");
        assert_eq!(routed.endpoint.replica_id, 0);
        assert_eq!(routed.response.status(), 200);
        let body = routed.response.bytes().await.unwrap();
        assert!(body.windows(4).any(|w| w == b"true"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn returns_no_endpoint_when_empty() {
        let router = Router::new();
        let metrics = Arc::new(crate::metrics::Metrics::default());
        let dual = DualWriteEmitter::from_env("test", None, None);
        let http = nebula_common::proxy_http_client().unwrap();

        let result = route_and_send(
            &router,
            &http,
            &test_cfg(),
            &metrics,
            &dual,
            &test_ctx(),
            reqwest::header::HeaderMap::new(),
            reqwest::Method::POST,
            "/v1/chat/completions",
            "",
            Some(Bytes::from_static(br#"{"model":"m1"}"#)),
        )
        .await;
        let err = match result {
            Ok(_) => panic!("expected NoEndpoint error"),
            Err(e) => e,
        };

        match err {
            ProxyError::NoEndpoint { model_uid } => assert_eq!(model_uid, "m1"),
            other => panic!("expected NoEndpoint, got {other:?}"),
        }
    }
}
