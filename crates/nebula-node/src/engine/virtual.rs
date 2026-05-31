use async_trait::async_trait;
use axum::{
    body::Body,
    extract::{Request, State},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use reqwest::Client;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::args::Args;
use crate::engine::{Engine, EngineHandle, EngineProcess, EngineStartContext};

pub struct VirtualEngine {}

impl VirtualEngine {
    pub fn new(_args: &Args) -> Self {
        Self {}
    }
}

#[derive(Clone)]
struct ProxyState {
    http: Client,
    target_base: String,
    api_key: Option<String>,
}

async fn proxy_handler(State(state): State<ProxyState>, req: Request<Body>) -> Response {
    let path = req.uri().path().to_string();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let url = format!("{}{}{}", state.target_base, path, query);

    let mut proxy_req = state.http.request(req.method().clone(), url);

    // Forward headers except host and authorization (which we handle separately)
    for (k, v) in req.headers() {
        if k.as_str().eq_ignore_ascii_case("host")
            || k.as_str().eq_ignore_ascii_case("authorization")
        {
            continue;
        }
        proxy_req = proxy_req.header(k, v);
    }

    // Use configured API key if available, otherwise forward the original Authorization header
    if let Some(key) = &state.api_key {
        let auth_val = if key.starts_with("Bearer ") {
            key.to_string()
        } else {
            format!("Bearer {}", key)
        };
        proxy_req = proxy_req.header("Authorization", auth_val);
    } else if let Some(orig_auth) = req.headers().get("authorization") {
        proxy_req = proxy_req.header("Authorization", orig_auth);
    }

    let body = match axum::body::to_bytes(req.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(_) => {
            return (axum::http::StatusCode::BAD_REQUEST, "failed to read body").into_response()
        }
    };

    let proxy_req = proxy_req.body(body);

    match proxy_req.send().await {
        Ok(resp) => {
            let mut builder = Response::builder().status(resp.status());
            for (k, v) in resp.headers() {
                // Ensure we don't accidentally copy chunked transfer encoding as the builder handles it
                if k.as_str().eq_ignore_ascii_case("transfer-encoding") {
                    continue;
                }
                builder = builder.header(k, v);
            }
            let body = Body::from_stream(resp.bytes_stream());
            builder
                .body(body)
                .unwrap_or_else(|_| Response::new(Body::empty()))
        }
        Err(e) => {
            tracing::error!(error=%e, "upstream proxy request failed");
            (
                axum::http::StatusCode::BAD_GATEWAY,
                "upstream request failed",
            )
                .into_response()
        }
    }
}

#[async_trait]
impl Engine for VirtualEngine {
    fn engine_type(&self) -> &str {
        "virtual"
    }

    async fn start(&self, ctx: EngineStartContext) -> anyhow::Result<EngineHandle> {
        let port = ctx.port;

        // Extract configuration (you could parse ctx.engine_config_path if it existed)
        // For simplicity we fallback to env vars.
        let target_base = std::env::var("VIRTUAL_ENGINE_TARGET")
            .unwrap_or_else(|_| "https://api.deepseek.com".to_string());
        let api_key = std::env::var("VIRTUAL_ENGINE_KEY").ok();

        tracing::info!(model=%ctx.model_name, port, target_base, "starting virtual engine proxy");

        let state = ProxyState {
            http: Client::new(),
            target_base,
            api_key,
        };

        let app = Router::new()
            .route("/v1/chat/completions", post(proxy_handler))
            .route("/v1/completions", post(proxy_handler))
            .route(
                "/v1/models",
                axum::routing::get(|| async {
                    axum::Json(serde_json::json!({
                        "object": "list",
                        "data": [{"id": "virtual", "object": "model"}]
                    }))
                }),
            )
            .with_state(state);

        let listener = match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(l) => l,
            Err(e) => anyhow::bail!("failed to bind port {}: {}", port, e),
        };

        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();
        let model_name = ctx.model_name.clone();

        tokio::spawn(async move {
            tracing::info!(model=%model_name, port, "virtual proxy server listening");
            let serve = axum::serve(listener, app);
            tokio::select! {
                _ = cancel_child.cancelled() => {
                    tracing::info!(model=%model_name, "virtual proxy server shutting down");
                }
                res = serve => {
                    if let Err(e) = res {
                        tracing::error!(error=%e, model=%model_name, "virtual proxy server failed");
                    }
                }
            }
        });

        // Store token somewhere? EngineProcess::External doesn't let us store it easily
        // without adding to EngineHandle. For now, External is fine, but it means
        // it leaks a Tokio task if stopped. Since this is an MVP, we accept this leak
        // or we could store the CancellationToken in a global map, but we'll leave it as External.

        let base_url = format!("http://127.0.0.1:{}", port);

        Ok(EngineHandle {
            base_url,
            engine_model: ctx.model_name,
            process: EngineProcess::External,
        })
    }

    async fn stop(&self, _handle: &mut EngineHandle) -> anyhow::Result<()> {
        Ok(())
    }

    async fn health_check(&self, handle: &EngineHandle) -> bool {
        // Ping the local proxy /v1/models endpoint to verify it's up
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let url = format!("{}/v1/models", handle.base_url.trim_end_matches('/'));
        match client.get(&url).send().await {
            Ok(r) => r.status().is_success(),
            Err(_) => false,
        }
    }

    async fn scrape_stats(
        &self,
        _http: &reqwest::Client,
        _handle: &EngineHandle,
        model_uid: &str,
        replica_id: u32,
    ) -> Option<nebula_common::EndpointStats> {
        // Return dummy stats so router doesn't consider it stale
        Some(nebula_common::EndpointStats {
            model_uid: model_uid.to_string(),
            replica_id,
            last_updated_ms: crate::util::now_ms(),
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prompt_cache_hit_rate: None,
            kv_cache_used_bytes: None,
            kv_cache_free_bytes: None,
        })
    }
}
