use std::convert::Infallible;
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    Extension, Json,
};
use bytes::Bytes;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use uuid::Uuid;

use nebula_common::{build_execution_context, PlacementPlan};
use nebula_meta::MetaStore;

use crate::auth::AuthContext;
use crate::interface::{
    anthropic_json_to_openai_chat, check_tooling_gate, openai_chat_json_to_anthropic,
    parse_openai_sse_chunk, payload_too_large_response, responses_json_to_openai_chat,
    maybe_normalize_router_error, upstream_transport_error, AnthropicSseMapper, OpenAiStreamChunk,
};
use crate::proxy_common::{
    append_headers, classify_reqwest_error, forward_upstream_response, prepare_upstream,
    prepare_upstream_from_json, post_router_chat,
};
use crate::responses::{build_non_stream_json, build_response, ResponseStreamBuilder};
use crate::state::AppState;

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
