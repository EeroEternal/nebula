use nebula_common::{
    validate_engine_and_config, DesiredState, ModelConfig, ModelDeployment, ModelLoadRequest,
    ModelSpec,
};
use nebula_meta::MetaStore;
use serde::Deserialize;

use crate::compat::validate_deploy_compat;
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
    pub image_id: Option<String>,
    #[serde(default)]
    pub image_override_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScaleDeploymentRequest {
    pub replicas: u32,
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

/// Upsert model spec + apply running deployment (compat-validated).
pub async fn load_model(
    store: &dyn MetaStore,
    principal: &str,
    req: ModelLoadRequest,
) -> Result<ModelDeployment, ServiceError> {
    if !is_valid_model_uid(&req.model_uid) {
        return Err(ServiceError::BadRequest(
            "model_uid must match [a-z0-9][a-z0-9-]* and be at most 63 chars".to_string(),
        ));
    }

    ensure_engine_config(req.engine_type.as_deref(), req.config.as_ref())?;

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

    let start = StartDeploymentRequest {
        replicas: Some(req.replicas.max(1)),
        min_replicas: req.min_replicas,
        max_replicas: req.max_replicas,
        config_overrides: req.config.clone(),
        node_id: req.node_id.clone(),
        gpu_indices: gpu_affinity_from_load(&req),
        image_id: None,
        image_override_reason: None,
    };
    start_model(store, &model_uid, start).await
}

pub async fn start_model(
    store: &dyn MetaStore,
    model_uid: &str,
    req: StartDeploymentRequest,
) -> Result<ModelDeployment, ServiceError> {
    let spec = get_model_spec(store, model_uid).await?;
    let effective_config = req
        .config_overrides
        .as_ref()
        .or(spec.config.as_ref());
    ensure_engine_config(spec.engine_type.as_deref(), effective_config)?;

    let image_id = req
        .image_id
        .clone()
        .or_else(|| spec.docker_image.clone());
    let compat_ids = validate_deploy_compat(
        store,
        spec.engine_type.as_deref().unwrap_or("vllm"),
        None,
        image_id.as_deref(),
        spec.docker_image.as_deref(),
        req.node_id.as_deref(),
        req.image_override_reason.as_deref(),
    )
    .await?;

    let now = now_ms();
    let deployment = match get_model_deployment(store, model_uid).await? {
        Some(mut dep) => {
            dep.desired_state = DesiredState::Running;
            if let Some(r) = req.replicas {
                dep.replicas = r.max(1);
            }
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
            if req.image_id.is_some() {
                dep.image_id = req.image_id;
            } else if dep.image_id.is_none() {
                dep.image_id = image_id.clone();
            }
            if req.image_override_reason.is_some() {
                dep.image_override_reason = req.image_override_reason;
            }
            dep.compat_rule_ids = compat_ids;
            dep.version += 1;
            dep.updated_at_ms = now;
            dep
        }
        None => ModelDeployment {
            model_uid: model_uid.to_string(),
            desired_state: DesiredState::Running,
            replicas: req.replicas.unwrap_or(1).max(1),
            min_replicas: req.min_replicas,
            max_replicas: req.max_replicas,
            node_affinity: req.node_id,
            gpu_affinity: req.gpu_indices,
            config_overrides: req.config_overrides,
            image_id,
            image_override_reason: req.image_override_reason,
            compat_rule_ids: compat_ids,
            version: 1,
            updated_at_ms: now,
        },
    };

    put_model_deployment(store, model_uid, &deployment).await?;
    Ok(deployment)
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
            config_overrides: None,
            image_id: None,
            image_override_reason: None,
            compat_rule_ids: vec![],
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
    let mut dep = match get_model_deployment(store, model_uid).await? {
        Some(d) => d,
        None => {
            return Err(ServiceError::NotFound(
                "deployment not found (model may not be started)".to_string(),
            ))
        }
    };

    dep.replicas = req.replicas.max(1);
    dep.desired_state = DesiredState::Running;
    dep.version += 1;
    dep.updated_at_ms = now_ms();

    put_model_deployment(store, model_uid, &dep).await?;
    Ok(dep)
}
