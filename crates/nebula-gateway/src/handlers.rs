use std::convert::Infallible;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Request, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    Extension, Json,
};
use bytes::Bytes;
use serde_json::{json, Value};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use uuid::Uuid;

use nebula_common::{
    build_execution_context, ClusterStatus, EndpointInfo, ModelLoadRequest, ModelRequest,
    NodeStatus, PlacementPlan,
};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::auth::{require_role, AuthContext, Role};
use crate::interface::{
    anthropic_json_to_openai_chat, check_tooling_gate, openai_chat_json_to_anthropic,
    parse_openai_sse_chunk, payload_too_large_response, responses_json_to_openai_chat,
    maybe_normalize_router_error, upstream_transport_error, AnthropicSseMapper, OpenAiStreamChunk,
};
use crate::proxy_common::{
    append_headers, classify_reqwest_error, forward_upstream_response, prepare_upstream,
    prepare_upstream_from_json, post_router_chat, to_reqwest_headers,
};
use crate::responses::{build_non_stream_json, build_response, ResponseStreamBuilder};
use crate::state::AppState;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct LogsQuery {
    lines: Option<usize>,
}

pub async fn create_responses(
    State(st): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"message": format!("invalid json: {e}"), "type": "invalid_request_error"}})),
            )
                .into_response();
        }
    };

    let model = payload
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if let Some(deny) = check_tooling_gate(&*st.store, &payload, model.as_deref()).await {
        return deny;
    }

    let (resp_req, chat_body) = match responses_json_to_openai_chat(&payload) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"message": e.to_string(), "type": "invalid_request_error"}})),
            )
                .into_response();
        }
    };

    let prepared = match prepare_upstream_from_json(&st, &auth, &headers, &chat_body).await {
        Ok(p) => p,
        Err(r) => return r,
    };

    let builder_seed = crate::responses::CreateResponseRequest {
        model: Some(resp_req.model.clone()),
        input: resp_req.input.clone(),
        instructions: resp_req.instructions.clone().map(Value::String),
        stream: Some(resp_req.stream),
    };

    proxy_chat_as_responses(
        st,
        prepared,
        chat_body,
        builder_seed,
        resp_req.stream,
    )
    .await
}

/// Anthropic Messages API → OpenAI chat via UniGateway protocol → Router → Anthropic-shaped reply.
pub async fn create_anthropic_messages(
    State(st): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"type": "error", "error": {"type": "invalid_request_error", "message": format!("invalid json: {e}")}})),
            )
                .into_response();
        }
    };

    let requested_model = payload
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    if let Some(deny) =
        check_tooling_gate(&*st.store, &payload, Some(requested_model.as_str())).await
    {
        return deny;
    }

    let chat_body = match anthropic_json_to_openai_chat(&payload) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"type": "error", "error": {"type": "invalid_request_error", "message": e.to_string()}})),
            )
                .into_response();
        }
    };

    let stream = chat_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let prepared = match prepare_upstream_from_json(&st, &auth, &headers, &chat_body).await {
        Ok(p) => p,
        Err(r) => return r,
    };

    proxy_chat_as_anthropic(st, prepared, chat_body, requested_model, stream).await
}

