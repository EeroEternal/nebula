use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use nebula_common::{
    EndpointInfo, EndpointKind, EndpointStatus, ModelRequest, ModelRequestStatus, ModelSpec,
    PlacementPlan,
};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::args::Args;
use crate::engine::{write_engine_env, Engine, EngineHandle, EngineStartContext};
use crate::heartbeat::{delete_endpoint, register_endpoint};
use crate::util::now_ms;

pub struct RunningModel {
    pub model_uid: String,
    pub replica_id: u32,
    pub assignment_signature: String,
    pub handle: EngineHandle,
    pub engine: Arc<dyn Engine>,
    /// When set, this replica is draining after scale-in / unassign.
    pub drain_started_ms: Option<u64>,
}

/// Max time to wait for in-flight traffic before force-stopping a draining replica.
const DRAIN_TIMEOUT_MS: u64 = 120_000;

fn assignment_signature(assignment: &nebula_common::PlacementAssignment) -> String {
    serde_json::to_string(assignment).unwrap_or_else(|_| {
        format!(
            "replica:{}:node:{}:port:{}:gpu:{:?}:extra:{:?}:engine:{:?}:image:{:?}",
            assignment.replica_id,
            assignment.node_id,
            assignment.port,
            assignment.effective_gpu_indices(),
            assignment.extra_args,
            assignment.engine_type,
            assignment.docker_image
        )
    })
}

fn should_reject_stale_epoch(plan: &PlacementPlan, last_epochs: &HashMap<String, u64>) -> bool {
    if plan.leader_epoch == 0 {
        return false;
    }
    match last_epochs.get(&plan.model_uid) {
        Some(&last) if plan.leader_epoch < last => true,
        _ => false,
    }
}

async fn pending_requests(store: &EtcdMetaStore, model_uid: &str, replica_id: u32) -> Option<u64> {
    let key = format!("/stats/{model_uid}/{replica_id}");
    let Ok(Some((bytes, _))) = store.get(&key).await else {
        return None;
    };
    let stats: nebula_common::EndpointStats = serde_json::from_slice(&bytes).ok()?;
    Some(stats.pending_requests)
}

async fn mark_request_failed(store: &EtcdMetaStore, request_id: &str, reason: String) {
    let key = format!("/model_requests/{request_id}");
    let loaded = store.get(&key).await;
    let Ok(Some((bytes, _rev))) = loaded else {
        tracing::warn!(%request_id, "failed to load model request for failure update");
        return;
    };
    let mut req: ModelRequest = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(%request_id, error=%e, "failed to deserialize model request for failure update");
            return;
        }
    };
    req.status = ModelRequestStatus::Failed(reason);
    let Ok(val) = serde_json::to_vec(&req) else {
        tracing::warn!(%request_id, "failed to serialize model request for failure update");
        return;
    };
    if let Err(e) = store.put(&key, val, None).await {
        tracing::warn!(%request_id, error=%e, "failed to persist model request failure update");
    }
}

async fn mark_endpoint_draining(
    store: &EtcdMetaStore,
    endpoint_state: &Arc<Mutex<HashMap<String, EndpointInfo>>>,
    model_uid: &str,
    ttl_ms: u64,
) -> anyhow::Result<()> {
    let mut guard = endpoint_state.lock().await;
    if let Some(ep) = guard.get_mut(model_uid) {
        if ep.status != EndpointStatus::Draining {
            ep.status = EndpointStatus::Draining;
            ep.last_heartbeat_ms = now_ms();
            register_endpoint(store, ep, ttl_ms).await?;
            tracing::info!(%model_uid, replica_id = ep.replica_id, "endpoint marked Draining");
        }
    }
    Ok(())
}

async fn finish_drain_stop(
    store: &EtcdMetaStore,
    running: &mut HashMap<String, RunningModel>,
    endpoint_state: &Arc<Mutex<HashMap<String, EndpointInfo>>>,
    model_uid: &str,
) -> anyhow::Result<()> {
    if let Some(mut rm) = running.remove(model_uid) {
        tracing::info!(%model_uid, "drain complete; stopping engine");
        rm.engine.stop(&mut rm.handle).await?;
        let _ = delete_endpoint(store, &rm.model_uid, rm.replica_id).await;
        endpoint_state.lock().await.remove(model_uid);
    }
    Ok(())
}

