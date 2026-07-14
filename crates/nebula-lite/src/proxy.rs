use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
pub struct AppState {
    pub http: Client,
    pub engine_base_url: String,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/models", get(proxy_get))
        .route("/v1/chat/completions", post(proxy_post))
        .route("/v1/completions", post(proxy_post))
        .route("/v1/embeddings", post(proxy_post))
        .with_state(Arc::new(state))
}

async fn healthz(State(st): State<Arc<AppState>>) -> impl IntoResponse {
    let url = format!("{}/health", st.engine_base_url.trim_end_matches('/'));
    match st.http.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => (StatusCode::OK, "ok"),
        Ok(_) => (StatusCode::SERVICE_UNAVAILABLE, "engine unhealthy"),
        Err(_) => {
            // Some engines omit /health; fall back to /v1/models.
            let models = format!("{}/v1/models", st.engine_base_url.trim_end_matches('/'));
            match st.http.get(&models).send().await {
                Ok(r) if r.status().is_success() => (StatusCode::OK, "ok"),
                _ => (StatusCode::SERVICE_UNAVAILABLE, "engine unreachable"),
            }
        }
    }
}

async fn proxy_get(State(st): State<Arc<AppState>>, req: Request<Body>) -> Response {
    forward(&st, req, false).await
}

async fn proxy_post(State(st): State<Arc<AppState>>, req: Request<Body>) -> Response {
    forward(&st, req, true).await
}

async fn forward(st: &AppState, req: Request<Body>, is_post: bool) -> Response {
    let base = st.engine_base_url.trim_end_matches('/');
    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let url = format!("{base}{path}{query}");

    let headers = filter_request_headers(req.headers());
    let body_bytes = if is_post {
        match axum::body::to_bytes(req.into_body(), 64 * 1024 * 1024).await {
            Ok(b) => b,
            Err(_) => {
                return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
            }
        }
    } else {
        Bytes::new()
    };

    let builder = if is_post {
        st.http.post(&url).headers(headers).body(body_bytes)
    } else {
        st.http.get(&url).headers(headers)
    };

    let resp = match builder.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error=%e, %url, "upstream request failed");
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
            loop {
                tokio::select! {
                    biased;
                    _ = tx.closed() => break,
                    item = upstream.next() => {
                        match item {
                            Some(Ok(b)) => {
                                if tx.send(Ok(b)).await.is_err() {
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
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error=%e, "failed to read upstream body");
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

fn filter_request_headers(inbound: &HeaderMap) -> reqwest::header::HeaderMap {
    let mut out = reqwest::header::HeaderMap::new();
    for (name, value) in inbound.iter() {
        let lname = name.as_str();
        if lname == "host"
            || lname == "connection"
            || lname == "content-length"
            || lname == "transfer-encoding"
        {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            reqwest::header::HeaderName::from_bytes(name.as_str().as_bytes()),
            reqwest::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            out.append(n, v);
        }
    }
    out
}

fn append_headers(from: &reqwest::header::HeaderMap, to: &mut Response) {
    for (name, value) in from.iter() {
        let lname = name.as_str();
        if lname == "transfer-encoding" || lname == "content-length" || lname == "connection" {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            to.headers_mut().append(n, v);
        }
    }
}
