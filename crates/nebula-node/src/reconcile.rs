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
}

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

pub async fn reconcile_model(
    store: &EtcdMetaStore,
    args: &Args,
    running: &mut HashMap<String, RunningModel>,
    endpoint_state: &Arc<Mutex<HashMap<String, EndpointInfo>>>,
    model_uid: &str,
    plan: Option<PlacementPlan>,
) -> anyhow::Result<()> {
    let plan = match plan {
        Some(p) => p,
        None => {
            if let Some(mut rm) = running.remove(model_uid) {
                tracing::info!(%model_uid, "stopping engine");
                rm.engine.stop(&mut rm.handle).await?;
                let _ = delete_endpoint(store, &rm.model_uid, rm.replica_id).await;
                endpoint_state.lock().await.remove(model_uid);
            }
            return Ok(());
        }
    };

    let desired = plan.assignments.iter().find(|a| a.node_id == args.node_id);

    let Some(assignment) = desired else {
        if let Some(mut rm) = running.remove(model_uid) {
            tracing::info!(%model_uid, "no longer assigned, stopping engine");
            rm.engine.stop(&mut rm.handle).await?;
            let _ = delete_endpoint(store, &rm.model_uid, rm.replica_id).await;
            endpoint_state.lock().await.remove(model_uid);
        }
        return Ok(());
    };

    let desired_signature = assignment_signature(assignment);
    let needs_restart = match running.get(model_uid) {
        Some(rm) => {
            rm.replica_id != assignment.replica_id || rm.assignment_signature != desired_signature
        }
        None => true,
    };

    if !needs_restart {
        return Ok(());
    }

    if let Some(mut rm) = running.remove(model_uid) {
        tracing::info!(%model_uid, "restarting engine due to placement update");
        rm.engine.stop(&mut rm.handle).await?;
        let _ = delete_endpoint(store, &rm.model_uid, rm.replica_id).await;
        endpoint_state.lock().await.remove(model_uid);
    }

    // Create engine instance based on assignment's engine_type and optional docker image override
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

    // Pre-check: if a docker image is specified, verify it exists locally.
    // This gives the user a clear error instead of a long timeout or cryptic docker failure.
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

    // Pre-download model files if a ModelSpec exists (ensures model is ready before engine start).
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

    // Try to reuse an existing instance before starting a new one.
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
            model_uid: plan.model_uid,
            replica_id: assignment.replica_id,
            assignment_signature: desired_signature,
            handle,
            engine,
        },
    );

    Ok(())
}