/// Scale-in / unassign: stop accepting new traffic, wait for idle or timeout, then stop.
async fn drain_then_stop(
    store: &EtcdMetaStore,
    args: &Args,
    running: &mut HashMap<String, RunningModel>,
    endpoint_state: &Arc<Mutex<HashMap<String, EndpointInfo>>>,
    model_uid: &str,
) -> anyhow::Result<()> {
    let now = now_ms();
    let (just_started, started, replica_id) = {
        let Some(rm) = running.get_mut(model_uid) else {
            return Ok(());
        };
        if rm.drain_started_ms.is_none() {
            rm.drain_started_ms = Some(now);
            (true, now, rm.replica_id)
        } else {
            (false, rm.drain_started_ms.unwrap_or(now), rm.replica_id)
        }
    };

    if just_started {
        mark_endpoint_draining(store, endpoint_state, model_uid, args.heartbeat_ttl_ms).await?;
        return Ok(());
    }

    let pending = pending_requests(store, model_uid, replica_id)
        .await
        .unwrap_or(0);
    let timed_out = now.saturating_sub(started) >= DRAIN_TIMEOUT_MS;

    if pending == 0 || timed_out {
        if timed_out && pending > 0 {
            tracing::warn!(%model_uid, pending, "drain timeout reached; force stopping");
        }
        finish_drain_stop(store, running, endpoint_state, model_uid).await?;
    } else {
        tracing::info!(%model_uid, pending, "draining; waiting for in-flight to finish");
        mark_endpoint_draining(store, endpoint_state, model_uid, args.heartbeat_ttl_ms).await?;
    }
    Ok(())
}

