mod args;
mod planner;
mod reconcile;
mod util;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use clap::Parser;
use futures_util::StreamExt;
use tracing::{error, info, warn};

use nebula_common::{
    DesiredState, ModelDeployment, ModelSpec, PlacementPlan,
};
use nebula_meta::{
    run_election_loop, ElectionConfig, EtcdMetaStore, LeaderGate, MetaStore,
};
use nebula_scheduler::metrics::{
    healthz_handler, metrics_handler, AppState, SharedMetrics,
};

use crate::args::Args;
use crate::planner::{build_plan_from_deployment, list_used_resources};

async fn write_placement_cas(
    store: &EtcdMetaStore,
    placement_key: &str,
    plan: &mut PlacementPlan,
    leader: &LeaderGate,
) -> Result<u64> {
    if !leader.is_leader() {
        anyhow::bail!("not leader; refusing placement write for {placement_key}");
    }
    plan.leader_epoch = leader.epoch();

    let existing = store.get(placement_key).await?;
    let expected_revision = existing.as_ref().map(|(_, revision)| *revision).unwrap_or(0);
    let prev_version = existing
        .as_ref()
        .and_then(|(data, _)| serde_json::from_slice::<PlacementPlan>(data).ok())
        .map(|p| p.version)
        .unwrap_or(0);
    // Logical monotonic version (never wall-clock).
    plan.version = nebula_common::next_placement_version(prev_version);
    plan.updated_at_ms = crate::util::now_ms();

    let placement_val = serde_json::to_vec(plan)?;
    let (ok, revision) = store
        .compare_and_swap(placement_key, expected_revision, placement_val)
        .await?;

    if ok {
        Ok(revision)
    } else {
        anyhow::bail!(
            "placement CAS failed for {}: expected revision {}, current revision {}",
            placement_key,
            expected_revision,
            revision
        );
    }
}

async fn delete_placement_if_leader(
    store: &EtcdMetaStore,
    placement_key: &str,
    leader: &LeaderGate,
) -> Result<()> {
    if !leader.is_leader() {
        anyhow::bail!("not leader; refusing placement delete for {placement_key}");
    }
    store.delete(placement_key).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let _otel_guard = nebula_common::telemetry::init_tracing(
        "nebula-scheduler",
        args.common.xtrace_url.as_deref(),
        args.common.xtrace_token.as_deref(),
        &args.common.log_format,
    );
    info!("nebula-scheduler starting...");

    let store = EtcdMetaStore::connect(std::slice::from_ref(&args.common.etcd_endpoint)).await?;
    info!("connected to etcd at {}", args.common.etcd_endpoint);

    let leader = LeaderGate::new();
    let holder_id = format!("scheduler-{}", uuid::Uuid::new_v4());
    let election_cfg = ElectionConfig::scheduler(holder_id);
    let store_for_election = store.clone();
    let leader_for_election = leader.clone();
    tokio::spawn(async move {
        run_election_loop(&store_for_election, leader_for_election, election_cfg).await;
    });

    let shared_metrics = Arc::new(SharedMetrics::default());
    let app_state = AppState {
        metrics: Arc::clone(&shared_metrics),
        leader: leader.clone(),
    };

    let listen_addr = args.listen_addr.clone();
    tokio::spawn(async move {
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/healthz", get(healthz_handler))
            .with_state(app_state);

        let listener = match tokio::net::TcpListener::bind(&listen_addr).await {
            Ok(l) => l,
            Err(e) => {
                error!("failed to bind metrics server on {}: {}", listen_addr, e);
                return;
            }
        };
        info!("metrics server listening on {}", listen_addr);
        if let Err(e) = axum::serve(listener, app).await {
            error!("metrics server error: {}", e);
        }
    });

    let store_for_reconcile = store.clone();
    let default_port_for_reconcile = args.default_port;
    let metrics_for_reconcile = Arc::clone(&shared_metrics);
    let leader_for_reconcile = leader.clone();
    tokio::spawn(async move {
        reconcile::reconcile_loop(
            store_for_reconcile,
            default_port_for_reconcile,
            metrics_for_reconcile,
            leader_for_reconcile,
        )
        .await;
    });

    // Declarative single path: only watch /deployments/ (B5).
    // Legacy /model_requests/ watch removed; migrate with `nebula admin migrate`.
    info!("scheduler control plane: deployments-only watch");
    deployment_watch_loop(store, args.default_port, leader).await;
    Ok(())
}

