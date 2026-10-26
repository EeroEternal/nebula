mod args;
mod audit;
mod auth;
mod control;
mod handlers;
mod interface;
mod metrics;
mod platform_auth;
mod platform_idempotency;
mod platform_v1;
mod platform_webhooks;
mod protocol_adapt;
mod proxy_common;
mod quota_members;
mod responses;
mod state;
mod util;

use std::sync::Arc;
use std::time::Duration;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use clap::Parser;
use nebula_common::proxy_http_client;

use crate::args::Args;
use crate::audit::AuditWriter;
use crate::handlers::{
    create_anthropic_messages, create_responses, healthz, list_models, proxy_post,
};
use crate::metrics::{metrics_handler, track_requests};
use crate::platform_auth::build_gateway_auth;
use crate::platform_v1::{
    platform_audit_logs, platform_cluster_status, platform_create_model, platform_create_pool,
    platform_delete_pool, platform_drain_node, platform_drain_replica, platform_evaluate_slo,
    platform_evict_model, platform_get_canary, platform_get_deployment, platform_get_model,
    platform_get_operation, platform_get_pool, platform_get_slo, platform_health_summary,
    platform_list_canaries, platform_list_models, platform_list_nodes, platform_list_pools,
    platform_list_replicas, platform_load_model, platform_operation_events,
    platform_prefetch_model, platform_put_deployment, platform_scale_deployment,
    platform_stop_model, platform_update_pool, platform_whoami,
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

    let http = proxy_http_client().unwrap_or_else(|e| {
        tracing::error!(error=%e, "failed to build reqwest client");
        std::process::exit(1);
    });
    // Two proxies with the same timeouts, on purpose: `http` serves the Gateway → Router
    // hop and control-plane calls, while `upstream` owns the engine data plane and is the
    // one the xlite swap replaces. Sharing one pool would tie an A/B to hop traffic.
    let upstream = nebula_common::upstream_transport_from_env().unwrap_or_else(|e| {
        tracing::error!(error=%e, "failed to build the upstream transport");
        std::process::exit(1);
    });
    tracing::info!(transport = upstream.name(), "upstream transport");

    let store = match nebula_meta::EtcdMetaStore::connect(&args.common.etcd_endpoints()).await {
        Ok(store) => store,
        Err(e) => {
            tracing::error!(error=%e, "failed to connect to etcd");
            return;
        }
    };

    // `NEBULA_ROUTER_MODE=embedded` runs routing in-process (no gateway→router
    // HTTP hop). Default `remote` keeps forwarding to a standalone router.
    let embedded_router = if std::env::var("NEBULA_ROUTER_MODE")
        .map(|v| v.eq_ignore_ascii_case("embedded"))
        .unwrap_or(false)
    {
        let strategy_name =
            std::env::var("NEBULA_ROUTER_STRATEGY").unwrap_or_else(|_| "least_pending".to_string());
        let strategy =
            nebula_router::strategy::parse_strategy(&strategy_name).unwrap_or_else(|e| {
                tracing::error!(error=%e, "invalid routing strategy");
                std::process::exit(1);
            });
        let router = nebula_router::Router::with_strategy(strategy);
        let (r1, s1) = (router.clone(), store.clone());
        tokio::spawn(async move {
            if let Err(e) = nebula_router::sync::endpoints_sync_loop(s1, r1).await {
                tracing::error!(error=%e, "endpoints sync loop exited");
            }
        });
        let (r2, s2) = (router.clone(), store.clone());
        tokio::spawn(async move {
            if let Err(e) = nebula_router::sync::placement_sync_loop(s2, r2).await {
                tracing::error!(error=%e, "placement sync loop exited");
            }
        });
        let (r3, s3) = (router.clone(), store.clone());
        tokio::spawn(async move {
            if let Err(e) = nebula_router::sync::models_sync_loop(s3, r3).await {
                tracing::error!(error=%e, "model specs sync loop exited");
            }
        });
        let (r4, s4) = (router.clone(), store.clone());
        tokio::spawn(async move {
            if let Err(e) = nebula_router::sync::stats_sync_loop(s4, r4).await {
                tracing::error!(error=%e, "stats sync loop exited");
            }
        });
        // Same KV-event runtime as the standalone router (ingest / sink / TTL),
        // so prefix affinity is functionally equivalent in embedded mode
        // (assessment P0-2: this used to be missing — index stayed empty).
        nebula_router::kv_events::start_kv_runtime(router.prefix_index().clone());
        tracing::info!(strategy = %strategy_name, "gateway running with embedded router (no router hop)");
        Some(router)
    } else {
        None
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
    // Embedded-mode retry policy; env names match nebula-router for parity.
    let retry_max = std::env::var("NEBULA_ROUTER_RETRY_MAX")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1);
    let retry_backoff_ms = std::env::var("NEBULA_ROUTER_RETRY_BACKOFF_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(75);
    let stage_timing = std::env::var("NEBULA_GATEWAY_STAGE_TIMING")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    // Cluster quota scope (issue #68). `NEBULA_GATEWAY_QUOTA_MEMBERS=N` pins the
    // divisor; otherwise the Gateway registers in `/gateways/` and shards against
    // the live replica count (join/leave re-shards, a failed replica's share goes
    // to the survivors).
    let quota_ttl = Duration::from_millis(env_num("NEBULA_GATEWAY_MEMBER_TTL_MS", 15_000));
    let quota_refresh = Duration::from_millis(env_num("NEBULA_GATEWAY_MEMBER_REFRESH_MS", 2_000));
    let view_stale = Duration::from_millis(env_num(
        "NEBULA_GATEWAY_QUOTA_VIEW_STALE_MS",
        quota_refresh.as_millis() as u64 * 3,
    ));
    let degrade = match std::env::var("NEBULA_GATEWAY_QUOTA_DEGRADED")
        .unwrap_or_else(|_| "fail_closed".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "fail_open" => nebula_common::admission::DegradePolicy::FailOpen,
        _ => nebula_common::admission::DegradePolicy::FailClosed,
    };
    let pinned_members = nebula_common::quota_shard::members_env_override();
    let store = Arc::new(store);
    let instance_id = std::env::var("NEBULA_GATEWAY_INSTANCE_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| format!("{}-{}", local_hostname(), args.listen_addr));
    let (tenant_admission, member_registry) = match pinned_members {
        Some(n) => {
            tracing::info!(
                quota_members = n,
                "tenant quotas sharded across a pinned member count"
            );
            (nebula_common::TenantAdmission::with_members(n), None)
        }
        None => {
            let view = Arc::new(nebula_common::quota_shard::MemberView::new(view_stale));
            let registry = Arc::new(quota_members::MemberRegistry::new(
                store.clone(),
                instance_id.clone(),
                quota_refresh,
                view.clone(),
            ));
            let joined = registry.join_and_settle(quota_ttl).await;
            match &joined {
                Ok(n) => tracing::info!(
                    instance_id = %instance_id,
                    members = n,
                    ttl_ms = quota_ttl.as_millis() as u64,
                    refresh_ms = quota_refresh.as_millis() as u64,
                    "gateway registered for cluster quota sharding"
                ),
                Err(e) => tracing::warn!(
                    error = %e,
                    instance_id = %instance_id,
                    "gateway registry join failed; tenant quota reads will degrade per policy"
                ),
            }
            let admission = nebula_common::TenantAdmission::default()
                .with_live_members(view.clone(), instance_id.clone())
                .with_degrade_policy(degrade);
            registry.clone().spawn_refresher();
            let registry_for_shutdown = registry.clone();
            tokio::spawn(async move {
                shutdown_signal().await;
                // Deregister first so survivors reclaim the shard immediately,
                // then exit: SIGTERM must still terminate the process (before
                // this, installing the handler kept the Gateway alive).
                registry_for_shutdown.leave().await;
                tracing::info!("gateway deregistered; exiting on shutdown signal");
                std::process::exit(0);
            });
            (admission, Some(registry))
        }
    };
    // Engine-level bounded fair queue (off unless configured).
    let queues = if std::env::var("NEBULA_GATEWAY_QUEUE_MODE").is_ok()
        || std::env::var("NEBULA_GATEWAY_QUEUE_MAX_CONCURRENCY").is_ok()
    {
        let mode = std::env::var("NEBULA_GATEWAY_QUEUE_MODE")
            .ok()
            .and_then(|v| nebula_common::queue::QueueMode::parse(&v))
            .unwrap_or(nebula_common::queue::QueueMode::Fair);
        let num = |k: &str, d: u64| {
            std::env::var(k)
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(d)
        };
        let cfg = nebula_common::queue::QueueConfig {
            mode,
            max_concurrency: num("NEBULA_GATEWAY_QUEUE_MAX_CONCURRENCY", 32).max(1) as usize,
            max_size: num("NEBULA_GATEWAY_QUEUE_MAX_SIZE", 500) as usize,
            max_wait: std::time::Duration::from_millis(num(
                "NEBULA_GATEWAY_QUEUE_MAX_WAIT_MS",
                2000,
            )),
            tenant_share: std::env::var("NEBULA_GATEWAY_QUEUE_TENANT_SHARE")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.5),
        };
        tracing::info!(
            mode = %cfg.mode.as_str(),
            max_concurrency = cfg.max_concurrency,
            max_size = cfg.max_size,
            max_wait_ms = cfg.max_wait.as_millis() as u64,
            "gateway engine queue admission enabled"
        );
        Some(nebula_common::queue::EngineQueues::new(cfg))
    } else {
        None
    };

    let audit = AuditWriter::spawn(
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
    );

    let st = AppState {
        _noop: Arc::new(()),
        router_base_url,
        router: embedded_router,
        retry_max,
        retry_backoff_ms,
        stage_timing,
        queues,
        http,
        upstream,
        store,
        auth,
        metrics,
        dual_write,
        max_request_body_bytes,
        log_path: args.log_path,
        audit,
        xtrace_url: args.common.xtrace_url.clone(),
        xtrace_token: args.common.xtrace_token.clone(),
        bff_url: args.bff_url,
        tenant_admission,
        member_registry,
    };

    // Engine queue capacity is a cluster budget too: keep it divided by the live
    // member count, so a join/leave re-scales it without a restart (issue #68).
    if let Some(queues) = st.queues.clone() {
        let initial = st
            .tenant_admission
            .live_member_count()
            .or(pinned_members)
            .unwrap_or(1);
        queues.set_members(initial);
        tracing::info!(
            members = initial,
            cap_per_replica = st
                .queues
                .as_ref()
                .map(|q| q.config().max_concurrency)
                .unwrap_or(0),
            "engine queue capacity divided across Gateway members"
        );
        let admission = st.tenant_admission.clone();
        let refresh = quota_refresh;
        tokio::spawn(async move {
            let mut last = initial;
            loop {
                if let Some(n) = admission.live_member_count() {
                    if n != last {
                        queues.set_members(n);
                        tracing::info!(members = n, "engine queue divisor updated");
                        last = n;
                    }
                }
                tokio::time::sleep(refresh).await;
            }
        });
    }

    let platform_routes = Router::new()
        .route(
            "/models",
            get(platform_list_models).post(platform_create_model),
        )
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
        .route("/models/:model_uid/prefetch", post(platform_prefetch_model))
        .route("/models/:model_uid/evict", post(platform_evict_model))
        .route("/models/:model_uid/replicas", get(platform_list_replicas))
        .route("/nodes", get(platform_list_nodes))
        .route("/operations/:operation_id", get(platform_get_operation))
        .route(
            "/operations/:operation_id/events",
            get(platform_operation_events),
        )
        .route("/health/summary", get(platform_health_summary))
        .route("/cluster/status", get(platform_cluster_status))
        .route("/whoami", get(platform_whoami))
        .route("/replicas/drain", post(platform_drain_replica))
        .route("/nodes/:node_id/drain", post(platform_drain_node))
        .route(
            "/pools",
            get(platform_list_pools).post(platform_create_pool),
        )
        .route(
            "/pools/:pool_id",
            get(platform_get_pool)
                .put(platform_update_pool)
                .delete(platform_delete_pool),
        )
        .route("/audit-logs", get(platform_audit_logs))
        .route(
            "/models/:model_uid/slo/evaluation",
            get(platform_evaluate_slo),
        )
        .route("/models/:model_uid/slo", get(platform_get_slo))
        .route("/canaries", get(platform_list_canaries))
        .route("/canaries/:canary_id", get(platform_get_canary))
        .route(
            "/webhooks",
            get(platform_list_webhooks).post(platform_create_webhook),
        )
        .route(
            "/webhooks/:webhook_id",
            axum::routing::delete(platform_delete_webhook),
        )
        .with_state(st.clone());

    let secure_routes = Router::new()
        .route("/v1/responses", post(create_responses))
        .route("/v1/messages", post(create_anthropic_messages))
        .route("/v1/chat/completions", post(proxy_post))
        .route("/v1/completions", post(proxy_post))
        .route("/v1/embeddings", post(proxy_post))
        .route("/v1/rerank", post(proxy_post))
        .route("/v1/score", post(proxy_post))
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

fn env_num(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// Registry id suffix. Unique per host so two Gateways on one host (different
/// ports) and Gateways on different hosts never collide.
fn local_hostname() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|h| !h.trim().is_empty())
        .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok())
        .map(|h| h.trim().trim_end_matches(['.']).to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "gateway".to_string())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
