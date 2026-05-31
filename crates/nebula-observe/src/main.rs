use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(
    name = "nebula-observe",
    about = "Nebula observability service (powered by xtrace)"
)]
struct Args {
    /// PostgreSQL connection URL
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    /// Bearer token for API authentication
    #[arg(long, env = "OBSERVE_TOKEN", default_value = "")]
    token: String,

    /// Bind address for the HTTP server
    #[arg(long, default_value = "0.0.0.0:8742")]
    bind_addr: String,

    /// Default project ID for metrics and traces
    #[arg(long, default_value = "nebula")]
    project_id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info,sqlx=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    tracing::info!(
        bind_addr = %args.bind_addr,
        project_id = %args.project_id,
        "starting nebula-observe"
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
