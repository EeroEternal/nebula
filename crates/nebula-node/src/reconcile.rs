use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use nebula_common::{
    EndpointInfo, EndpointKind, EndpointStatus, ModelRequest, ModelRequestStatus, ModelSpec,
    PlacementAssignment, PlacementPlan,
};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::args::Args;
use crate::engine::{write_engine_env, wait_engine_ready, Engine, EngineHandle, EngineStartContext};
use crate::heartbeat::{
    delete_capability, delete_endpoint, delete_stats, register_capability, register_endpoint,
};
use crate::util::now_ms;

/// Local running / endpoint index key: (model_uid, replica_id).
pub type ReplicaKey = (String, u32);

pub struct RunningModel {
    pub model_uid: String,
    pub replica_id: u32,
    pub assignment_signature: String,
    /// Placement plan version last published on the endpoint (router filters on this).
    pub plan_version: u64,
    pub handle: EngineHandle,
    pub engine: Arc<dyn Engine>,
    /// Context needed to rebuild the engine on local restart.
    pub start_ctx: EngineStartContext,
    pub request_id: Option<String>,
    /// When set, this replica is draining after scale-in / unassign.
    pub drain_started_ms: Option<u64>,
    /// Recovery budget exhausted — skip further restarts.
    pub failed: bool,
    /// Last runtime/static capability snapshot (refreshed into etcd by heartbeat).
    pub capability: Option<nebula_common::ReplicaCapability>,
}

/// Max time to wait for in-flight traffic before force-stopping a draining replica.
const DRAIN_TIMEOUT_MS: u64 = 120_000;

pub fn replica_key(model_uid: &str, replica_id: u32) -> ReplicaKey {
    (model_uid.to_string(), replica_id)
}

fn assignment_signature(assignment: &PlacementAssignment) -> String {
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

/// Assignments on this node from a placement plan.
pub fn local_assignments<'a>(
    plan: &'a PlacementPlan,
    node_id: &str,
) -> Vec<&'a PlacementAssignment> {
    plan.assignments
        .iter()
        .filter(|a| a.node_id == node_id)
        .collect()
}

/// Replica ids currently running locally for `model_uid`.
pub fn running_replica_ids(
    running: &HashMap<ReplicaKey, RunningModel>,
    model_uid: &str,
) -> Vec<u32> {
    running
        .keys()
        .filter(|(uid, _)| uid == model_uid)
        .map(|(_, rid)| *rid)
        .collect()
}

/// Whether any local replica exists for `model_uid`.
pub fn has_local_replica(running: &HashMap<ReplicaKey, RunningModel>, model_uid: &str) -> bool {
    running.keys().any(|(uid, _)| uid == model_uid)
}

async fn pending_requests(store: &EtcdMetaStore, model_uid: &str, replica_id: u32) -> Option<u64> {
    let key = format!("/stats/{model_uid}/{replica_id}");
    let Ok(Some((bytes, _))) = store.get(&key).await else {
        return None;
    };
    let stats: nebula_common::EndpointStats = serde_json::from_slice(&bytes).ok()?;
    Some(stats.pending_requests)
}

pub(crate) async fn mark_request_failed(store: &EtcdMetaStore, request_id: &str, reason: String) {
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
    endpoint_state: &Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    model_uid: &str,
    replica_id: u32,
    ttl_ms: u64,
) -> anyhow::Result<()> {
    let key = replica_key(model_uid, replica_id);
    let info = {
        let mut guard = endpoint_state.lock().await;
        if let Some(ep) = guard.get_mut(&key) {
            if ep.status != EndpointStatus::Draining {
                ep.status = EndpointStatus::Draining;
                ep.last_heartbeat_ms = now_ms();
                Some(ep.clone())
            } else {
                None
            }
        } else {
            None
        }
    };
    if let Some(ep) = info {
        register_endpoint(store, &ep, ttl_ms, None).await?;
        tracing::info!(%model_uid, replica_id, "endpoint marked Draining");
    }
    Ok(())
}

/// Stop engine outside the running lock (C2).
async fn stop_engine_outside(mut rm: RunningModel) -> anyhow::Result<()> {
    tracing::info!(
        model_uid=%rm.model_uid,
        replica_id=rm.replica_id,
        "stopping engine"
    );
    rm.engine.stop(&mut rm.handle).await
}

