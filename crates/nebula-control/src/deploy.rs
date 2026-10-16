use nebula_common::{
    validate_engine_and_config, validate_replica_specs, DesiredState, ModelConfig, ModelDeployment,
    ModelLoadRequest, ModelSpec, ReplicaPlacementSpec,
};
use nebula_meta::MetaStore;
use serde::Deserialize;

use crate::compat::{resolve_concrete_image_ref, resolve_replica_platform, validate_deploy_compat};
use crate::error::ServiceError;
use crate::store::{
    get_model_deployment, get_model_spec, infer_model_source, is_valid_model_uid, now_ms,
    put_model_deployment, put_model_spec,
};

#[derive(Debug, Clone, Deserialize)]
pub struct StartDeploymentRequest {
    pub replicas: Option<u32>,
    pub min_replicas: Option<u32>,
    pub max_replicas: Option<u32>,
    pub config_overrides: Option<ModelConfig>,
    pub node_id: Option<String>,
    pub gpu_indices: Option<Vec<u32>>,
    #[serde(default)]
    pub replica_specs: Option<Vec<ReplicaPlacementSpec>>,
    #[serde(default)]
    pub image_id: Option<String>,
    #[serde(default)]
    pub image_override_reason: Option<String>,
    #[serde(default)]
    pub callback_url: Option<String>,
    /// pd_split topology (pool.md §pd_split); None = replicated.
    #[serde(default)]
    pub topology: Option<nebula_common::DeploymentTopology>,
    /// Shared-KV orchestration for pd_split.
    #[serde(default)]
    pub kv_sharing: Option<nebula_common::KvSharingSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScaleDeploymentRequest {
    pub replicas: u32,
    #[serde(default)]
    pub callback_url: Option<String>,
}

fn ensure_engine_config(
    engine_type: Option<&str>,
    config: Option<&ModelConfig>,
) -> Result<String, ServiceError> {
    validate_engine_and_config(engine_type, config).map_err(ServiceError::BadRequest)
}

fn gpu_affinity_from_load(req: &ModelLoadRequest) -> Option<Vec<u32>> {
    req.gpu_indices
        .clone()
        .or_else(|| req.gpu_index.map(|i| vec![i]))
}

pub fn validate_callback_url(url: Option<&str>) -> Result<(), ServiceError> {
    let Some(url) = url else {
        return Ok(());
    };
    if !url.starts_with("https://") {
        return Err(ServiceError::BadRequest(
            "callback_url must use https://".to_string(),
        ));
    }
    Ok(())
}

/// True when shrinking/blessing `replicas` would break pd_split's role
/// requirements (v1 forbids role-blind autoscaling).
fn deps_pd_split_scale_invalid(dep: &ModelDeployment, replicas: u32) -> bool {
    let Some(specs) = dep.replica_specs.as_ref() else {
        return true; // pd_split without specs should never have been accepted
    };
    if specs.len() != replicas as usize {
        return true; // resizing needs a matching, explicitly-roled spec list
    }
    let has_prefill = specs
        .iter()
        .any(|s| s.role == Some(nebula_common::ReplicaRole::Prefill));
    let has_decode = specs
        .iter()
        .any(|s| s.role == Some(nebula_common::ReplicaRole::Decode));
    !(has_prefill && has_decode)
}

fn effective_replicas(req: &StartDeploymentRequest) -> u32 {
    req.replicas.unwrap_or(1).max(1)
}

fn validate_start_replica_specs(
    replicas: u32,
    replica_specs: &Option<Vec<ReplicaPlacementSpec>>,
) -> Result<(), ServiceError> {
    if let Some(specs) = replica_specs {
        validate_replica_specs(replicas, specs).map_err(ServiceError::BadRequest)?;
    }
    Ok(())
}

async fn validate_compat_for_replica_specs(
    store: &dyn MetaStore,
    engine_type: &str,
    image_id: Option<&str>,
    docker_image: Option<&str>,
    image_override_reason: Option<&str>,
    replica_specs: &Option<Vec<ReplicaPlacementSpec>>,
    fallback_node: Option<&str>,
) -> Result<Vec<String>, ServiceError> {
    let mut all_ids = Vec::new();
    let mut targets: Vec<(Option<&str>, Option<&str>)> = Vec::new();
    if let Some(specs) = replica_specs {
        for spec in specs {
            targets.push((
                spec.image_id.as_deref().or(image_id),
                spec.node_id.as_deref().or(fallback_node),
            ));
        }
    } else if fallback_node.is_some() {
        targets.push((image_id, fallback_node));
    } else {
        targets.push((image_id, None));
    }

    for (rep_image_id, node_id) in targets {
        let ids = validate_deploy_compat(
            store,
            engine_type,
            None,
            rep_image_id,
            docker_image,
            node_id,
            image_override_reason,
        )
        .await?;
        for id in ids {
            if !all_ids.contains(&id) {
                all_ids.push(id);
            }
        }
    }
    Ok(all_ids)
}

/// Upsert model spec + apply running deployment (compat-validated).
pub async fn load_model(
    store: &dyn MetaStore,
    principal: &str,
    req: ModelLoadRequest,
) -> Result<(ModelDeployment, Option<String>), ServiceError> {
    if !is_valid_model_uid(&req.model_uid) {
        return Err(ServiceError::BadRequest(
            "model_uid must match [a-z0-9][a-z0-9-]* and be at most 63 chars".to_string(),
        ));
    }

    ensure_engine_config(req.engine_type.as_deref(), req.config.as_ref())?;
    validate_callback_url(req.callback_url.as_deref())?;

    let now = now_ms();
    let model_uid = req.model_uid.clone();
    let spec_key_exists = store
        .get(&format!("/models/{model_uid}/spec"))
        .await?
        .is_some();

    let spec = if spec_key_exists {
        let mut existing = get_model_spec(store, &model_uid).await?;
        existing.model_name = req.model_name.clone();
        existing.engine_type = req.engine_type.clone();
        existing.docker_image = req.docker_image.clone();
        existing.config = req.config.clone();
        existing.updated_at_ms = now;
        existing
    } else {
        ModelSpec {
            model_uid: model_uid.clone(),
            model_name: req.model_name.clone(),
            model_source: infer_model_source(&req.model_name, None),
            model_path: None,
            engine_type: req.engine_type.clone(),
            docker_image: req.docker_image.clone(),
            config: req.config.clone(),
            labels: Default::default(),
            created_at_ms: now,
            updated_at_ms: now,
            created_by: Some(principal.to_string()),
        }
    };
    put_model_spec(store, &model_uid, &spec).await?;

    let callback_url = req.callback_url.clone();
    let start = StartDeploymentRequest {
        topology: None,
        kv_sharing: None,
        replicas: Some(req.replicas.max(1)),
        min_replicas: req.min_replicas,
        max_replicas: req.max_replicas,
        config_overrides: req.config.clone(),
        node_id: req.node_id.clone(),
        gpu_indices: gpu_affinity_from_load(&req),
        replica_specs: req.replica_specs.clone(),
        image_id: None,
        image_override_reason: None,
        callback_url: None,
    };
    let dep = start_model(store, &model_uid, start).await?;
    Ok((dep, callback_url))
}

pub async fn start_model(
    store: &dyn MetaStore,
    model_uid: &str,
    req: StartDeploymentRequest,
) -> Result<ModelDeployment, ServiceError> {
    let spec = get_model_spec(store, model_uid).await?;
    let effective_config = req.config_overrides.as_ref().or(spec.config.as_ref());
    ensure_engine_config(spec.engine_type.as_deref(), effective_config)?;
    validate_callback_url(req.callback_url.as_deref())?;

    let replicas = effective_replicas(&req);
    validate_start_replica_specs(replicas, &req.replica_specs)?;

    let image_id = req.image_id.clone().or_else(|| spec.docker_image.clone());
    let _ = &image_id; // used by the fresh-deployment branch below
    let now = now_ms();
    let deployment = match get_model_deployment(store, model_uid).await? {
        Some(mut dep) => {
            dep.desired_state = DesiredState::Running;
            dep.replicas = replicas;
            if req.min_replicas.is_some() {
                dep.min_replicas = req.min_replicas;
            }
            if req.max_replicas.is_some() {
                dep.max_replicas = req.max_replicas;
            }
            if req.config_overrides.is_some() {
                dep.config_overrides = req.config_overrides;
            }
            if req.node_id.is_some() {
                dep.node_affinity = req.node_id;
            }
            if req.gpu_indices.is_some() {
                dep.gpu_affinity = req.gpu_indices;
            }
            let switching_to_replicated =
                req.topology == Some(nebula_common::DeploymentTopology::Replicated);
            if req.replica_specs.is_some() {
                dep.replica_specs = req.replica_specs;
            } else if switching_to_replicated {
                if let Some(specs) = dep.replica_specs.as_mut() {
                    for s in specs.iter_mut() {
                        s.role = None;
                    }
                    if specs.iter().all(|s| {
                        s.node_id.is_none()
                            && s.gpu_indices.is_none()
                            && s.config_overrides.is_none()
                            && s.image_id.is_none()
                            && s.role.is_none()
                    }) {
                        dep.replica_specs = None;
                    }
                }
            }
            if req.image_id.is_some() {
                dep.image_id = req.image_id;
            } else if dep.image_id.is_none() {
                dep.image_id = image_id.clone();
            }
            if req.image_override_reason.is_some() {
                dep.image_override_reason = req.image_override_reason;
            }
            if req.topology.is_some() {
                dep.topology = req.topology;
            }
            if req.kv_sharing.is_some() {
                dep.kv_sharing = req.kv_sharing;
            } else if switching_to_replicated {
                dep.kv_sharing = None;
            }
            dep.compat_rule_ids = Vec::new();
            dep.version += 1;
            dep.updated_at_ms = now;
            dep
        }
        None => ModelDeployment {
            model_uid: model_uid.to_string(),
            desired_state: DesiredState::Running,
            replicas,
            min_replicas: req.min_replicas,
            max_replicas: req.max_replicas,
            node_affinity: req.node_id,
            gpu_affinity: req.gpu_indices,
            replica_specs: req.replica_specs,
            config_overrides: req.config_overrides,
            image_id,
            allowed_pools: None,
            image_override_reason: req.image_override_reason,
            compat_rule_ids: Vec::new(),
            topology: req.topology,
            kv_sharing: req.kv_sharing,
            version: 1,
            updated_at_ms: now,
        },
    };

    let replica_count = deployment.replicas.max(1) as usize;
    let mut replica_platforms = Vec::with_capacity(replica_count);
    for idx in 0..replica_count {
        let rep_cfg =
            nebula_common::effective_replica_config(&deployment, spec.config.as_ref(), idx);
        ensure_engine_config(spec.engine_type.as_deref(), rep_cfg.as_ref())?;

        let raw_image =
            nebula_common::effective_replica_image(&deployment, spec.docker_image.as_deref(), idx);
        let rep_node = deployment
            .replica_specs
            .as_ref()
            .and_then(|s| s.get(idx))
            .and_then(|r| r.node_id.as_deref())
            .or(deployment.node_affinity.as_deref());
        let plat = resolve_replica_platform(store, raw_image, rep_node).await?;
        replica_platforms.push(plat);
    }

    // Structural + KV-compat validation (shared control logic, pool.md §pd_split v1.1)
    // runs BEFORE environment/compat checks: an invalid topology should be
    // rejected with its own actionable reason regardless of what the cluster
    // currently looks like. This also keeps the rejection deterministic.
    nebula_common::validate_pd_split_with_context(
        &deployment,
        spec.engine_type.as_deref(),
        spec.config.as_ref(),
        &replica_platforms,
    )
    .map_err(ServiceError::BadRequest)?;

    // Gate-passed artifact allowlist (review #4). Enforced on every replica's
    // resolved concrete runtime image so replica_specs[i].image_id cannot
    // bypass the top-level allowlist check.
    validate_pd_split_deployment_images(store, &deployment, spec.docker_image.as_deref()).await?;

    let mut deployment = deployment;
    deployment.compat_rule_ids = validate_compat_for_replica_specs(
        store,
        spec.engine_type.as_deref().unwrap_or("vllm"),
        deployment.image_id.as_deref(),
        spec.docker_image.as_deref(),
        deployment.image_override_reason.as_deref(),
        &deployment.replica_specs,
        deployment.node_affinity.as_deref(),
    )
    .await?;

    put_model_deployment(store, model_uid, &deployment).await?;
    Ok(deployment)
}

/// Validate that every replica of a `pd_split` deployment resolves to a
/// concrete runtime image on `NEBULA_APPROVED_PD_IMAGES`. Reused by deployment
/// admission (`start_model`) and by canary creation/promotion/rollback so
/// image mutations cannot bypass the allowlist.
pub async fn validate_pd_split_deployment_images(
    store: &dyn MetaStore,
    deployment: &ModelDeployment,
    spec_docker_image: Option<&str>,
) -> Result<(), ServiceError> {
    if deployment.topology != Some(nebula_common::DeploymentTopology::PdSplit) {
        return Ok(());
    }
    let approved = nebula_common::approved_pd_images_from_env();
    let replica_count = deployment.replicas.max(1) as usize;
    for idx in 0..replica_count {
        let raw_image = nebula_common::effective_replica_image(deployment, spec_docker_image, idx);
        let concrete_image = resolve_concrete_image_ref(store, raw_image).await?;
        nebula_common::validate_pd_split_image(concrete_image.as_deref(), &approved)
            .map_err(ServiceError::BadRequest)?;
    }
    Ok(())
}

pub async fn stop_model(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<ModelDeployment, ServiceError> {
    get_model_spec(store, model_uid).await?;

    let now = now_ms();
    let deployment = match get_model_deployment(store, model_uid).await? {
        Some(mut dep) => {
            dep.desired_state = DesiredState::Stopped;
            dep.version += 1;
            dep.updated_at_ms = now;
            dep
        }
        None => ModelDeployment {
            model_uid: model_uid.to_string(),
            desired_state: DesiredState::Stopped,
            replicas: 0,
            min_replicas: None,
            max_replicas: None,
            node_affinity: None,
            gpu_affinity: None,
            replica_specs: None,
            config_overrides: None,
            image_id: None,
            allowed_pools: None,
            image_override_reason: None,
            compat_rule_ids: vec![],
            topology: None,
            kv_sharing: None,
            version: 1,
            updated_at_ms: now,
        },
    };

    put_model_deployment(store, model_uid, &deployment).await?;
    Ok(deployment)
}

pub async fn scale_model(
    store: &dyn MetaStore,
    model_uid: &str,
    req: ScaleDeploymentRequest,
) -> Result<ModelDeployment, ServiceError> {
    validate_callback_url(req.callback_url.as_deref())?;

    let mut dep = match get_model_deployment(store, model_uid).await? {
        Some(d) => d,
        None => {
            return Err(ServiceError::NotFound(
                "deployment not found (model may not be started)".to_string(),
            ))
        }
    };

    let replicas = req.replicas.max(1);
    if let Some(specs) = dep.replica_specs.as_ref() {
        validate_replica_specs(replicas, specs).map_err(ServiceError::BadRequest)?;
    }
    // pd_split v1: scaling must stay role-complete (see pool.md — a resize
    // that drops the last prefill or decode replica is rejected).
    if dep.topology == Some(nebula_common::DeploymentTopology::PdSplit)
        && deps_pd_split_scale_invalid(&dep, replicas)
    {
        return Err(ServiceError::BadRequest(
            "pd_split scale would leave an incomplete role set (need >=1 prefill and >=1 decode); \
             supply a full replica_specs with explicit roles"
                .to_string(),
        ));
    }

    dep.replicas = replicas;
    dep.desired_state = DesiredState::Running;
    dep.version += 1;
    dep.updated_at_ms = now_ms();

    put_model_deployment(store, model_uid, &dep).await?;
    Ok(dep)
}

/// Extract callback URL from a start/load request (not persisted on deployment).
pub fn callback_url_from_start(req: &StartDeploymentRequest) -> Option<String> {
    req.callback_url.clone()
}

pub fn callback_url_from_scale(req: &ScaleDeploymentRequest) -> Option<String> {
    req.callback_url.clone()
}

#[cfg(test)]
mod pd_split_tests {
    use super::*;
    use nebula_common::{DeploymentTopology, KvSharingSpec, ReplicaRole};
    use nebula_meta::memory::MemoryMetaStore;
    use nebula_meta::MetaStore;

    fn spec_for(uid: &str) -> nebula_common::ModelSpec {
        serde_json::from_value(serde_json::json!({
            "model_uid": uid,
            "model_name": "Qwen2.5-7B-Instruct",
            "model_source": "local",
            "model_path": "/models/q",
            "engine_type": "vllm",
            "docker_image": "nebula/vllm:0.23.0-lmcache0.5.5",
            "config": { "served_model_name": "m1" }
        }))
        .unwrap()
    }

    fn req(topology: Option<DeploymentTopology>, kv: bool, roles: bool) -> StartDeploymentRequest {
        StartDeploymentRequest {
            replicas: Some(2),
            min_replicas: None,
            max_replicas: None,
            config_overrides: None,
            node_id: None,
            gpu_indices: None,
            replica_specs: roles.then(|| {
                vec![
                    ReplicaPlacementSpec {
                        role: Some(ReplicaRole::Prefill),
                        ..Default::default()
                    },
                    ReplicaPlacementSpec {
                        role: Some(ReplicaRole::Decode),
                        ..Default::default()
                    },
                ]
            }),
            image_id: None,
            image_override_reason: None,
            callback_url: None,
            topology,
            kv_sharing: kv.then(|| KvSharingSpec {
                remote_url: "lm://store:6800".into(),
                serde: "naive".into(),
            }),
        }
    }

    /// The deployment API must reject an invalid pd_split instead of
    /// persisting it and letting the controller create unusable pods.
    #[tokio::test]
    async fn start_model_rejects_invalid_pd_split() {
        // Gate-passed artifact allowlist (review #4) — the valid case needs
        // the spec's image on it.
        std::env::set_var(
            "NEBULA_APPROVED_PD_IMAGES",
            "nebula/vllm:0.23.0-lmcache0.5.5",
        );
        let store = MemoryMetaStore::new();
        for uid in ["m1", "m2"] {
            store
                .put(
                    &format!("/models/{uid}/spec"),
                    serde_json::to_vec(&spec_for(uid)).unwrap(),
                    None,
                )
                .await
                .unwrap();
        }

        // pd_split without kv_sharing → rejected.
        let err = start_model(
            &store,
            "m1",
            req(Some(DeploymentTopology::PdSplit), false, true),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ServiceError::BadRequest(_)), "{err:?}");

        // pd_split without roles → rejected.
        let err = start_model(
            &store,
            "m1",
            req(Some(DeploymentTopology::PdSplit), true, false),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ServiceError::BadRequest(_)), "{err:?}");

        // Valid pd_split passes STRUCTURAL validation. It may still be
        // rejected by environment/compat rules (this test store has no node
        // inventory), so assert the failure — if any — is not structural.
        match start_model(
            &store,
            "m1",
            req(Some(DeploymentTopology::PdSplit), true, true),
        )
        .await
        {
            Ok(dep) => {
                assert_eq!(dep.topology, Some(DeploymentTopology::PdSplit));
                assert_eq!(
                    dep.kv_sharing.as_ref().unwrap().remote_url,
                    "lm://store:6800"
                );
            }
            Err(ServiceError::BadRequest(m)) => {
                assert!(
                    !m.contains("pd_split") && !m.contains("kv_sharing") && !m.contains("role"),
                    "valid pd_split was rejected structurally: {m}"
                );
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }

        // An image off the allowlist is rejected even when structurally valid.
        std::env::set_var("NEBULA_APPROVED_PD_IMAGES", "other/img:1");
        let err = start_model(
            &store,
            "m1",
            req(Some(DeploymentTopology::PdSplit), true, true),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&err, ServiceError::BadRequest(m) if m.contains("approved list")),
            "{err:?}"
        );
        std::env::set_var(
            "NEBULA_APPROVED_PD_IMAGES",
            "nebula/vllm:0.23.0-lmcache0.5.5",
        );

        // Per-replica image override in replica_specs cannot bypass allowlist.
        let mut r_override = req(Some(DeploymentTopology::PdSplit), true, true);
        r_override.replica_specs.as_mut().unwrap()[1].image_id =
            Some("unapproved/vllm:rogue".into());
        let err = start_model(&store, "m1", r_override).await.unwrap_err();
        assert!(
            matches!(&err, ServiceError::BadRequest(m) if m.contains("approved list")),
            "per-replica image override bypassed allowlist: {err:?}"
        );

        // Spec-level TP>1 is rejected for pd_split.
        let mut spec_tp2 = spec_for("m3");
        spec_tp2.config = Some(ModelConfig {
            tensor_parallel_size: Some(2),
            ..Default::default()
        });
        store
            .put(
                "/models/m3/spec",
                serde_json::to_vec(&spec_tp2).unwrap(),
                None,
            )
            .await
            .unwrap();
        let err = start_model(
            &store,
            "m3",
            req(Some(DeploymentTopology::PdSplit), true, true),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(&err, ServiceError::BadRequest(m) if m.contains("tensor_parallel_size=1")),
            "spec TP>1 was not rejected: {err:?}"
        );

        // Node status stored at canonical `/nodes/{id}/status` with default
        // platform "nvidia-cuda" is accepted, whereas "huawei-ascend" is rejected.
        let cuda_node = nebula_common::NodeStatus {
            node_id: "node-cuda".into(),
            last_heartbeat_ms: 1,
            gpus: vec![],
            api_addr: None,
            platform: Some(nebula_common::DEFAULT_NODE_PLATFORM.into()),
        };
        store
            .put(
                "/nodes/node-cuda/status",
                serde_json::to_vec(&cuda_node).unwrap(),
                None,
            )
            .await
            .unwrap();
        let mut r_cuda = req(Some(DeploymentTopology::PdSplit), true, true);
        r_cuda.node_id = Some("node-cuda".into());
        start_model(&store, "m1", r_cuda)
            .await
            .expect("pd_split on /nodes/node-cuda/status with nvidia-cuda must succeed");

        let ascend_node = nebula_common::NodeStatus {
            node_id: "node-ascend".into(),
            platform: Some("huawei-ascend".into()),
            ..cuda_node
        };
        store
            .put(
                "/nodes/node-ascend/status",
                serde_json::to_vec(&ascend_node).unwrap(),
                None,
            )
            .await
            .unwrap();
        let mut r_ascend = req(Some(DeploymentTopology::PdSplit), true, true);
        r_ascend.node_id = Some("node-ascend".into());
        let err = start_model(&store, "m1", r_ascend).await.unwrap_err();
        assert!(
            matches!(&err, ServiceError::BadRequest(m) if m.contains("CUDA")),
            "expected non-CUDA node under /nodes/{{id}}/status to be rejected, got: {err:?}"
        );

        // Switching an existing pd_split deployment (m1) back to replicated
        // clears stale kv_sharing and replica roles so validation passes.
        let dep = start_model(
            &store,
            "m1",
            req(Some(DeploymentTopology::Replicated), false, false),
        )
        .await
        .expect("switching back to replicated should clear pd_split fields and succeed");
        assert_eq!(dep.topology, Some(DeploymentTopology::Replicated));
        assert!(dep.kv_sharing.is_none());
        assert!(dep.replica_specs.is_none());

        // A fresh model started without topology is replicated and never hits
        // pd_split rules.
        match start_model(&store, "m2", req(None, false, false)).await {
            Ok(dep) => assert_eq!(dep.topology, None),
            Err(ServiceError::BadRequest(m)) => {
                assert!(
                    !m.contains("pd_split"),
                    "replicated hit pd_split rules: {m}"
                )
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
}