async fn proxy_chat_as_responses(
    st: AppState,
    prepared: crate::proxy_common::PreparedUpstream,
    chat_body: Value,
    builder_seed: crate::responses::CreateResponseRequest,
    stream: bool,
) -> Response {
    let resp = match post_router_chat(&st, prepared.headers.clone(), &chat_body).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    // Keep admission guard alive until response finishes.
    let _guard = prepared._conc_guard;

    if !resp.status().is_success() {
        let status =
            StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let bytes = resp.bytes().await.unwrap_or_default();
        if let Some(normalized) = maybe_normalize_router_error(status, &bytes) {
            return normalized;
        }
        return Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| Response::new(Body::empty()));
    }

    if stream {
        let mut upstream = resp.bytes_stream();
        let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);
        let metrics = st.metrics.clone();
        let mut builder = ResponseStreamBuilder::new(&builder_seed);

        tokio::spawn(async move {
            if tx
                .send(Ok(
                    Event::default().data(builder.created_event().to_string())
                ))
                .await
                .is_err()
            {
                metrics
                    .requests_aborted_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }

            let mut buf = String::new();
            loop {
                tokio::select! {
                    biased;
                    _ = tx.closed() => {
                        metrics
                            .requests_aborted_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tracing::info!("responses SSE aborted: client disconnected");
                        break;
                    }
                    item = upstream.next() => {
                        match item {
                            Some(Ok(chunk)) => {
                                buf.push_str(&String::from_utf8_lossy(&chunk));
                                while let Some(pos) = buf.find('\n') {
                                    let mut line = buf[..pos].to_string();
                                    buf.drain(..=pos);
                                    line = line.trim().to_string();
                                    if line.is_empty() || !line.starts_with("data:") {
                                        continue;
                                    }
                                    let data = line.trim_start_matches("data:").trim();
                                    for chunk in parse_openai_sse_chunk(data) {
                                        match chunk {
                                            OpenAiStreamChunk::Done => {
                                                let completed = builder.completed_event();
                                                let _ = tx
                                                    .send(Ok(Event::default().data(completed.to_string())))
                                                    .await;
                                                return;
                                            }
                                            OpenAiStreamChunk::Text(delta) => {
                                                let ev = builder.push_delta(delta);
                                                if tx.send(Ok(Event::default().data(ev.to_string()))).await.is_err() {
                                                    metrics
                                                        .requests_aborted_total
                                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                                    return;
                                                }
                                            }
                                            OpenAiStreamChunk::ToolCallDelta {
                                                index,
                                                id,
                                                name,
                                                arguments,
                                            } => {
                                                for ev in builder.push_tool_call_delta(
                                                    index, id, name, arguments,
                                                ) {
                                                    if tx.send(Ok(Event::default().data(ev.to_string()))).await.is_err() {
                                                        metrics
                                                            .requests_aborted_total
                                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                                        return;
                                                    }
                                                }
                                            }
                                            OpenAiStreamChunk::FinishReason(_)
                                            | OpenAiStreamChunk::Ignored => {}
                                        }
                                    }
                                }
                            }
                            Some(Err(_)) | None => {
                                let completed = builder.completed_event();
                                let _ = tx
                                    .send(Ok(Event::default().data(completed.to_string())))
                                    .await;
                                break;
                            }
                        }
                    }
                }
            }
        });

        return Sse::new(ReceiverStream::new(rx))
            .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
            .into_response();
    }

    let bytes = resp.bytes().await.unwrap_or_default();
    let openai: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    let text = openai
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let built = build_response(&builder_seed, text);
    let mut out = build_non_stream_json(&built);
    // Preserve non-stream tool_calls as Responses function_call output items.
    if let Some(tcs) = openai
        .pointer("/choices/0/message/tool_calls")
        .and_then(|v| v.as_array())
    {
        if let Some(output) = out.get_mut("output").and_then(|v| v.as_array_mut()) {
            for tc in tcs {
                let call_id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = tc
                    .pointer("/function/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let arguments = tc
                    .pointer("/function/arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");
                output.push(json!({
                    "id": format!("fc_{}", Uuid::new_v4()),
                    "type": "function_call",
                    "status": "completed",
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments,
                }));
            }
        }
    }
    (StatusCode::OK, Json(out)).into_response()
}