async fn finish_drain_stop(
    store: &EtcdMetaStore,
    running: &Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    endpoint_state: &Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    model_uid: &str,
    replica_id: u32,
) -> anyhow::Result<()> {
    let key = replica_key(model_uid, replica_id);
    let removed = {
        let mut guard = running.lock().await;
        guard.remove(&key)
    };
    if let Some(rm) = removed {
        tracing::info!(%model_uid, replica_id, "drain complete; stopping engine");
        let _ = stop_engine_outside(rm).await;
        let _ = delete_endpoint(store, model_uid, replica_id).await;
        let _ = delete_stats(store, model_uid, replica_id).await;
        let _ = delete_capability(store, model_uid, replica_id).await;
        endpoint_state.lock().await.remove(&key);
    }
    Ok(())
}

/// Scale-in / unassign: stop accepting new traffic, wait for idle or timeout, then stop.
async fn drain_then_stop(
    store: &EtcdMetaStore,
    args: &Args,
    running: &Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    endpoint_state: &Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    model_uid: &str,
    replica_id: u32,
) -> anyhow::Result<()> {
    let key = replica_key(model_uid, replica_id);
    let now = now_ms();
    let (just_started, started) = {
        let mut guard = running.lock().await;
        let Some(rm) = guard.get_mut(&key) else {
            return Ok(());
        };
        if rm.drain_started_ms.is_none() {
            rm.drain_started_ms = Some(now);
            (true, now)
        } else {
            (false, rm.drain_started_ms.unwrap_or(now))
        }
    };

    if just_started {
        // etcd write outside running lock
        mark_endpoint_draining(store, endpoint_state, model_uid, replica_id, args.heartbeat_ttl_ms)
            .await?;
        return Ok(());
    }

    // etcd stats read outside running lock
    let pending = pending_requests(store, model_uid, replica_id)
        .await
        .unwrap_or(0);
    let timed_out = now.saturating_sub(started) >= DRAIN_TIMEOUT_MS;

    if pending == 0 || timed_out {
        if timed_out && pending > 0 {
            tracing::warn!(%model_uid, replica_id, pending, "drain timeout reached; force stopping");
        }
        finish_drain_stop(store, running, endpoint_state, model_uid, replica_id).await?;
    } else {
        tracing::info!(%model_uid, replica_id, pending, "draining; waiting for in-flight to finish");
        mark_endpoint_draining(store, endpoint_state, model_uid, replica_id, args.heartbeat_ttl_ms)
            .await?;
    }
    Ok(())
}

async fn drain_all_local(
    store: &EtcdMetaStore,
    args: &Args,
    running: &Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    endpoint_state: &Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    model_uid: &str,
) -> anyhow::Result<()> {
    let rids = {
        let guard = running.lock().await;
        running_replica_ids(&guard, model_uid)
    };
    for rid in rids {
        drain_then_stop(store, args, running, endpoint_state, model_uid, rid).await?;
    }
    Ok(())
}

async fn stop_replica(
    store: &EtcdMetaStore,
    running: &Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    endpoint_state: &Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    model_uid: &str,
    replica_id: u32,
) -> anyhow::Result<()> {
    let key = replica_key(model_uid, replica_id);
    let removed = {
        let mut guard = running.lock().await;
        guard.remove(&key)
    };
    if let Some(rm) = removed {
        tracing::info!(%model_uid, replica_id, "stopping engine due to placement update");
        let _ = stop_engine_outside(rm).await;
        let _ = delete_endpoint(store, model_uid, replica_id).await;
        let _ = delete_stats(store, model_uid, replica_id).await;
        let _ = delete_capability(store, model_uid, replica_id).await;
        endpoint_state.lock().await.remove(&key);
    }
    Ok(())
}

