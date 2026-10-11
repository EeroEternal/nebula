mod args;
mod handlers;
mod metrics;
mod state;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use clap::Parser;
use nebula_common::proxy_http_client;

use crate::args::Args;
use crate::handlers::{healthz, proxy_chat_completions};
use crate::metrics::{metrics_handler, track_requests};
use crate::state::AppState;
use nebula_router::sync::{
    endpoints_sync_loop, models_sync_loop, placement_sync_loop, stats_sync_loop,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let _otel_guard = nebula_common::telemetry::init_tracing(
        "nebula-router",
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
        &args.common.log_format,
    );

    let store = nebula_meta::EtcdMetaStore::connect(&args.common.etcd_endpoints()).await?;

    let strategy =
        nebula_router::strategy::parse_strategy(&args.routing_strategy).unwrap_or_else(|e| {
            tracing::error!(error=%e, "invalid routing strategy");
            std::process::exit(1);
        });
    let router = nebula_router::Router::with_strategy(strategy);

    let router_for_sync = router.clone();
    let store_for_endpoints = store.clone();
    let store_for_placement = store.clone();
    let router_for_placement = router.clone();

    tokio::spawn(async move {
        if let Err(e) = endpoints_sync_loop(store_for_endpoints, router_for_sync).await {
            tracing::error!(error=%e, "endpoints sync loop exited");
        }
    });

    tokio::spawn(async move {
        if let Err(e) = placement_sync_loop(store_for_placement, router_for_placement).await {
            tracing::error!(error=%e, "placement sync loop exited");
        }
    });

    let store_for_models = store.clone();
    let router_for_models = router.clone();
    tokio::spawn(async move {
        if let Err(e) = models_sync_loop(store_for_models, router_for_models).await {
            tracing::error!(error=%e, "model specs sync loop exited");
        }
    });

    let router_for_stats = router.clone();
    let store_for_stats = store.clone();
    tokio::spawn(async move {
        if let Err(e) = stats_sync_loop(store_for_stats, router_for_stats).await {
            tracing::error!(error=%e, "stats sync loop exited");
        }
    });

    let http = proxy_http_client().unwrap_or_else(|e| {
        tracing::error!(error=%e, "failed to build reqwest client");
        std::process::exit(1);
    });

    let metrics = Arc::new(metrics::Metrics::default());
    let dual_write = nebula_common::DualWriteEmitter::from_env(
        "nebula-router",
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
    );

    let max_request_body_bytes = std::env::var("NEBULA_ROUTER_MAX_REQUEST_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4 * 1024 * 1024);
    let retry_max = std::env::var("NEBULA_ROUTER_RETRY_MAX")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1);
    let retry_backoff_ms = std::env::var("NEBULA_ROUTER_RETRY_BACKOFF_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(75);

    let auth = nebula_common::auth::parse_auth_from_env();

    // P1c: KV-event ingestion (opt-in). Engines publish block store/remove
    // events over zmq; the router maintains the prefix→replica index. Sources:
    // BIND (engines connect here) and/or CONNECT (peer router's fanout — HA);
    // FANOUT re-publishes the stream for peer routers.
    let kv_bind = std::env::var("NEBULA_ROUTER_KV_EVENTS_BIND").ok();
    let kv_connect = std::env::var("NEBULA_ROUTER_KV_EVENTS_CONNECT").ok();
    let kv_fanout = std::env::var("NEBULA_ROUTER_KV_EVENTS_FANOUT").ok();
    if kv_bind.is_some() || kv_connect.is_some() {
        let idx = router.prefix_index().clone();
        tokio::spawn(async move {
            if let Err(e) =
                nebula_router::kv_events::kv_events_loop(kv_bind, kv_connect, kv_fanout, idx).await
            {
                tracing::error!(error = %e, "kv events loop exited");
            }
        });
        let ttl_ms = std::env::var("NEBULA_ROUTER_PREFIX_TTL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300_000);
        let idx = router.prefix_index().clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let evicted = idx.evict_expired(now, ttl_ms);
                if evicted > 0 {
                    tracing::debug!(evicted, "prefix index ttl sweep");
                }
            }
        });
    }

    let st = AppState {
        model_uid: args.model_uid,
        router,
        http,
        metrics,
        dual_write,
        max_request_body_bytes,
        retry_max,
        retry_backoff_ms,
        auth,
    };

    // Routes that require auth (inference endpoints)
    let authed_routes = Router::new()
        .route("/v1/chat/completions", post(proxy_chat_completions))
        .route("/v1/completions", post(proxy_chat_completions))
        .route("/v1/embeddings", post(proxy_chat_completions))
        .route("/v1/rerank", post(proxy_chat_completions))
        .route(
            "/v1/models",
            post(proxy_chat_completions).get(proxy_chat_completions),
        )
        .layer(middleware::from_fn_with_state(
            st.clone(),
            nebula_common::auth::auth_middleware::<AppState>,
        ));

    // Routes that do NOT require auth (health/metrics)
    let public_routes = Router::new()
        .route("/healthz", get(healthz))
        .route("/health", get(healthz))
        .route("/metrics", get(metrics_handler));

    let app = public_routes
        .merge(authed_routes)
        .layer(middleware::from_fn_with_state(st.clone(), track_requests))
        .layer(middleware::from_fn(
            nebula_common::telemetry::trace_context_middleware,
        ))
        .with_state(st);

    let listener = tokio::net::TcpListener::bind(&args.listen_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