/// Watch `/deployments/` prefix for the new declarative model management flow.
async fn deployment_watch_loop(store: EtcdMetaStore, default_port: u16, leader: LeaderGate) {
    let prefix = "/deployments/";
    loop {
        info!("watching prefix: {}", prefix);
        let mut stream = match store.watch_prefix(prefix, None).await {
            Ok(s) => s,
            Err(e) => {
                error!("failed to watch deployments: {}, retrying in 5s", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        while let Some(event) = stream.next().await {
            if !leader.is_leader() {
                continue;
            }

            match event.value {
                Some(value) => {
                    let deployment: ModelDeployment = match serde_json::from_slice(&value) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!("failed to deserialize deployment: {}", e);
                            continue;
                        }
                    };

                    if deployment.desired_state == DesiredState::Running {
                        info!(
                            model_uid=%deployment.model_uid,
                            replicas=deployment.replicas,
                            "deployment running: building placement plan"
                        );

                        let spec_key = format!("/models/{}/spec", deployment.model_uid);
                        let spec: ModelSpec = match store.get(&spec_key).await {
                            Ok(Some((val, _))) => match serde_json::from_slice(&val) {
                                Ok(s) => s,
                                Err(e) => {
                                    warn!(
                                        model_uid=%deployment.model_uid,
                                        error=%e,
                                        "failed to deserialize model spec, skipping"
                                    );
                                    continue;
                                }
                            },
                            Ok(None) => {
                                warn!(
                                    model_uid=%deployment.model_uid,
                                    "model spec not found at {}, skipping",
                                    spec_key
                                );
                                continue;
                            }
                            Err(e) => {
                                error!(
                                    model_uid=%deployment.model_uid,
                                    error=%e,
                                    "failed to read model spec"
                                );
                                continue;
                            }
                        };

                        let (used_ports, used_gpus) = match list_used_resources(&store).await {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("failed to list placements: {}", e);
                                (
                                    std::collections::HashSet::new(),
                                    std::collections::HashMap::new(),
                                )
                            }
                        };

                        let mut plan = match build_plan_from_deployment(
                            &store,
                            &spec,
                            &deployment,
                            default_port,
                            used_ports,
                            used_gpus,
                        )
                        .await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                error!(
                                    model_uid=%deployment.model_uid,
                                    error=%e,
                                    "failed to build placement plan from deployment"
                                );
                                continue;
                            }
                        };

                        let placement_key = format!("/placements/{}", plan.model_uid);
                        if let Err(e) =
                            write_placement_cas(&store, &placement_key, &mut plan, &leader).await
                        {
                            error!(
                                model_uid=%deployment.model_uid,
                                error=%e,
                                "failed to write placement"
                            );
                        } else {
                            info!(
                                model_uid=%deployment.model_uid,
                                epoch=plan.leader_epoch,
                                "wrote placement to {} (CAS)",
                                placement_key
                            );
                        }
                    } else if deployment.desired_state == DesiredState::Stopped {
                        info!(
                            model_uid=%deployment.model_uid,
                            "deployment stopped: deleting placement"
                        );
                        let placement_key = format!("/placements/{}", deployment.model_uid);
                        if let Err(e) =
                            delete_placement_if_leader(&store, &placement_key, &leader).await
                        {
                            warn!(
                                model_uid=%deployment.model_uid,
                                error=%e,
                                "failed to delete placement {}",
                                placement_key
                            );
                        } else {
                            info!(
                                model_uid=%deployment.model_uid,
                                "deleted placement {}",
                                placement_key
                            );
                        }
                    }
                }
                None => {
                    let model_uid = event.key.strip_prefix(prefix).unwrap_or(&event.key);
                    if model_uid.is_empty() {
                        continue;
                    }
                    info!(
                        model_uid=%model_uid,
                        "deployment deleted: deleting placement"
                    );
                    let placement_key = format!("/placements/{}", model_uid);
                    if let Err(e) =
                        delete_placement_if_leader(&store, &placement_key, &leader).await
                    {
                        warn!(
                            model_uid=%model_uid,
                            error=%e,
                            "failed to delete placement {}",
                            placement_key
                        );
                    } else {
                        info!(
                            model_uid=%model_uid,
                            "deleted placement {}",
                            placement_key
                        );
                    }
                }
            }
        }

        warn!("deployment watch stream ended, reconnecting...");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
