//! Nebula observability launcher — thin wrapper around the xtrace server.
//!
//! Prefer this binary (or the `xtrace` image) over ad-hoc process flags so
//! `DATABASE_URL` / `OBSERVE_TOKEN` / bind address stay consistent with BFF
//! (`OBSERVE_URL`, `OBSERVE_TOKEN`, `OBSERVE_AUTH_MODE`).
//!
//! Production compose may run the upstream image directly (`docker-compose`
//! profile `observe`). This binary is for local/dev and custom packaging.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "nebula-observe",
    about = "Nebula observability service (xtrace wrapper)"
)]
struct Args {
    /// PostgreSQL connection URL (observe DB — not BFF session DB)
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Bearer token for API authentication (same value as BFF OBSERVE_TOKEN)
    #[arg(long, env = "OBSERVE_TOKEN", default_value = "")]
    token: String,

    /// Bind address for the HTTP server
    #[arg(long, env = "OBSERVE_BIND_ADDR", default_value = "0.0.0.0:8742")]
    bind_addr: String,

    /// Default project ID for metrics and traces
    #[arg(long, env = "OBSERVE_PROJECT_ID", default_value = "nebula")]
    project_id: String,

    /// Log format: text | json
    #[arg(long, env = "LOG_FORMAT", default_value = "text")]
    log_format: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Align with other Nebula components (stdout + optional OTLP later).
    let _otel_guard = nebula_common::telemetry::init_tracing(
        "nebula-observe",
        None,
        None,
        &args.log_format,
    );

    if args.token.is_empty() {
        tracing::warn!(
            "OBSERVE_TOKEN is empty; xtrace API will accept unauthenticated requests \
             (dev only). Set OBSERVE_TOKEN for production."
        );
    }

    tracing::info!(
        bind_addr = %args.bind_addr,
        project_id = %args.project_id,
        "starting nebula-observe (xtrace)"
    );

    xtrace::run_server(xtrace::ServerConfig {
        database_url: args.database_url,
        api_bearer_token: args.token,
        bind_addr: args.bind_addr,
        default_project_id: args.project_id,
        langfuse_public_key: None,
        langfuse_secret_key: None,
    })
    .await
}