/// Heavy start path with no `running` lock held (C2).
async fn launch_replica_engine(
    store: &EtcdMetaStore,
    args: &Args,
    plan: &PlacementPlan,
    assignment: &PlacementAssignment,
) -> anyhow::Result<Option<(RunningModel, EndpointInfo)>> {
    let model_uid = plan.model_uid.as_str();
    let replica_id = assignment.replica_id;
    let desired_signature = assignment_signature(assignment);

    let engine_type = assignment.engine_type.as_deref();
    let docker_image_override = assignment.docker_image.as_deref();
    let engine: Arc<dyn Engine> = Arc::from(crate::engine::create_engine(
        args,
        engine_type,
        docker_image_override,
    )?);
    let caps = engine.capabilities();
    tracing::info!(
        engine_type = %engine.engine_type(),
        capability_source = ?caps.source,
        "engine adapter selected"
    );

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
            tracing::error!(%model_uid, replica_id, %image, "docker image not available locally");
            if let Some(request_id) = plan.request_id.as_deref() {
                mark_request_failed(store, request_id, reason).await;
            }
            return Ok(None);
        }
    }

    let mut engine_model_path = plan.model_name.clone();
    let spec_key = format!("/models/{}/spec", model_uid);
    if let Ok(Some((spec_bytes, _))) = store.get(&spec_key).await {
        if let Ok(spec) = serde_json::from_slice::<ModelSpec>(&spec_bytes) {
            tracing::info!(%model_uid, replica_id, source=?spec.model_source, "ensuring model files are available");
            match crate::model_cache_manager::download_model_if_needed(
                store,
                &args.node_id,
                model_uid,
                &spec.model_name,
                &spec.model_source,
                spec.model_path.as_deref(),
                &args.vllm_model_dir,
                replica_id,
                args.vllm_hf_endpoint.as_deref(),
                args.vllm_use_modelscope,
            )
            .await
            {
                Ok(path) => engine_model_path = path,
                Err(e) => {
                    let reason = format!("model download failed: {}", e);
                    tracing::error!(%model_uid, replica_id, error=%e, "model download failed");
                    if let Some(request_id) = plan.request_id.as_deref() {
                        mark_request_failed(store, request_id, reason).await;
                    }
                    return Ok(None);
                }
            }
        }
    }

    let ctx = EngineStartContext {
        model_uid: model_uid.to_string(),
        model_name: engine_model_path,
        replica_id,
        port: assignment.port,
        gpu_indices: assignment.effective_gpu_indices(),
        engine_config_path: assignment.engine_config_path.clone(),
        extra_args: assignment.extra_args.clone(),
        ready_timeout: Duration::from_secs(args.ready_timeout_secs),
    };

    let handle = if let Some(h) = engine.try_reuse(&ctx).await {
        tracing::info!(%model_uid, replica_id, base_url=%h.base_url, engine=%engine.engine_type(), "reused existing engine instance");
        h
    } else {
        tracing::info!(%model_uid, replica_id, engine=%engine.engine_type(), "starting new engine instance");
        match engine.start(ctx.clone()).await {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(%model_uid, replica_id, error=%e, engine=%engine.engine_type(), "failed to start engine");
                if let Some(request_id) = plan.request_id.as_deref() {
                    mark_request_failed(store, request_id, e.to_string()).await;
                }
                return Ok(None);
            }
        }
    };

    let engine_model = match wait_engine_ready(&handle.base_url, ctx.ready_timeout).await {
        Ok(name) => name,
        Err(e) => {
            tracing::error!(%model_uid, replica_id, error=%e, "engine not ready within timeout");
            if let Some(request_id) = plan.request_id.as_deref() {
                mark_request_failed(store, request_id, e.to_string()).await;
            }
            let mut h = handle;
            let _ = engine.stop(&mut h).await;
            return Ok(None);
        }
    };
    let mut handle = handle;
    if !engine_model.is_empty() {
        handle.engine_model = engine_model;
    } else if handle.engine_model.is_empty() {
        handle.engine_model = plan.model_name.clone();
    }

    // Best-effort runtime capability discovery (never blocks Ready).
    let mut capability = None;
    if let Ok(http) = nebula_common::health_http_client() {
        let discovered = crate::engine::discover::discover_runtime_capability(
            &http,
            engine.engine_type(),
            &handle.base_url,
        )
        .await;
        tracing::info!(
            %model_uid,
            replica_id,
            engine_type = %discovered.engine_type,
            capability_source = ?discovered.source,
            engine_version = ?discovered.engine_version,
            pending = ?discovered.observability.pending_requests,
            kv = ?discovered.observability.kv_cache_usage,
            "runtime capability discovery"
        );
        let cap = nebula_common::ReplicaCapability {
            model_uid: plan.model_uid.clone(),
            replica_id,
            capability: discovered,
            updated_at_ms: now_ms(),
        };
        let _ = register_capability(store, &cap, args.heartbeat_ttl_ms, None).await;
        capability = Some(cap);
    }

    let env_path = format!("{}-{}", args.engine_env_path, replica_id);
    write_engine_env(&env_path, &handle.base_url, &handle.engine_model).await?;

    let info = EndpointInfo {
        model_uid: plan.model_uid.clone(),
        replica_id,
        plan_version: plan.version,
        node_id: args.node_id.clone(),
        endpoint_kind: EndpointKind::NativeHttp,
        api_flavor: "openai".to_string(),
        status: EndpointStatus::Ready,
        last_heartbeat_ms: now_ms(),
        grpc_target: None,
        base_url: Some(handle.base_url.clone()),
    };

    let rm = RunningModel {
        model_uid: plan.model_uid.clone(),
        replica_id,
        assignment_signature: desired_signature,
        plan_version: plan.version,
        handle,
        engine,
        start_ctx: ctx,
        request_id: plan.request_id.clone(),
        drain_started_ms: None,
        failed: false,
        capability,
    };
    Ok(Some((rm, info)))
}

