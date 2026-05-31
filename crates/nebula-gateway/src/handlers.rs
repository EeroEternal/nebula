use std::convert::Infallible;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    Extension, Json,
};
use bytes::Bytes;
use serde_json::json;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt};
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use uuid::Uuid;

use nebula_common::{
    ClusterStatus, EndpointInfo, ExecutionContext, ModelLoadRequest, ModelRequest,
    ModelRequestStatus, NodeStatus, PlacementPlan,
};
use nebula_meta::MetaStore;

use crate::auth::{require_role, AuthContext, Role};
use crate::responses::{
    build_non_stream_json, build_response, CreateResponseRequest, ResponseStreamBuilder,
};
use crate::state::AppState;

#[derive(Debug, serde::Deserialize)]
pub(crate) struct LogsQuery {
    lines: Option<usize>,
}

pub async fn create_responses(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateResponseRequest>,
) -> Response {
    let _ctx = build_execution_context(&headers);

    let input_text = req.extract_input_text();

    if req.stream.unwrap_or(false) {
        let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(256);
        let engine = st.engine.clone();

        let builder = ResponseStreamBuilder::new(&req);

        tokio::spawn(async move {
            let mut stream = engine.stream_text(input_text);
            let mut builder = builder;

            let _ = tx
                .send(Ok(
                    Event::default().data(builder.created_event().to_string())
                ))
                .await;

            while let Some(delta) = stream.next().await {
                if delta.is_empty() {
                    continue;
                }
                let ev = builder.push_delta(delta);
                let _ = tx.send(Ok(Event::default().data(ev.to_string()))).await;
            }

            let completed = builder.completed_event();
            let _ = tx
                .send(Ok(Event::default().data(completed.to_string())))
                .await;
        });

        Sse::new(ReceiverStream::new(rx))
            .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
            .into_response()
    } else {
        let mut out = String::new();
        let mut stream = st.engine.stream_text(input_text);
        while let Some(delta) = stream.next().await {
            if !delta.is_empty() {
                out.push_str(&delta);
            }
        }
        let built = build_response(&req, out);
        let body = build_non_stream_json(&built);
        (StatusCode::OK, Json(body)).into_response()
    }
}

pub fn build_execution_context(headers: &HeaderMap) -> ExecutionContext {
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    ExecutionContext {
        request_id: format!("req_{}", Uuid::new_v4()),
        session_id,
        tenant_id: None,
        priority: None,
        deadline_ms: None,
        budget_tokens: None,
    }
}

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub async fn not_implemented(State(_st): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let ctx = build_execution_context(&headers);
    let body = json!({
        "error": {
            "message": "not implemented",
            "type": "nebula_gateway_not_implemented",
            "request_id": ctx.request_id
        }
    });
    (StatusCode::NOT_IMPLEMENTED, Json(body))
}

fn to_reqwest_headers(headers: &HeaderMap) -> reqwest::header::HeaderMap {
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

fn append_headers(src: &reqwest::header::HeaderMap, dst: &mut Response) {
    for (k, v) in src.iter() {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(k.as_str().as_bytes()),
            HeaderValue::from_bytes(v.as_bytes()),
        ) {
            dst.headers_mut().insert(name, value);
        }
    }
}

fn classify_reqwest_error(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "timeout";
    }
    if error.is_connect() {
        return "connect";
    }
    "other"
}

