mod args;
mod audit;
mod auth;
mod control;
mod engine;
mod handlers;
mod interface;
mod metrics;
mod platform_auth;
mod platform_idempotency;
mod platform_v1;
mod platform_webhooks;
mod protocol_adapt;
mod proxy_common;
mod responses;
mod state;
mod util;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{get, post, put},
    Router,
};
use clap::Parser;
use nebula_common::proxy_http_client;

use crate::args::Args;
use crate::audit::AuditWriter;
use crate::engine::{EngineClient, OpenAIEngineClient};
use crate::handlers::{
    create_anthropic_messages, create_responses, healthz, list_models, proxy_post,
};
use crate::metrics::{metrics_handler, track_requests};
use crate::platform_auth::build_gateway_auth;
use crate::platform_v1::{
    platform_audit_logs, platform_cluster_status, platform_create_model, platform_create_pool,
    platform_delete_pool, platform_drain_node, platform_drain_replica, platform_evaluate_slo,
    platform_get_canary, platform_get_deployment, platform_get_model, platform_get_operation,
    platform_get_pool, platform_get_slo, platform_health_summary, platform_list_canaries,
    platform_list_models, platform_list_nodes, platform_list_pools, platform_list_replicas,
    platform_load_model, platform_operation_events, platform_put_deployment,
    platform_scale_deployment, platform_stop_model, platform_update_pool, platform_whoami,
};
use crate::platform_webhooks::{
    platform_create_webhook, platform_delete_webhook, platform_list_webhooks,
};
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

    let http = proxy_http_client().unwrap_or_else(|e| {
        tracing::error!(error=%e, "failed to build reqwest client");
        std::process::exit(1);
    });

    let store = match nebula_meta::EtcdMetaStore::connect(&args.common.etcd_endpoints()).await {
        Ok(store) => store,
        Err(e) => {
            tracing::error!(error=%e, "failed to connect to etcd");
            return;
        }
    };

    let auth = build_gateway_auth().await;

    let metrics = Arc::new(metrics::Metrics::default());
    let dual_write = nebula_common::DualWriteEmitter::from_env(
        "nebula-gateway",
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
    );
    let max_request_body_bytes = std::env::var("NEBULA_GATEWAY_MAX_REQUEST_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4 * 1024 * 1024);

    let audit = AuditWriter::spawn(
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
    );

    let st = AppState {
        _noop: Arc::new(()),
        engine,
        router_base_url,
        http,
        store: Arc::new(store),
        auth,
        metrics,
        dual_write,
        max_request_body_bytes,
        log_path: args.log_path,
        audit,
        xtrace_url: args.common.xtrace_url.clone(),
        xtrace_token: args.common.xtrace_token.clone(),
        bff_url: args.bff_url,
        tenant_admission: nebula_common::TenantAdmission::new(),
    };

    let platform_routes = Router::new()
        .route("/models", get(platform_list_models).post(platform_create_model))
        .route("/models/load", post(platform_load_model))
        .route("/models/:model_uid", get(platform_get_model))
        .route(
            "/models/:model_uid/deployment",
            get(platform_get_deployment).put(platform_put_deployment),
        )
        .route(
            "/models/:model_uid/deployment/scale",
            post(platform_scale_deployment),
        )
        .route("/models/:model_uid/stop", post(platform_stop_model))
        .route("/models/:model_uid/replicas", get(platform_list_replicas))
        .route("/nodes", get(platform_list_nodes))
        .route("/operations/:operation_id", get(platform_get_operation))
        .route("/operations/:operation_id/events", get(platform_operation_events))
        .route("/health/summary", get(platform_health_summary))
        .route("/cluster/status", get(platform_cluster_status))
        .route("/whoami", get(platform_whoami))
        .route("/replicas/drain", post(platform_drain_replica))
        .route("/nodes/:node_id/drain", post(platform_drain_node))
        .route("/pools", get(platform_list_pools).post(platform_create_pool))
        .route(
            "/pools/:pool_id",
            get(platform_get_pool)
                .put(platform_update_pool)
                .delete(platform_delete_pool),
        )
        .route("/audit-logs", get(platform_audit_logs))
        .route("/models/:model_uid/slo/evaluation", get(platform_evaluate_slo))
        .route("/models/:model_uid/slo", get(platform_get_slo))
        .route("/canaries", get(platform_list_canaries))
        .route("/canaries/:canary_id", get(platform_get_canary))
        .route("/webhooks", get(platform_list_webhooks).post(platform_create_webhook))
        .route("/webhooks/:webhook_id", axum::routing::delete(platform_delete_webhook))
        .with_state(st.clone());

    let secure_routes = Router::new()
        .route("/v1/responses", post(create_responses))
        .route("/v1/messages", post(create_anthropic_messages))
        .route("/v1/chat/completions", post(proxy_post))
        .route("/v1/completions", post(proxy_post))
        .route("/v1/embeddings", post(proxy_post))
        .route("/v1/rerank", post(proxy_post))
        .route("/v1/models", get(list_models))
        .nest("/platform/v1", platform_routes)
        .layer(middleware::from_fn_with_state(
            st.clone(),
            audit::audit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            st.clone(),
            crate::platform_auth::gateway_auth_middleware,
        ))
        .layer(middleware::from_fn(
            nebula_common::telemetry::trace_context_middleware,
        ));

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
