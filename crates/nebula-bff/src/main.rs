mod args;
mod auth;
mod auth_handlers;
mod benchmark_svc;
mod compat_slo;
mod handlers;
mod handlers_v2;
mod selection_svc;
mod service;
mod state;
mod tenant_svc;

use std::sync::Arc;

use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use clap::Parser;
use nebula_common::control_plane_http_client;

use crate::args::Args;
use crate::auth::{db_auth_middleware, initialize_auth_schema};
use crate::auth_handlers::{
    create_user, delete_user, get_settings, list_users, login, logout, me, update_profile,
    update_settings, update_user,
};
use crate::handlers::{
    audit_logs, delete_image, engine_stats, get_image, healthz, list_image_status, list_images,
    list_requests, load_model, logs, metrics, observe_metrics_names, observe_metrics_query,
    observe_trace_detail, observe_traces, overview, put_image, search_models, unload_model, whoami,
};
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let _otel_guard = nebula_common::telemetry::init_tracing(
        "nebula-bff",
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
        &args.common.log_format,
    );

    let store =
        nebula_meta::EtcdMetaStore::connect(&args.common.etcd_endpoints()).await?;

    let http = control_plane_http_client().unwrap_or_else(|e| {
        tracing::error!(error=%e, "failed to build reqwest client");
        std::process::exit(1);
    });

    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&args.database_url)
        .await?;

    let st = AppState {
        store: Arc::new(store),
        db,
        http,
        router_url: args.router_url,
        session_ttl_hours: args.session_ttl_hours,
        xtrace_url: args
            .common
            .xtrace_url
            .clone()
            .unwrap_or_else(|| "http://127.0.0.1:8742".to_string()),
        xtrace_token: args.common.xtrace_token.clone().unwrap_or_default(),
        xtrace_auth_mode: args.xtrace_auth_mode,
    };

    initialize_auth_schema(&st).await?;

    let protected_routes = Router::new()
        .route("/whoami", get(whoami))
        .route("/overview", get(overview))
        .route("/requests", get(list_requests))
        .route("/models/load", post(load_model))
        .route("/models/requests/:id", delete(unload_model))
        .route("/metrics", get(metrics))
        .route("/engine-stats", get(engine_stats))
        .route("/logs", get(logs))
        .route("/models/search", get(search_models))
        .route("/observe/traces", get(observe_traces))
        .route("/observe/traces/:traceId", get(observe_trace_detail))
        .route("/observe/metrics/query", get(observe_metrics_query))
        .route("/observe/metrics/names", get(observe_metrics_names))
        .route("/audit-logs", get(audit_logs))
        // Image registry
        .route("/images", get(list_images))
        .route("/images/status", get(list_image_status))
        .route(
            "/images/:id",
            get(get_image).put(put_image).delete(delete_image),
        )
        .layer(middleware::from_fn_with_state(
            st.clone(),
            db_auth_middleware,
        ))
        .with_state(st.clone());

    let v2_routes = Router::new()
        .route(
            "/observability/gateway/overview",
            get(handlers_v2::gateway_overview),
        )
        .route(
            "/observability/gateway/traffic",
            get(handlers_v2::gateway_traffic),
        )
        .route(
            "/observability/gateway/reliability",
            get(handlers_v2::gateway_reliability),
        )
        .route(
            "/observability/gateway/protection",
            get(handlers_v2::gateway_protection),
        )
        .route(
            "/observability/gateway/latency",
            get(handlers_v2::gateway_latency),
        )
        .route(
            "/models",
            get(handlers_v2::list_models).post(handlers_v2::create_model),
        )
        .route(
            "/models/:model_uid",
            get(handlers_v2::get_model)
                .put(handlers_v2::update_model)
                .delete(handlers_v2::delete_model),
        )
        .route("/models/:model_uid/start", post(handlers_v2::start_model))
        .route("/models/:model_uid/stop", post(handlers_v2::stop_model))
        .route("/models/:model_uid/scale", put(handlers_v2::scale_model))
        .route(
            "/models/:model_uid/save-as-template",
            post(handlers_v2::save_as_template),
        )
        .route(
            "/templates",
            get(handlers_v2::list_templates).post(handlers_v2::create_template),
        )
        .route(
            "/templates/:id",
            get(handlers_v2::get_template)
                .put(handlers_v2::update_template)
                .delete(handlers_v2::delete_template),
        )
        .route("/templates/:id/deploy", post(handlers_v2::deploy_template))
        .route("/nodes/:node_id/cache", get(handlers_v2::node_cache))
        .route("/nodes/:node_id/disk", get(handlers_v2::node_disk))
        .route("/cache/summary", get(handlers_v2::cache_summary))
        .route("/alerts", get(handlers_v2::list_alerts))
        .route("/migrate", post(handlers_v2::migrate_v1_to_v2))
        .route(
            "/compat",
            get(handlers_v2::list_compat_rules).put(handlers_v2::put_compat_rule),
        )
        .route("/compat/seed", post(handlers_v2::seed_compat_rules))
        .route("/compat/:id", delete(handlers_v2::delete_compat_rule))
        .route("/inventory/hardware", get(handlers_v2::hardware_inventory))
        .route("/slos", get(handlers_v2::list_slos))
        .route(
            "/slos/:model_uid",
            get(handlers_v2::get_slo)
                .put(handlers_v2::upsert_slo)
                .delete(handlers_v2::delete_slo),
        )
        .route(
            "/slos/:model_uid/evaluate",
            get(handlers_v2::evaluate_slo),
        )
        .route("/diagnostics/events", get(handlers_v2::list_diagnostics))
        .route("/benchmarks/workloads", get(handlers_v2::list_workloads))
        .route(
            "/benchmarks/runs",
            get(handlers_v2::list_benchmark_runs).post(handlers_v2::ingest_benchmark_run),
        )
        .route("/benchmarks/runs/:run_id", get(handlers_v2::get_benchmark_run))
        .route("/benchmarks/profiles", get(handlers_v2::list_benchmark_profiles))
        .route("/benchmarks/recommend", post(handlers_v2::recommend_engines))
        .route(
            "/model-profiles/:profile_id",
            get(handlers_v2::get_model_profile).put(handlers_v2::put_model_profile),
        )
        .route("/selection/recommend", post(handlers_v2::selection_recommend))
        .route("/selection/draft", post(handlers_v2::selection_draft))
        .route("/selection/apply", post(handlers_v2::selection_apply))
        .route(
            "/canaries",
            get(handlers_v2::list_canaries).post(handlers_v2::create_canary),
        )
        .route(
            "/canaries/:canary_id/evaluate",
            post(handlers_v2::evaluate_canary),
        )
        .route(
            "/canaries/:canary_id/promote",
            post(handlers_v2::promote_canary),
        )
        .route(
            "/canaries/:canary_id/rollback",
            post(handlers_v2::rollback_canary),
        )
        .route(
            "/tenants",
            get(handlers_v2::list_tenants).put(handlers_v2::upsert_tenant),
        )
        .route(
            "/tenants/:tenant_id",
            get(handlers_v2::get_tenant).delete(handlers_v2::delete_tenant),
        )
        .route(
            "/tenants/:tenant_id/usage",
            get(handlers_v2::list_tenant_usage).post(handlers_v2::ingest_usage),
        )
        .route(
            "/tenants/:tenant_id/cost",
            get(handlers_v2::tenant_cost_summary),
        )
        .route(
            "/pricing",
            get(handlers_v2::list_pricing).put(handlers_v2::upsert_pricing),
        )
        .route("/pricing/:price_id", delete(handlers_v2::delete_pricing))
        .route("/usage", post(handlers_v2::ingest_usage))
        .layer(middleware::from_fn_with_state(
            st.clone(),
            db_auth_middleware,
        ))
        .with_state(st.clone());

    let auth_public_routes = Router::new()
        .route("/auth/login", post(login))
        .with_state(st.clone());

    let auth_routes = Router::new()
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/profile", put(update_profile))
        .route("/auth/settings", get(get_settings).put(update_settings))
        .route("/auth/users", get(list_users).post(create_user))
        .route("/auth/users/:id", put(update_user).delete(delete_user))
        .layer(middleware::from_fn_with_state(
            st.clone(),
            db_auth_middleware,
        ))
        .with_state(st.clone());

    let api_routes = Router::new()
        .route("/healthz", get(healthz))
        .merge(auth_public_routes)
        .merge(auth_routes)
        .merge(protected_routes);

    let app = Router::new()
        .nest("/api", api_routes)
        .nest("/api/v2", v2_routes);

    let listener = tokio::net::TcpListener::bind(&args.listen_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