pub async fn proxy_post(
    State(st): State<AppState>,
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
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };

    let mut req_headers = to_reqwest_headers(&headers);
    nebula_common::telemetry::inject_trace_context(&mut req_headers);

    let resp = match st
        .http
        .post(url)
        .headers(req_headers)
        .body(body_bytes)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let kind = classify_reqwest_error(&e);
            st.metrics.record_upstream_error(kind);
            tracing::error!(error=%e, "upstream request failed");
            return (StatusCode::BAD_GATEWAY, "upstream request failed").into_response();
        }
    };

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
        tokio::spawn(async move {
            while let Some(item) = upstream.next().await {
                match item {
                    Ok(b) => {
                        let _ = tx.send(Ok(b)).await;
                    }
                    Err(_) => break,
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
    let mut out = Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap_or_else(|_| Response::new(Body::empty()));
    append_headers(&resp_headers, &mut out);
    out
}

pub async fn proxy_v2(
    State(st): State<AppState>,
    headers: HeaderMap,
    req: Request<Body>,
) -> Response {
    let bff_base = st.bff_url.trim_end_matches('/');
    let uri_path = req.uri().path().to_string();
    let rest = uri_path
        .strip_prefix("/v2")
        .unwrap_or(&uri_path);
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
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
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
            return (StatusCode::BAD_GATEWAY, "bff proxy request failed").into_response();
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
                        if tx.send(Ok(Event::default().data(trimmed.to_string()))).await.is_err() {
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

pub async fn admin_delete_request(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let key = format!("/model_requests/{}", id);

    let (data, _) = match st.store.get(&key).await {
        Ok(Some(kv)) => kv,
        Ok(None) => return (StatusCode::NOT_FOUND, "request not found").into_response(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };

    let mut req: ModelRequest = match serde_json::from_slice(&data) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("deserialization error: {}", e),
            )
                .into_response();
        }
    };

    req.status = ModelRequestStatus::Unloading;
    let val = match serde_json::to_vec(&req) {
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
        Json(json!({"status": "unloading_triggered"})),
    )
        .into_response()
}

pub async fn admin_load_model(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<ModelLoadRequest>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Operator) {
        return resp;
    }
    let request_id = Uuid::new_v4().to_string();
    let model_req = ModelRequest {
        id: request_id.clone(),
        request: req,
        status: ModelRequestStatus::Pending,
        created_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };

    let key = format!("/model_requests/{}", model_req.id);
    let val = match serde_json::to_vec(&model_req) {
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

    let body = json!({
        "request_id": request_id,
        "status": "pending"
    });
    (StatusCode::OK, Json(body)).into_response()
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
    let key = format!("/model_requests/{}", id);

    let (data, _) = match st.store.get(&key).await {
        Ok(Some(kv)) => kv,
        Ok(None) => return (StatusCode::NOT_FOUND, "request not found").into_response(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("etcd error: {}", e),
            )
                .into_response();
        }
    };

    let mut req: ModelRequest = match serde_json::from_slice(&data) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("deserialization error: {}", e),
            )
                .into_response();
        }
    };

    let old = req.request.replicas;
    req.request.replicas = body.replicas;
    let val = match serde_json::to_vec(&req) {
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
            "request_id": id,
            "old_replicas": old,
            "new_replicas": body.replicas,
        })),
    )
        .into_response()
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
        return (
            StatusCode::OK,
            Json(json!({"status": "already_draining"})),
        )
            .into_response();
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
    (
        StatusCode::OK,
        Json(json!({"id": id, "status": "deleted"})),
    )
        .into_response()
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

    let (xtrace_url, xtrace_token) = match (&st.xtrace_url, &st.xtrace_token) {
        (Some(u), Some(t)) => (u.clone(), t.clone()),
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"error": "audit logging not configured (xtrace not set)"})),
            )
                .into_response();
        }
    };

    let mut params = vec![("tags[]", "audit".to_string())];
    if let Some(p) = query.page {
        params.push(("page", p.to_string()));
    }
    params.push(("limit", query.limit.unwrap_or(50).min(200).to_string()));
    if let Some(u) = &query.user_id {
        params.push(("userId", u.clone()));
    }
    if let Some(f) = &query.from {
        params.push(("fromTimestamp", f.clone()));
    }
    if let Some(t) = &query.to {
        params.push(("toTimestamp", t.clone()));
    }

    let base = xtrace_url.trim_end_matches('/');
    let qs = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");
    let url = format!("{base}/api/public/traces?{qs}");

    let resp = match st.http.get(&url).bearer_auth(&xtrace_token).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("xtrace request failed: {}", e)})),
            )
                .into_response();
        }
    };

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("xtrace parse error: {}", e)})),
            )
                .into_response();
        }
    };

    (status, Json(body)).into_response()
}
