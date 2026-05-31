mod args;
mod metrics;
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

use nebula_common::{DesiredState, ModelDeployment, ModelRequest, ModelRequestStatus, ModelSpec};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::args::Args;
use crate::metrics::{healthz_handler, metrics_handler, SharedMetrics};
use crate::planner::{build_plan_from_deployment, build_plan_multi, list_used_resources};

async fn write_placement_cas(
    store: &EtcdMetaStore,
    placement_key: &str,
    placement_val: Vec<u8>,
) -> Result<u64> {
    let expected_revision = store
        .get(placement_key)
        .await?
        .map(|(_, revision)| revision)
        .unwrap_or(0);

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

    // Shared metrics for Prometheus exposition
    let shared_metrics = Arc::new(SharedMetrics::default());

    // Spawn metrics / health HTTP server
    let listen_addr = args.listen_addr.clone();
    let metrics_state = Arc::clone(&shared_metrics);
    tokio::spawn(async move {
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/healthz", get(healthz_handler))
            .with_state(metrics_state);

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

    // Build xtrace query config for autoscaling signals.
    let xtrace = args.common.xtrace_url.as_deref().map(|url| {
        let freshness_ms = std::env::var("NEBULA_XTRACE_METRIC_MAX_AGE_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120_000);
        let cfg = reconcile::XtraceQueryConfig {
            url: url.to_string(),
            token: args.common.xtrace_token.clone().unwrap_or_default(),
            freshness_ms,
        };
        info!(
            xtrace_url=%cfg.url,
            freshness_ms=cfg.freshness_ms,
            "xtrace signal query enabled"
        );
        cfg
    });

    // Spawn reconcile loop (health self-healing)
    let store_for_reconcile = store.clone();
    let default_port_for_reconcile = args.default_port;
    let metrics_for_reconcile = Arc::clone(&shared_metrics);
    tokio::spawn(async move {
        reconcile::reconcile_loop(
            store_for_reconcile,
            default_port_for_reconcile,
            xtrace,
            metrics_for_reconcile,
        )
        .await;
    });

    // Spawn deployment watch loop (new declarative path)
    let store_for_deploy = store.clone();
    let default_port_for_deploy = args.default_port;
    tokio::spawn(async move {
        deployment_watch_loop(store_for_deploy, default_port_for_deploy).await;
    });

    // Watch for model requests (legacy path)
    let prefix = "/model_requests/";

    loop {
        info!("watching prefix: {}", prefix);
        let mut stream = match store.watch_prefix(prefix, None).await {
            Ok(s) => s,
            Err(e) => {
                error!("failed to watch prefix: {}, retrying in 5s", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        while let Some(event) = stream.next().await {
            let Some(value) = event.value else { continue };

            let mut req: ModelRequest = match serde_json::from_slice(&value) {
                Ok(r) => r,
                Err(e) => {
                    warn!("failed to deserialize model request: {}", e);
                    continue;
                }
            };

            if req.status == ModelRequestStatus::Pending {
                info!(
                    "processing pending request: {} (model={})",
                    req.id, req.request.model_name
                );

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

                let plan =
                    match build_plan_multi(&store, &req, args.default_port, used_ports, used_gpus)
                        .await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            error!("failed to build placement plan: {}", e);
                            continue;
                        }
                    };

                // 3. Write Placement
                let placement_key = format!("/placements/{}", plan.model_uid);
                let placement_val = serde_json::to_vec(&plan)?;

                if let Err(e) = write_placement_cas(&store, &placement_key, placement_val).await {
                    error!("failed to write placement: {}", e);
                    continue;
                }
                info!("wrote placement to {} (CAS)", placement_key);

                // 3. Update Request Status
                req.status = ModelRequestStatus::Scheduled;
                if let Ok(updated_val) = serde_json::to_vec(&req) {
                    let req_key = format!("{}{}", prefix, req.id);
                    let _ = store.put(&req_key, updated_val, None).await;
                    info!("updated request {} status to Scheduled", req.id);
                }
            } else if req.status == ModelRequestStatus::Unloading {
                info!(
                    "processing unloading request: {} (model={})",
                    req.id, req.request.model_name
                );

                let placement_key = format!("/placements/{}", req.request.model_uid);
                if let Err(e) = store.delete(&placement_key).await {
                    warn!("failed to delete placement {}: {}", placement_key, e);
                } else {
                    info!("deleted placement {}", placement_key);
                }

                let req_key = format!("{}{}", prefix, req.id);
                if let Err(e) = store.delete(&req_key).await {
                    error!("failed to delete request key {}: {}", req_key, e);
                } else {
                    info!("successfully cleaned up request {}", req.id);
                }
            }
        }

        warn!("watch stream ended, reconnecting...");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Watch `/deployments/` prefix for the new declarative model management flow.
async fn deployment_watch_loop(store: EtcdMetaStore, default_port: u16) {
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
            match event.value {
                Some(value) => {
                    // Deployment created or updated
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

                        // Read ModelSpec
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

                        let plan = match build_plan_from_deployment(
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
                        match serde_json::to_vec(&plan) {
                            Ok(val) => {
                                if let Err(e) =
                                    write_placement_cas(&store, &placement_key, val).await
                                {
                                    error!(
                                        model_uid=%deployment.model_uid,
                                        error=%e,
                                        "failed to write placement"
                                    );
                                } else {
                                    info!(
                                        model_uid=%deployment.model_uid,
                                        "wrote placement to {} (CAS)",
                                        placement_key
                                    );
                                }
                            }
                            Err(e) => {
                                error!(
                                    model_uid=%deployment.model_uid,
                                    error=%e,
                                    "failed to serialize placement plan"
                                );
                            }
                        }
                    } else if deployment.desired_state == DesiredState::Stopped {
                        info!(
                            model_uid=%deployment.model_uid,
                            "deployment stopped: deleting placement"
                        );
                        let placement_key = format!("/placements/{}", deployment.model_uid);
                        if let Err(e) = store.delete(&placement_key).await {
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
                    // Deployment deleted — extract model_uid from key
                    let model_uid = event.key.strip_prefix(prefix).unwrap_or(&event.key);
                    if model_uid.is_empty() {
                        continue;
                    }
                    info!(
                        model_uid=%model_uid,
                        "deployment deleted: deleting placement"
                    );
                    let placement_key = format!("/placements/{}", model_uid);
                    if let Err(e) = store.delete(&placement_key).await {
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