async fn start_replica(
    store: &EtcdMetaStore,
    args: &Args,
    running: &Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    starting: &Arc<Mutex<HashSet<ReplicaKey>>>,
    endpoint_state: &Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    plan: &PlacementPlan,
    assignment: &PlacementAssignment,
) -> anyhow::Result<()> {
    let key = replica_key(&plan.model_uid, assignment.replica_id);
    // Skip if already present (another reconcile may have won).
    {
        let guard = running.lock().await;
        if guard.contains_key(&key) {
            return Ok(());
        }
    }
    {
        let guard = starting.lock().await;
        if guard.contains(&key) {
            tracing::debug!(?key, "replica start already in progress");
            return Ok(());
        }
    }
    starting.lock().await.insert(key.clone());

    let launch_result = launch_replica_engine(store, args, plan, assignment).await;
    starting.lock().await.remove(&key);

    let Some((rm, info)) = launch_result? else {
        return Ok(());
    };

    // Commit under lock; if raced, tear down the duplicate outside.
    let duplicate = {
        let mut guard = running.lock().await;
        if guard.contains_key(&key) {
            Some(rm)
        } else {
            guard.insert(key.clone(), rm);
            None
        }
    };
    if let Some(dup) = duplicate {
        tracing::warn!(
            model_uid=%plan.model_uid,
            replica_id=assignment.replica_id,
            "duplicate start raced; stopping extra engine"
        );
        let _ = stop_engine_outside(dup).await;
        return Ok(());
    }

    register_endpoint(store, &info, args.heartbeat_ttl_ms, None).await?;
    endpoint_state.lock().await.insert(key, info);
    tracing::info!(
        model_uid=%plan.model_uid,
        replica_id=assignment.replica_id,
        "registered endpoint"
    );
    Ok(())
}