async fn proxy_chat_as_anthropic(
    st: AppState,
    prepared: crate::proxy_common::PreparedUpstream,
    chat_body: Value,
    requested_model: String,
    stream: bool,
) -> Response {
    let resp = match post_router_chat(&st, prepared.headers.clone(), &chat_body).await {
        Ok(r) => r,
        Err(r) => return r,
    };
    let _guard = prepared._conc_guard;

    if !resp.status().is_success() {
        let status =
            StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let bytes = resp.bytes().await.unwrap_or_default();
        if let Some(normalized) = maybe_normalize_router_error(status, &bytes) {
            return normalized;
        }
        return Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| Response::new(Body::empty()));
    }

    if stream {
        let mut upstream = resp.bytes_stream();
        let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);
        let metrics = st.metrics.clone();
        let model = requested_model.clone();

        tokio::spawn(async move {
            let msg_id = format!("msg_{}", Uuid::new_v4());
            let start = json!({
                "type": "message_start",
                "message": {
                    "id": msg_id,
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {"input_tokens": 0, "output_tokens": 0}
                }
            });
            if tx
                .send(Ok(Event::default()
                    .event("message_start")
                    .data(start.to_string())))
                .await
                .is_err()
            {
                metrics
                    .requests_aborted_total
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            let block_start = json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            });
            if tx
                .send(Ok(Event::default()
                    .event("content_block_start")
                    .data(block_start.to_string())))
                .await
                .is_err()
            {
                return;
            }

            let mut buf = String::new();
            let mut mapper = AnthropicSseMapper::new();
            loop {
                tokio::select! {
                    biased;
                    _ = tx.closed() => {
                        metrics
                            .requests_aborted_total
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        tracing::info!("anthropic SSE aborted: client disconnected");
                        break;
                    }
                    item = upstream.next() => {
                        match item {
                            Some(Ok(chunk)) => {
                                buf.push_str(&String::from_utf8_lossy(&chunk));
                                while let Some(pos) = buf.find('\n') {
                                    let mut line = buf[..pos].to_string();
                                    buf.drain(..=pos);
                                    line = line.trim().to_string();
                                    if line.is_empty() || !line.starts_with("data:") {
                                        continue;
                                    }
                                    let data = line.trim_start_matches("data:").trim();
                                    for parsed in parse_openai_sse_chunk(data) {
                                        let is_done =
                                            matches!(parsed, OpenAiStreamChunk::Done);
                                        let events = mapper.push(parsed);
                                        for (event_name, payload) in events {
                                            if tx
                                                .send(Ok(Event::default()
                                                    .event(event_name)
                                                    .data(payload.to_string())))
                                                .await
                                                .is_err()
                                            {
                                                metrics
                                                    .requests_aborted_total
                                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                                return;
                                            }
                                        }
                                        if is_done {
                                            return;
                                        }
                                    }
                                }
                            }
                            Some(Err(_)) | None => {
                                for (event_name, payload) in mapper.push(OpenAiStreamChunk::Done) {
                                    let _ = tx
                                        .send(Ok(Event::default()
                                            .event(event_name)
                                            .data(payload.to_string())))
                                        .await;
                                }
                                break;
                            }
                        }
                    }
                }
            }
        });

        return Sse::new(ReceiverStream::new(rx))
            .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
            .into_response();
    }

    let bytes = resp.bytes().await.unwrap_or_default();
    let openai: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
    let anth = openai_chat_json_to_anthropic(&openai, &requested_model);
    (StatusCode::OK, Json(anth)).into_response()
}

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub async fn not_implemented(
    State(_st): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let ctx = build_execution_context(&headers, auth.tenant_id.as_deref(), None);
    let body = json!({
        "error": {
            "message": "not implemented",
            "type": "nebula_gateway_not_implemented",
            "request_id": ctx.request_id
        }
    });
    (StatusCode::NOT_IMPLEMENTED, Json(body))
}

pub async fn proxy_post(
    State(st): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    let base = st.router_base_url.trim_end_matches('/');
    let uri_path = req.uri().path().to_string();
    let uri_query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let url = format!("{base}{uri_path}{uri_query}");

    let body_bytes = match axum::body::to_bytes(req.into_body(), st.max_request_body_bytes).await {
        Ok(b) => b,
        Err(_) => {
            st.metrics
                .request_too_large_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return payload_too_large_response();
        }
    };

    // C5 tooling gate for chat completions (embeddings/rerank payloads rarely have tools).
    if uri_path.contains("chat/completions") {
        if let Ok(payload) = serde_json::from_slice::<Value>(&body_bytes) {
            let model = payload
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(deny) = check_tooling_gate(&*st.store, &payload, model.as_deref()).await {
                return deny;
            }
        }
    }

    let prepared = match prepare_upstream(&st, &auth, &headers, &body_bytes).await {
        Ok(p) => p,
        Err(r) => return r,
    };

    let resp = match st
        .http
        .post(&url)
        .headers(prepared.headers)
        .body(body_bytes)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let kind = classify_reqwest_error(&e);
            st.metrics.record_upstream_error(kind);
            tracing::error!(error=%e, "upstream request failed");
            return upstream_transport_error(kind, format!("upstream request failed: {kind}"));
        }
    };
    let _guard = prepared._conc_guard;

    forward_upstream_response(&st, resp, Some(&prepared.request_id)).await
}

