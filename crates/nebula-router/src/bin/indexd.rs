//! P1c Batch 2: standalone KV-event index service.
//!
//! Engines publish to ONE stable endpoint (this service); it maintains the
//! authoritative prefix index and pushes to every router sink
//! (Pub-connect→Sub-bind, the zeromq-0.4-safe pair):
//!   - raw frames, verbatim, for incremental freshness
//!   - periodic full snapshots (`!reset/{model}` + `!snapshot/{model}/{replica}`)
//!     so restarted routers rebuild without waiting for reuse events
//!
//! Routers keep a local replica of the index — the request hot path never
//! queries this service. If it dies, routers degrade to their last-known
//! index (visible via the fallback counters) and self-heal once it returns.
//!
//! Env:
//!   NEBULA_INDEXD_BIND          engines' zmq SUB endpoint (required)
//!   NEBULA_INDEXD_SINKS         comma-separated router sink addrs (required)
//!   NEBULA_INDEXD_SNAPSHOT_SECS snapshot round interval (default 60)
//!   NEBULA_ROUTER_PREFIX_TTL_MS index TTL (default 300_000, shared with router)

use std::time::Duration;

use nebula_router::kv_events::{encode_stored_batch, kv_events_loop, PrefixIndex};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let bind = std::env::var("NEBULA_INDEXD_BIND")
        .map_err(|_| anyhow::anyhow!("NEBULA_INDEXD_BIND is required (engines' zmq endpoint)"))?;
    let sinks: Vec<String> = std::env::var("NEBULA_INDEXD_SINKS")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if sinks.is_empty() {
        anyhow::bail!("NEBULA_INDEXD_SINKS must list at least one router sink");
    }
    let snapshot_secs = std::env::var("NEBULA_INDEXD_SNAPSHOT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    let ttl_ms = std::env::var("NEBULA_ROUTER_PREFIX_TTL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300_000);

    let index = PrefixIndex::new();
    tracing::info!(
        bind = %bind,
        sinks = ?sinks,
        snapshot_secs,
        ttl_ms,
        "nebula-indexd starting"
    );

    // Ingest from engines + forward raw frames to sinks (reuses the router's
    // proven loop: apply locally and push verbatim).
    {
        let index = index.clone();
        let sinks = sinks.clone();
        tokio::spawn(async move {
            if let Err(e) = kv_events_loop(Some(bind), sinks, index).await {
                tracing::error!(error = %e, "indexd ingest loop exited");
                std::process::exit(1);
            }
        });
    }

    // TTL sweep (authoritative expiry; routers sweep their own copies).
    {
        let index = index.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let evicted = index.evict_expired(now, ttl_ms);
                if evicted > 0 {
                    tracing::debug!(evicted, "indexd ttl sweep");
                }
            }
        });
    }

    // Periodic snapshot push: for every model, !reset then one
    // !snapshot/{model}/{replica} batch per replica.
    {
        let index = index.clone();
        tokio::spawn(async move {
            use nebula_router::kv_events::connect_pusher;
            use zeromq::SocketSend;
            loop {
                // Fresh pusher every round: zeromq PUB sockets never
                // reconnect to re-bound peers, and router restarts are
                // exactly what snapshots exist for.
                let mut pusher: zeromq::PubSocket = connect_pusher(&sinks).await;
                tokio::time::sleep(Duration::from_secs(snapshot_secs)).await;
                for model in index.models() {
                    let _ = pusher
                        .send(control_msg(format!("!reset/{model}"), vec![]))
                        .await;
                    for (replica, keys) in index.snapshot_for(&model) {
                        let payload = encode_stored_batch(&keys);
                        let _ = pusher
                            .send(control_msg(format!("!snapshot/{model}/{replica}"), payload))
                            .await;
                    }
                    tracing::debug!(model = %model, "snapshot pushed");
                }
            }
        });
    }

    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn control_msg(topic: String, payload: Vec<u8>) -> zeromq::ZmqMessage {
    let mut msg = zeromq::ZmqMessage::from(topic);
    msg.push_back(bytes::Bytes::from(vec![0u8; 8])); // seq placeholder
    msg.push_back(bytes::Bytes::from(payload));
    msg
}