pub async fn reconcile_model(
    store: &EtcdMetaStore,
    args: &Args,
    running: &mut HashMap<String, RunningModel>,
    endpoint_state: &Arc<Mutex<HashMap<String, EndpointInfo>>>,
    last_epochs: &mut HashMap<String, u64>,
    model_uid: &str,
    plan: Option<PlacementPlan>,
) -> anyhow::Result<()> {
    let plan = match plan {
        Some(p) => p,
        None => {
            return drain_then_stop(store, args, running, endpoint_state, model_uid).await;
        }
    };

    if should_reject_stale_epoch(&plan, last_epochs) {
        tracing::warn!(
            %model_uid,
            plan_epoch = plan.leader_epoch,
            last_epoch = last_epochs.get(model_uid).copied().unwrap_or(0),
            "rejecting stale placement (leader_epoch fencing)"
        );
        return Ok(());
    }

    let desired = plan.assignments.iter().find(|a| a.node_id == args.node_id);

    let Some(assignment) = desired else {
        return drain_then_stop(store, args, running, endpoint_state, model_uid).await;
    };

    if let Some(rm) = running.get_mut(model_uid) {
        rm.drain_started_ms = None;
    }

    let desired_signature = assignment_signature(assignment);
    let needs_restart = match running.get(model_uid) {
        Some(rm) => {
            rm.replica_id != assignment.replica_id || rm.assignment_signature != desired_signature
        }
        None => true,
    };

    if !needs_restart {
        if plan.leader_epoch > 0 {
            last_epochs.insert(model_uid.to_string(), plan.leader_epoch);
        }
        return Ok(());
    }

    if let Some(mut rm) = running.remove(model_uid) {
        tracing::info!(%model_uid, "restarting engine due to placement update");
        rm.engine.stop(&mut rm.handle).await?;
        let _ = delete_endpoint(store, &rm.model_uid, rm.replica_id).await;
        endpoint_state.lock().await.remove(model_uid);
    }

    let engine_type = assignment.engine_type.as_deref();
    let docker_image_override = assignment.docker_image.as_deref();
    let engine: Arc<dyn Engine> = Arc::from(crate::engine::create_engine(
        args,
        engine_type,
        docker_image_override,
    ));

    let ctx = EngineStartContext {
        model_uid: model_uid.to_string(),
        model_name: plan.model_name.clone(),
        replica_id: assignment.replica_id,
        port: assignment.port,
        gpu_indices: assignment.effective_gpu_indices(),
        engine_config_path: assignment.engine_config_path.clone(),
        extra_args: assignment.extra_args.clone(),
        ready_timeout: Duration::from_secs(args.ready_timeout_secs),
    };

    if let Some(image) = assignment.docker_image.as_deref() {
        let output = tokio::process::Command::new("docker")
            .args(["images", "-q", image])
            .output()
            .await;
        let exists = match output {
            Ok(o) => o.status.success() && !o.stdout.is_empty(),
            Err(_) => false,
        };
        if !exists {
            let reason = format!(
                "Docker image '{}' not found on this node. \
                 Please register and pre-pull the image via the Images page before deploying.",
                image
            );
            tracing::error!(%model_uid, %image, "docker image not available locally");
            if let Some(request_id) = plan.request_id.as_deref() {
                mark_request_failed(store, request_id, reason).await;
            }
            return Ok(());
        }
    }

    let spec_key = format!("/models/{}/spec", model_uid);
    if let Ok(Some((spec_bytes, _))) = store.get(&spec_key).await {
        if let Ok(spec) = serde_json::from_slice::<ModelSpec>(&spec_bytes) {
            tracing::info!(%model_uid, source=?spec.model_source, "ensuring model files are available");
            if let Err(e) = crate::model_cache_manager::download_model_if_needed(
                store,
                &args.node_id,
                model_uid,
                &spec.model_name,
                &spec.model_source,
                spec.model_path.as_deref(),
                &args.vllm_model_dir,
                assignment.replica_id,
                args.vllm_hf_endpoint.as_deref(),
                args.vllm_use_modelscope,
            )
            .await
            {
                let reason = format!("model download failed: {}", e);
                tracing::error!(%model_uid, error=%e, "model download failed");
                if let Some(request_id) = plan.request_id.as_deref() {
                    mark_request_failed(store, request_id, reason).await;
                }
                return Ok(());
            }
        }
    }

    let handle = if let Some(h) = engine.try_reuse(&ctx).await {
        tracing::info!(%model_uid, base_url=%h.base_url, engine=%engine.engine_type(), "reused existing engine instance");
        h
    } else {
        tracing::info!(%model_uid, engine=%engine.engine_type(), "starting new engine instance");
        match engine.start(ctx).await {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(%model_uid, error=%e, engine=%engine.engine_type(), "failed to start engine");
                if let Some(request_id) = plan.request_id.as_deref() {
                    mark_request_failed(store, request_id, e.to_string()).await;
                }
                return Ok(());
            }
        }
    };

    write_engine_env(
        &args.engine_env_path,
        &handle.base_url,
        &handle.engine_model,
    )
    .await?;

    let info = EndpointInfo {
        model_uid: plan.model_uid.clone(),
        replica_id: assignment.replica_id,
        plan_version: plan.version,
        node_id: args.node_id.clone(),
        endpoint_kind: EndpointKind::NativeHttp,
        api_flavor: "openai".to_string(),
        status: EndpointStatus::Ready,
        last_heartbeat_ms: now_ms(),
        grpc_target: None,
        base_url: Some(handle.base_url.clone()),
    };

    register_endpoint(store, &info, args.heartbeat_ttl_ms).await?;
    tracing::info!(model_uid=%info.model_uid, replica_id=info.replica_id, base_url=%handle.base_url, "registered endpoint");

    endpoint_state
        .lock()
        .await
        .insert(model_uid.to_string(), info);
    running.insert(
        model_uid.to_string(),
        RunningModel {
            model_uid: plan.model_uid.clone(),
            replica_id: assignment.replica_id,
            assignment_signature: desired_signature,
            handle,
            engine,
            drain_started_ms: None,
        },
    );
    if plan.leader_epoch > 0 {
        last_epochs.insert(model_uid.to_string(), plan.leader_epoch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_lower_leader_epoch() {
        let mut last = HashMap::new();
        last.insert("m1".into(), 5u64);
        let plan = PlacementPlan {
            request_id: None,
            model_uid: "m1".into(),
            model_name: "m".into(),
            version: 1,
            leader_epoch: 4,
            assignments: vec![],
        };
        assert!(should_reject_stale_epoch(&plan, &last));
        let plan2 = PlacementPlan {
            leader_epoch: 5,
            ..plan.clone()
        };
        assert!(!should_reject_stale_epoch(&plan2, &last));
        let plan3 = PlacementPlan {
            leader_epoch: 0,
            ..plan
        };
        assert!(!should_reject_stale_epoch(&plan3, &last));
    }
}
