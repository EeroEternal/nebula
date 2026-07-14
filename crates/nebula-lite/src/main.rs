mod args;
mod engine;
mod proxy;

use std::net::SocketAddr;

use anyhow::Context;
use clap::Parser;

use args::Args;
use engine::start_engine;
use proxy::{router, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = nebula_common::telemetry::init_tracing("nebula-lite", None, None, "text");

    let args = Args::parse();
    // Validate GPU list early for clear errors.
    let _ = args.gpu_indices()?;

    let handle = start_engine(&args).await?;
    let engine_base_url = handle.base_url.clone();

    let http = nebula_common::proxy_http_client().context("build HTTP client")?;
    let state = AppState {
        http,
        engine_base_url: engine_base_url.clone(),
    };

    let listen: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("invalid listen address {}:{}", args.host, args.port))?;
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("bind {listen}"))?;

    tracing::info!(
        listen=%listen,
        engine=%engine_base_url,
        model=%args.model,
        "nebula-lite ready"
    );
    eprintln!("nebula-lite listening on http://{listen}");
    eprintln!("  model:  {}", args.model);
    eprintln!("  engine: {:?}", args.engine);
    eprintln!("  upstream: {engine_base_url}");

    let app = router(state);
    let server = axum::serve(listener, app);

    tokio::select! {
        r = server => {
            if let Err(e) = r {
                tracing::error!(error=%e, "HTTP server error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received Ctrl+C, shutting down");
        }
    }

    handle.shutdown().await;
    Ok(())
}
