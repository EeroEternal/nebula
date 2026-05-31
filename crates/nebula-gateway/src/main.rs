mod args;
mod audit;
mod auth;
mod engine;
mod handlers;
mod metrics;
mod responses;
mod state;
mod util;

use std::sync::Arc;
use std::time::Duration;

use axum::{
    middleware,
    routing::{any, get, post, put},
    Router,
};
use clap::Parser;

use crate::args::Args;
use crate::audit::AuditWriter;
use nebula_common::auth::parse_auth_from_env;
use crate::engine::{EngineClient, OpenAIEngineClient};
use crate::handlers::{
    admin_audit_logs, admin_cluster_status, admin_delete_image, admin_delete_request,
    admin_drain_endpoint, admin_get_image, admin_list_image_status, admin_list_images,
    admin_list_requests, admin_load_model, admin_logs, admin_logs_stream, admin_put_image,
    admin_scale_request, admin_whoami, create_responses, healthz, list_models, not_implemented,
    proxy_post, proxy_v2,
};
use crate::metrics::{metrics_handler, track_requests};
use crate::state::AppState;
use crate::util::read_engine_env_file;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let _otel_guard = nebula_common::telemetry::init_tracing(
        "nebula-gateway",
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
        &args.common.log_format,
    );
    let router_base_url = args.router_url;

    let engine_model = match args.engine_model {
        Some(model) => model,
        None => read_engine_env_file("/tmp/nebula/engine.env")
            .await
            .map(|(_url, model)| model)
            .unwrap_or_else(|| "unknown".to_string()),
    };

    tracing::info!(router_base_url=%router_base_url, engine_model=%engine_model, "gateway starting");

    let engine: Arc<dyn EngineClient> = Arc::new(OpenAIEngineClient::new(
        router_base_url.clone(),
        engine_model,
    ));

    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(300))
        .build()
        .unwrap_or_else(|e| {
            tracing::error!(error=%e, "failed to build reqwest client");
            std::process::exit(1);
        });

    let store = match nebula_meta::EtcdMetaStore::connect(&[args.common.etcd_endpoint]).await {
        Ok(store) => store,
        Err(e) => {
            tracing::error!(error=%e, "failed to connect to etcd");
            return;
        }
    };

    let auth = parse_auth_from_env();

    let metrics = Arc::new(metrics::Metrics::default());
    let max_request_body_bytes = std::env::var("NEBULA_GATEWAY_MAX_REQUEST_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4 * 1024 * 1024);

    let audit = AuditWriter::spawn(args.common.xtrace_url.as_deref(), args.common.xtrace_token.as_deref());

    let st = AppState {
        _noop: Arc::new(()),
        engine,
        router_base_url,
        http,
        store: Arc::new(store),
        auth,
        metrics,
        max_request_body_bytes,
        log_path: args.log_path,
        audit,
        xtrace_url: args.common.xtrace_url.clone(),
        xtrace_token: args.common.xtrace_token.clone(),
        bff_url: args.bff_url,
    };

    let admin_routes = Router::new()
        .route("/models/load", post(admin_load_model))
        .route("/models/requests", get(admin_list_requests))
        .route(
            "/models/requests/:id",
            post(admin_delete_request).delete(admin_delete_request),
        )
        .route("/cluster/status", get(admin_cluster_status))
        .route("/whoami", get(admin_whoami))
        .route("/metrics", get(metrics::admin_metrics))
        .route("/logs", get(admin_logs))
        .route("/logs/stream", get(admin_logs_stream))
        .route("/models/requests/:id/scale", put(admin_scale_request))
        .route("/endpoints/drain", post(admin_drain_endpoint))
        .route("/audit-logs", get(admin_audit_logs))
        // Image registry
        .route("/images", get(admin_list_images))
        .route(
            "/images/:id",
            get(admin_get_image)
                .put(admin_put_image)
                .delete(admin_delete_image),
        )
        .route("/images/status", get(admin_list_image_status))
        // v2 proxy routes (forwarded to BFF)
        .route("/v2/models", any(proxy_v2))
        .route("/v2/models/:model_uid", any(proxy_v2))
        .route("/v2/models/:model_uid/start", any(proxy_v2))
        .route("/v2/models/:model_uid/stop", any(proxy_v2))
        .route("/v2/models/:model_uid/scale", any(proxy_v2))
        .route("/v2/models/:model_uid/save-as-template", any(proxy_v2))
        .route("/v2/templates", any(proxy_v2))
        .route("/v2/templates/:id", any(proxy_v2))
        .route("/v2/templates/:id/deploy", any(proxy_v2))
        .route("/v2/nodes/:node_id/cache", any(proxy_v2))
        .route("/v2/nodes/:node_id/disk", any(proxy_v2))
        .route("/v2/cache/summary", any(proxy_v2))
        .route("/v2/alerts", any(proxy_v2))
        .route("/v2/migrate", any(proxy_v2))
        .with_state(st.clone());

    let secure_routes = Router::new()
        .route("/v1/responses", get(not_implemented).post(create_responses))
        .route("/v1/chat/completions", post(proxy_post))
        .route("/v1/embeddings", post(proxy_post))
        .route("/v1/rerank", post(proxy_post))
        .route("/v1/models", get(list_models))
        .nest("/v1/admin", admin_routes)
        .layer(middleware::from_fn_with_state(st.clone(), audit::audit_middleware))
        .layer(middleware::from_fn_with_state(st.clone(), nebula_common::auth::auth_middleware::<AppState>))
        .layer(middleware::from_fn(nebula_common::telemetry::trace_context_middleware));

    let public_routes = Router::new()
        .route("/healthz", get(healthz))
        .route("/health", get(healthz))
        .route("/metrics", get(metrics_handler));

    let app = public_routes
        .merge(secure_routes)
        .layer(middleware::from_fn_with_state(st.clone(), track_requests))
        .with_state(st);

    let addr = args.listen_addr;

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!(error=%e, addr=%addr, "failed to bind gateway address");
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!(error=%e, "gateway server exited");
    }
}
