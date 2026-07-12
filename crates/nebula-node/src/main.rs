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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use futures_util::StreamExt;
use nebula_common::PlacementPlan;
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::args::Args;
use crate::heartbeat::heartbeat_loop;
use crate::reconcile::{has_local_replica, local_assignments, reconcile_model, ReplicaKey, RunningModel};

/// Interval for periodic full reconcile (drain progression + watch gap fill).
const PERIODIC_RECONCILE_INTERVAL: Duration = Duration::from_secs(3);

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

async fn periodic_full_reconcile_loop(
    store: EtcdMetaStore,
    args: Args,
    running: Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    endpoint_state: Arc<Mutex<HashMap<ReplicaKey, nebula_common::EndpointInfo>>>,
    last_epochs: Arc<Mutex<HashMap<String, u64>>>,
) {
    let prefix = "/placements/";
    loop {
        tokio::time::sleep(PERIODIC_RECONCILE_INTERVAL).await;

        let plans = match store.list_prefix(prefix).await {
            Ok(kvs) => kvs
                .into_iter()
                .filter_map(|(_, val, _)| serde_json::from_slice::<PlacementPlan>(&val).ok())
                .collect::<Vec<_>>(),
            Err(e) => {
                tracing::warn!(error=%e, "periodic reconcile: failed to list placements");
                continue;
            }
        };

        let mut seen_models: HashSet<String> = HashSet::new();
        for plan in plans {
            seen_models.insert(plan.model_uid.clone());
            let mid = plan.model_uid.clone();
            let local_desired = !local_assignments(&plan, &args.node_id).is_empty();
            let local_running = has_local_replica(&*running.lock().await, &mid);
            if !local_desired && !local_running {
                continue;
            }
            if let Err(e) = reconcile_model(
                &store,
                &args,
                &running,
                &endpoint_state,
                &last_epochs,
                &mid,
                Some(plan),
            )
            .await
            {
                tracing::warn!(model=%mid, error=%e, "periodic reconcile failed");
            }
        }

        // Orphans: local running models with no placement key left.
        let orphan_uids: Vec<String> = {
            let guard = running.lock().await;
            let mut uids: HashSet<String> = guard.keys().map(|(uid, _)| uid.clone()).collect();
            uids.retain(|uid| !seen_models.contains(uid));
            uids.into_iter().collect()
        };
        for mid in orphan_uids {
            tracing::info!(model=%mid, "periodic reconcile: draining orphan after placement delete");
            if let Err(e) = reconcile_model(
                &store,
                &args,
                &running,
                &endpoint_state,
                &last_epochs,
                &mid,
                None,
            )
            .await
            {
                tracing::warn!(model=%mid, error=%e, "periodic orphan drain failed");
            }
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

    let store = EtcdMetaStore::connect(&args.common.etcd_endpoints()).await?;

    // C3: one shared lease for node status + endpoints, refreshed by keepalive.
    let lease_ttl_secs = ((args.heartbeat_ttl_ms as f64 / 1000.0).ceil() as i64).max(10);
    let lease_id = match store.grant_lease(lease_ttl_secs).await {
        Ok(id) => {
            tracing::info!(lease_id=id, ttl_secs=lease_ttl_secs, "granted etcd lease for status/endpoints");
            store.spawn_lease_keepalive(id, lease_ttl_secs);
            Some(id)
        }
        Err(e) => {
            tracing::warn!(error=%e, "failed to grant etcd lease; falling back to per-put TTL");
            None
        }
    };

    let endpoint_state: Arc<Mutex<HashMap<ReplicaKey, nebula_common::EndpointInfo>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // shared running state (used by reconcile, heartbeat, and periodic drain)
    let running: Arc<Mutex<HashMap<ReplicaKey, RunningModel>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let last_epochs: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));

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
        lease_id,
    ));

    // A2: advance Drain and fill watch gaps even when no placement events arrive.
    tokio::spawn(periodic_full_reconcile_loop(
        store.clone(),
        args.clone(),
        running.clone(),
        endpoint_state.clone(),
        last_epochs.clone(),
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
                    let assigned = !local_assignments(&plan, &args.node_id).is_empty();
                    if assigned {
                        tracing::info!(model=%plan.model_uid, "found existing assignment");
                        let mid = plan.model_uid.clone();
                        let _ = reconcile_model(
                            &store,
                            &args,
                            &running,
                            &endpoint_state,
                            &last_epochs,
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
                        &running,
                        &endpoint_state,
                        &last_epochs,
                        &mid,
                        Some(p),
                    )
                    .await;
                }
                None => {
                    let key = ev.key;
                    let model_uid = key.strip_prefix(prefix).unwrap_or(&key);

                    if has_local_replica(&*running.lock().await, model_uid) {
                        tracing::info!(model=%model_uid, "placement deleted event affecting local node");
                        let model_uid = model_uid.to_string();
                        let _ = reconcile_model(
                            &store,
                            &args,
                            &running,
                            &endpoint_state,
                            &last_epochs,
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
