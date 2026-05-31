mod args;
mod docker_api;
mod engine;
mod gpu;
mod heartbeat;
mod image_manager;
mod model_cache_manager;
mod reconcile;
mod util;

use clap::Parser;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use futures_util::StreamExt;
use nebula_common::PlacementPlan;
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::args::Args;
use crate::heartbeat::heartbeat_loop;
use crate::reconcile::{reconcile_model, RunningModel};

fn init_xtrace_client(args: &Args) -> Option<xtrace_client::Client> {
    let url = args.common.xtrace_url.as_deref()?;
    let token = args.common.xtrace_token.as_deref().unwrap_or("");
    match xtrace_client::Client::new(url, token) {
        Ok(c) => {
            tracing::info!(%url, "xtrace metrics reporting enabled");
            Some(c)
        }
        Err(e) => {
            tracing::warn!(error=%e, "failed to create xtrace client, metrics reporting disabled");
            None
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let _otel_guard = nebula_common::telemetry::init_tracing(
        "nebula-node",
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
        &args.common.log_format,
    );
    println!(
        "DEBUG: nebula-node process started! node_id={}",
        args.node_id
    );
    tracing::info!(node_id=%args.node_id, "nebula-node starting...");

    let store = EtcdMetaStore::connect(std::slice::from_ref(&args.common.etcd_endpoint)).await?;

    let endpoint_state: Arc<Mutex<HashMap<String, nebula_common::EndpointInfo>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // shared running state (used by both main reconcile loop and heartbeat)
    let running: Arc<Mutex<HashMap<String, RunningModel>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let xtrace = init_xtrace_client(&args);

    // Shared metrics state for Prometheus /metrics endpoint
    let shared_metrics: docker_api::SharedNodeMetrics =
        Arc::new(Mutex::new(docker_api::NodeMetricsSnapshot::default()));

    tokio::spawn(heartbeat_loop(
        store.clone(),
        args.node_id.clone(),
        args.heartbeat_ttl_ms,
        args.heartbeat_interval_ms,
        args.api_port,
        endpoint_state.clone(),
        running.clone(),
        xtrace,
        shared_metrics.clone(),
    ));

    // Start image manager: watches /images/ registry, pre-pulls and GC
    tokio::spawn(image_manager::image_manager_loop(
        store.clone(),
        args.node_id.clone(),
    ));

    // Start model cache scanner: periodically scans model_dir and reports to etcd
    tokio::spawn(model_cache_manager::model_cache_scan_loop(
        store.clone(),
        args.node_id.clone(),
        args.vllm_model_dir.clone(),
    ));

    // Start Node HTTP API server
    let api_addr = format!("0.0.0.0:{}", args.api_port);
    let api_router = docker_api::node_api_router(shared_metrics);
    let listener = tokio::net::TcpListener::bind(&api_addr).await?;
    tracing::info!(%api_addr, "node API server listening");
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, api_router).await {
            tracing::error!(error=%e, "node API server error");
        }
    });

    // 1. List existing placements to find if any are assigned to us
    let prefix = "/placements/";
    let mut start_rev = 0;

    if let Ok(kvs) = store.list_prefix(prefix).await {
        for (_key, val, rev) in kvs {
            if rev > start_rev {
                start_rev = rev;
            }

            match serde_json::from_slice::<PlacementPlan>(&val) {
                Ok(plan) => {
                    let assigned = plan.assignments.iter().any(|a| a.node_id == args.node_id);
                    if assigned {
                        tracing::info!(model=%plan.model_uid, "found existing assignment");
                        let mid = plan.model_uid.clone();
                        let _ = reconcile_model(
                            &store,
                            &args,
                            &mut *running.lock().await,
                            &endpoint_state,
                            &mid,
                            Some(plan),
                        )
                        .await;
                    }
                }
                Err(e) => {
                    tracing::error!(key=%_key, error=%e, "failed to deserialize placement plan");
                }
            }
        }
    }

    loop {
        tracing::info!("watching placements from rev {}", start_rev);
        let mut watch = match store.watch_prefix(prefix, Some(start_rev)).await {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch placements, will retry");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        while let Some(ev) = watch.next().await {
            if ev.revision > start_rev {
                start_rev = ev.revision;
            }

            let plan: Option<PlacementPlan> =
                ev.value.and_then(|val| serde_json::from_slice(&val).ok());

            match plan {
                Some(p) => {
                    let mid = p.model_uid.clone();
                    let _ = reconcile_model(
                        &store,
                        &args,
                        &mut *running.lock().await,
                        &endpoint_state,
                        &mid,
                        Some(p),
                    )
                    .await;
                }
                None => {
                    let key = ev.key;
                    let model_uid = key.strip_prefix(prefix).unwrap_or(&key);
                    
                    if running.lock().await.contains_key(model_uid) {
                        tracing::info!(model=%model_uid, "placement deleted event affecting local node");
                        let model_uid = model_uid.to_string();
                        let _ = reconcile_model(
                            &store,
                            &args,
                            &mut *running.lock().await,
                            &endpoint_state,
                            &model_uid,
                            None,
                        )
                        .await;
                    } else {
                        tracing::debug!(model=%model_uid, "ignoring placement deletion for non-local model");
                    }
                }
            }
        }

        tracing::warn!("watch stream ended, reconnecting");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