pub async fn proxy_v2(
    State(st): State<AppState>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    let bff_base = st.bff_url.trim_end_matches('/');
    let uri_path = req.uri().path().to_string();
    let rest = uri_path.strip_prefix("/v2").unwrap_or(&uri_path);
    let uri_query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let url = format!("{bff_base}/api/v2{rest}{uri_query}");
    let method = req.method().clone();

    let body_bytes = match axum::body::to_bytes(req.into_body(), st.max_request_body_bytes).await {
        Ok(b) => b,
        Err(_) => {
            st.metrics
                .request_too_large_total
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return payload_too_large_response();
        }
    };

    let reqwest_method =
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET);

    let mut req_headers = to_reqwest_headers(&headers);
    nebula_common::telemetry::inject_trace_context(&mut req_headers);

    let resp = match st
        .http
        .request(reqwest_method, &url)
        .headers(req_headers)
        .body(body_bytes)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let kind = classify_reqwest_error(&e);
            st.metrics.record_upstream_error(kind);
            tracing::error!(error=%e, url=%url, "bff proxy request failed");
            return upstream_transport_error(kind, format!("bff proxy request failed: {kind}"));
        }
    };

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = resp.headers().clone();
    let bytes = match resp.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!(error=%e, "failed to read bff response body");
            Bytes::new()
        }
    };
    let mut out = Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    append_headers(&resp_headers, &mut out);
    out
}

pub async fn admin_cluster_status(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    let nodes_raw = match st.store.list_prefix("/nodes/").await {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };
    let mut nodes = Vec::new();
    for (_, v, _) in nodes_raw {
        if let Ok(n) = serde_json::from_slice::<NodeStatus>(&v) {
            nodes.push(n);
        }
    }

    let endpoints_raw = match st.store.list_prefix("/endpoints/").await {
        Ok(e) => e,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };
    let mut endpoints = Vec::new();
    for (_, v, _) in endpoints_raw {
        if let Ok(ep) = serde_json::from_slice::<EndpointInfo>(&v) {
            endpoints.push(ep);
        }
    }

    let placements_raw = match st.store.list_prefix("/placements/").await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };
    let mut placements = Vec::new();
    for (_, v, _) in placements_raw {
        if let Ok(p) = serde_json::from_slice::<PlacementPlan>(&v) {
            placements.push(p);
        }
    }

    let requests_raw = match st.store.list_prefix("/model_requests/").await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };
    let mut model_requests = Vec::new();
    for (_, v, _) in requests_raw {
        if let Ok(r) = serde_json::from_slice::<ModelRequest>(&v) {
            model_requests.push(r);
        }
    }

    let status = ClusterStatus {
        nodes,
        endpoints,
        placements,
        model_requests,
    };

    (StatusCode::OK, Json(status)).into_response()
}

pub async fn admin_list_requests(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    let requests_raw = match st.store.list_prefix("/model_requests/").await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };
    let mut model_requests = Vec::new();
    for (_, v, _) in requests_raw {
        if let Ok(r) = serde_json::from_slice::<ModelRequest>(&v) {
            model_requests.push(r);
        }
    }
    (StatusCode::OK, Json(model_requests)).into_response()
}

pub async fn admin_whoami(Extension(ctx): Extension<AuthContext>) -> impl IntoResponse {
    let role = match ctx.role {
        Role::Admin => "admin",
        Role::Operator => "operator",
        Role::Viewer => "viewer",
    };
    (
        StatusCode::OK,
        Json(json!({
            "principal": ctx.principal,
            "role": role,
        })),
    )
        .into_response()
}

pub async fn admin_logs(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    axum::extract::Query(query): axum::extract::Query<LogsQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }

    let lines = query.lines.unwrap_or(200).min(2000);
    let content = match fs::read_to_string(&st.log_path).await {
        Ok(content) => content,
        Err(e) => {
            tracing::warn!(error=%e, path=%st.log_path, "failed to read log file");
            String::new()
        }
    };
    let mut out_lines: Vec<&str> = content.lines().rev().take(lines).collect();
    out_lines.reverse();
    (StatusCode::OK, out_lines.join("\n")).into_response()
}