/// Reconcile local replicas for one model against the desired placement plan.
///
/// C2: holds `running` only for snapshot / commit; download / start / stop / etcd
/// side-effects run outside the lock.
pub async fn reconcile_model(
    store: &EtcdMetaStore,
    args: &Args,
    running: &Arc<Mutex<HashMap<ReplicaKey, RunningModel>>>,
    starting: &Arc<Mutex<HashSet<ReplicaKey>>>,
    endpoint_state: &Arc<Mutex<HashMap<ReplicaKey, EndpointInfo>>>,
    last_epochs: &Arc<Mutex<HashMap<String, u64>>>,
    model_uid: &str,
    plan: Option<PlacementPlan>,
) -> anyhow::Result<()> {
    let plan = match plan {
        Some(p) => p,
        None => {
            return drain_all_local(store, args, running, endpoint_state, model_uid).await;
        }
    };

    {
        let epochs = last_epochs.lock().await;
        if should_reject_stale_epoch(&plan, &epochs) {
            tracing::warn!(
                %model_uid,
                plan_epoch = plan.leader_epoch,
                last_epoch = epochs.get(model_uid).copied().unwrap_or(0),
                "rejecting stale placement (leader_epoch fencing)"
            );
            return Ok(());
        }
    }

    let desired = local_assignments(&plan, &args.node_id);
    let desired_ids: HashSet<u32> = desired.iter().map(|a| a.replica_id).collect();

    // Snapshot local replica ids under short lock.
    let local_rids = {
        let guard = running.lock().await;
        running_replica_ids(&guard, model_uid)
    };
    for rid in local_rids {
        if !desired_ids.contains(&rid) {
            drain_then_stop(store, args, running, endpoint_state, model_uid, rid).await?;
        }
    }

    if desired.is_empty() {
        if plan.leader_epoch > 0 {
            last_epochs
                .lock()
                .await
                .insert(model_uid.to_string(), plan.leader_epoch);
        }
        return Ok(());
    }

    enum Action {
        KeepRefreshVersion,
        Restart,
        Start,
    }

    let mut actions: Vec<(PlacementAssignment, Action)> = Vec::new();
    {
        let mut guard = running.lock().await;
        for assignment in &desired {
            let key = replica_key(model_uid, assignment.replica_id);
            let desired_signature = assignment_signature(assignment);
            match guard.get_mut(&key) {
                Some(rm) if rm.assignment_signature == desired_signature => {
                    rm.drain_started_ms = None;
                    if rm.plan_version != plan.version {
                        actions.push(((*assignment).clone(), Action::KeepRefreshVersion));
                    }
                }
                Some(_) => {
                    let key = replica_key(model_uid, assignment.replica_id);
                    if starting.lock().await.contains(&key) {
                        continue;
                    }
                    actions.push(((*assignment).clone(), Action::Restart))
                }
                None => {
                    let key = replica_key(model_uid, assignment.replica_id);
                    if starting.lock().await.contains(&key) {
                        continue;
                    }
                    actions.push(((*assignment).clone(), Action::Start))
                }
            }
        }
    }

    for (assignment, action) in actions {
        match action {
            Action::KeepRefreshVersion => {
                let key = replica_key(model_uid, assignment.replica_id);
                let refreshed = {
                    let mut ep = endpoint_state.lock().await;
                    ep.get_mut(&key).map(|info| {
                        info.plan_version = plan.version;
                        info.last_heartbeat_ms = now_ms();
                        info.clone()
                    })
                };
                if let Some(info) = refreshed {
                    register_endpoint(store, &info, args.heartbeat_ttl_ms, None).await?;
                }
                if let Some(rm) = running.lock().await.get_mut(&key) {
                    rm.plan_version = plan.version;
                }
                tracing::info!(
                    %model_uid,
                    replica_id = assignment.replica_id,
                    plan_version = plan.version,
                    "refreshed endpoint plan_version after placement bump"
                );
            }
            Action::Restart => {
                stop_replica(
                    store,
                    running,
                    endpoint_state,
                    model_uid,
                    assignment.replica_id,
                )
                .await?;
                start_replica(
                    store,
                    args,
                    running,
                    starting,
                    endpoint_state,
                    &plan,
                    &assignment,
                )
                .await?;
            }
            Action::Start => {
                start_replica(
                    store,
                    args,
                    running,
                    starting,
                    endpoint_state,
                    &plan,
                    &assignment,
                )
                .await?;
            }
        }
    }

    if plan.leader_epoch > 0 {
        last_epochs
            .lock()
            .await
            .insert(model_uid.to_string(), plan.leader_epoch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assignment(replica_id: u32, node_id: &str, port: u16) -> PlacementAssignment {
        PlacementAssignment {
            replica_id,
            node_id: node_id.into(),
            engine_config_path: "/tmp/cfg.yaml".into(),
            port,
            gpu_index: None,
            gpu_indices: None,
            extra_args: None,
            engine_type: None,
            docker_image: None,
        }
    }

    #[test]
    fn rejects_lower_leader_epoch() {
        let mut last = HashMap::new();
        last.insert("m1".into(), 5u64);
        let plan = PlacementPlan {
            request_id: None,
            model_uid: "m1".into(),
            model_name: "m".into(),
            version: 1,
            updated_at_ms: 0,
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

    #[test]
    fn local_assignments_returns_all_on_node() {
        let plan = PlacementPlan {
            request_id: None,
            model_uid: "m1".into(),
            model_name: "m".into(),
            version: 1,
            updated_at_ms: 0,
            leader_epoch: 1,
            assignments: vec![
                assignment(0, "node_a", 8000),
                assignment(1, "node_a", 8001),
                assignment(2, "node_b", 8002),
            ],
        };
        let local = local_assignments(&plan, "node_a");
        assert_eq!(local.len(), 2);
        assert_eq!(local[0].replica_id, 0);
        assert_eq!(local[1].replica_id, 1);
    }

    #[test]
    fn set_diff_identifies_excess_and_missing() {
        let plan = PlacementPlan {
            request_id: None,
            model_uid: "m1".into(),
            model_name: "m".into(),
            version: 1,
            updated_at_ms: 0,
            leader_epoch: 1,
            assignments: vec![
                assignment(0, "node_a", 8000),
                assignment(1, "node_a", 8001),
            ],
        };
        let desired: HashSet<u32> = local_assignments(&plan, "node_a")
            .iter()
            .map(|a| a.replica_id)
            .collect();

        // Simulate running replicas 0 and 2 locally.
        let running_ids: HashSet<u32> = [0u32, 2].into_iter().collect();
        let excess: Vec<u32> = {
            let mut v: Vec<u32> = running_ids.difference(&desired).copied().collect();
            v.sort_unstable();
            v
        };
        let missing: Vec<u32> = {
            let mut v: Vec<u32> = desired.difference(&running_ids).copied().collect();
            v.sort_unstable();
            v
        };

        assert_eq!(excess, vec![2]);
        assert_eq!(missing, vec![1]);
    }
}