pub async fn admin_logs_stream(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> Response {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }

    let log_path = st.log_path.clone();
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);

    tokio::spawn(async move {
        let file = match tokio::fs::File::open(&log_path).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(error=%e, path=%log_path, "failed to open log file for streaming");
                let _ = tx
                    .send(Ok(Event::default().data(format!("error: {}", e))))
                    .await;
                return;
            }
        };

        let mut reader = tokio::io::BufReader::new(file);
        // Seek to end of file so we only stream new lines
        if let Err(e) = reader.seek(std::io::SeekFrom::End(0)).await {
            tracing::warn!(error=%e, "failed to seek to end of log file");
            return;
        }

        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // No new data; wait and try again
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Ok(_) => {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        if tx
                            .send(Ok(Event::default().data(trimmed.to_string())))
                            .await
                            .is_err()
                        {
                            // Client disconnected
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error=%e, "error reading log file");
                    break;
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

pub async fn list_models(State(st): State<AppState>) -> impl IntoResponse {
    let placements_raw = match st.store.list_prefix("/placements/").await {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };

    let mut models = Vec::new();
    for (key, val, _) in placements_raw {
        if let Ok(plan) = serde_json::from_slice::<PlacementPlan>(&val) {
            models.push(plan.model_uid);
            continue;
        }
        if let Some(uid) = key.strip_prefix("/placements/") {
            models.push(uid.to_string());
        }
    }
    models.sort();
    models.dedup();

    let data: Vec<serde_json::Value> = models
        .into_iter()
        .map(|id| json!({"id": id, "object": "model", "owned_by": "nebula"}))
        .collect();

    (
        StatusCode::OK,
        Json(json!({"object": "list", "data": data})),
    )
        .into_response()
}

/// Resolve a caller-provided id to a model_uid.
/// Prefer `/deployments/{id}`; fall back to legacy `/model_requests/{id}` (read-only).
async fn resolve_model_uid(store: &EtcdMetaStore, id: &str) -> Result<Option<String>, String> {
    let dep_key = format!("/deployments/{id}");
    match store.get(&dep_key).await {
        Ok(Some(_)) => return Ok(Some(id.to_string())),
        Ok(None) => {}
        Err(e) => return Err(format!("etcd error: {e}")),
    }
    let req_key = format!("/model_requests/{id}");
    match store.get(&req_key).await {
        Ok(Some((data, _))) => {
            let req: ModelRequest = serde_json::from_slice(&data)
                .map_err(|e| format!("deserialization error: {e}"))?;
            Ok(Some(req.request.model_uid))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(format!("etcd error: {e}")),
    }
}

pub async fn admin_delete_request(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let model_uid = match resolve_model_uid(&st.store, &id).await {
        Ok(Some(uid)) => uid,
        Ok(None) => {
            return crate::control::control_error(nebula_control::ServiceError::NotFound(
                "model/deployment not found".to_string(),
            ));
        }
        Err(e) => {
            return crate::control::control_error(nebula_control::ServiceError::Internal(e));
        }
    };

    match nebula_control::stop_model(&*st.store, &model_uid).await {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "status": "stop_triggered",
                "model_uid": model_uid,
                "path": "deployments",
            })),
        )
            .into_response(),
        Err(e) => crate::control::control_error(e),
    }
}

pub async fn admin_load_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<ModelLoadRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }

    match nebula_control::load_model(&*st.store, &ctx.principal, req).await {
        Ok(deployment) => {
            let body = json!({
                "request_id": deployment.model_uid,
                "model_uid": deployment.model_uid,
                "status": "running_desired",
                "path": "deployments",
            });
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => crate::control::control_error(e),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ScaleRequest {
    pub replicas: u32,
}

pub async fn admin_scale_request(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<ScaleRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let model_uid = match resolve_model_uid(&st.store, &id).await {
        Ok(Some(uid)) => uid,
        Ok(None) => {
            return crate::control::control_error(nebula_control::ServiceError::NotFound(
                "model/deployment not found".to_string(),
            ));
        }
        Err(e) => {
            return crate::control::control_error(nebula_control::ServiceError::Internal(e));
        }
    };

    let old_replicas = nebula_control::get_model_deployment(&*st.store, &model_uid)
        .await
        .ok()
        .flatten()
        .map(|d| d.replicas)
        .unwrap_or(0);

    match nebula_control::scale_model(
        &*st.store,
        &model_uid,
        nebula_control::ScaleDeploymentRequest {
            replicas: body.replicas,
        },
    )
    .await
    {
        Ok(dep) => (
            StatusCode::OK,
            Json(json!({
                "request_id": id,
                "model_uid": model_uid,
                "old_replicas": old_replicas,
                "new_replicas": dep.replicas,
                "path": "deployments",
            })),
        )
            .into_response(),
        Err(e) => crate::control::control_error(e),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct DrainRequest {
    pub model_uid: String,
    pub replica_id: u32,
}

pub async fn admin_drain_endpoint(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(body): Json<DrainRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let key = format!("/endpoints/{}/{}", body.model_uid, body.replica_id);

    let (data, _) = match st.store.get(&key).await {
        Ok(Some(kv)) => kv,
        Ok(None) => return (StatusCode::NOT_FOUND, "endpoint not found").into_response(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };

    let mut ep: EndpointInfo = match serde_json::from_slice(&data) {
        Ok(ep) => ep,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("deserialization error: {}", e),
            )
                .into_response();
        }
    };

    use nebula_common::EndpointStatus;
    if ep.status == EndpointStatus::Draining {
        return (StatusCode::OK, Json(json!({"status": "already_draining"}))).into_response();
    }

    ep.status = EndpointStatus::Draining;
    let val = match serde_json::to_vec(&ep) {
        Ok(val) => val,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("serialization error: {}", e),
            )
                .into_response();
        }
    };
    if let Err(e) = st.store.put(&key, val, None).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("etcd error: {}", e),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "model_uid": body.model_uid,
            "replica_id": body.replica_id,
            "status": "draining",
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Image Registry CRUD
// ---------------------------------------------------------------------------

pub async fn admin_list_images(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    let kvs = match st.store.list_prefix("/images/").await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };
    let images: Vec<nebula_common::EngineImage> = kvs
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    (StatusCode::OK, Json(json!(images))).into_response()
}

pub async fn admin_get_image(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    let key = format!("/images/{}", id);
    match st.store.get(&key).await {
        Ok(Some((data, _))) => match serde_json::from_slice::<nebula_common::EngineImage>(&data) {
            Ok(img) => (StatusCode::OK, Json(json!(img))).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("deserialization error: {}", e),
            )
                .into_response(),
        },
        Ok(None) => (StatusCode::NOT_FOUND, "image not found").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("etcd error: {}", e),
        )
            .into_response(),
    }
}

pub async fn admin_put_image(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(mut img): Json<nebula_common::EngineImage>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    img.id = id.clone();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if img.created_at_ms == 0 {
        img.created_at_ms = now;
    }
    img.updated_at_ms = now;

    let val = match serde_json::to_vec(&img) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("serialization error: {}", e),
            )
                .into_response();
        }
    };
    let key = format!("/images/{}", id);
    if let Err(e) = st.store.put(&key, val, None).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("etcd error: {}", e),
        )
            .into_response();
    }
    (StatusCode::OK, Json(json!(img))).into_response()
}

pub async fn admin_delete_image(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let key = format!("/images/{}", id);
    if let Err(e) = st.store.delete(&key).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("etcd error: {}", e),
        )
            .into_response();
    }
    // Also clean up image_status entries for this image
    let status_prefix = format!("/image_status/");
    if let Ok(kvs) = st.store.list_prefix(&status_prefix).await {
        for (k, _, _) in kvs {
            if k.ends_with(&format!("/{}", id)) {
                let _ = st.store.delete(&k).await;
            }
        }
    }
    (StatusCode::OK, Json(json!({"id": id, "status": "deleted"}))).into_response()
}

pub async fn admin_list_image_status(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    let kvs = match st.store.list_prefix("/image_status/").await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };
    let statuses: Vec<nebula_common::NodeImageStatus> = kvs
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    (StatusCode::OK, Json(json!(statuses))).into_response()
}

pub async fn admin_audit_logs(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    axum::extract::Query(query): axum::extract::Query<crate::audit::AuditLogQuery>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Admin) {
        return resp;
    }

    match crate::audit::fetch_audit_logs(&st, &query).await {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(resp) => resp,
    }
}
