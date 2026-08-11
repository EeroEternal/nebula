use std::collections::HashMap;

use nebula_common::{ModelConfig, ModelSource, ModelSpec};
use nebula_meta::MetaStore;
use serde::Deserialize;

use crate::error::ServiceError;
use crate::store::{
    get_model_spec, is_valid_model_uid, now_ms, put_model_spec,
};

#[derive(Debug, Clone, Deserialize)]
pub struct CreateModelRequest {
    pub model_uid: String,
    pub model_name: String,
    #[serde(default)]
    pub model_source: Option<ModelSource>,
    #[serde(default)]
    pub model_path: Option<String>,
    #[serde(default)]
    pub engine_type: Option<String>,
    #[serde(default)]
    pub docker_image: Option<String>,
    #[serde(default)]
    pub config: Option<ModelConfig>,
    #[serde(default)]
    pub labels: Option<HashMap<String, String>>,
}

pub async fn list_models(store: &dyn MetaStore) -> Result<Vec<ModelSpec>, ServiceError> {
    let raw = store.list_prefix("/models/").await?;
    let mut specs = Vec::new();
    for (key, value, _) in raw {
        if !key.ends_with("/spec") {
            continue;
        }
        if let Ok(spec) = serde_json::from_slice::<ModelSpec>(&value) {
            specs.push(spec);
        }
    }
    specs.sort_by(|a, b| a.model_uid.cmp(&b.model_uid));
    Ok(specs)
}

pub async fn create_model(
    store: &dyn MetaStore,
    principal: &str,
    req: CreateModelRequest,
) -> Result<ModelSpec, ServiceError> {
    if !is_valid_model_uid(&req.model_uid) {
        return Err(ServiceError::BadRequest(
            "model_uid must match [a-z0-9][a-z0-9-]* and be at most 63 chars".to_string(),
        ));
    }

    if store
        .get(&format!("/models/{}/spec", req.model_uid))
        .await?
        .is_some()
    {
        return Err(ServiceError::Conflict(format!(
            "model '{}' already exists",
            req.model_uid
        )));
    }

    nebula_common::validate_engine_and_config(req.engine_type.as_deref(), req.config.as_ref())
        .map_err(ServiceError::BadRequest)?;

    let now = now_ms();
    let spec = ModelSpec {
        model_uid: req.model_uid.clone(),
        model_name: req.model_name,
        model_source: req
            .model_source
            .unwrap_or(ModelSource::HuggingFace),
        model_path: req.model_path,
        engine_type: req.engine_type,
        docker_image: req.docker_image,
        config: req.config,
        labels: req.labels.unwrap_or_default(),
        created_at_ms: now,
        updated_at_ms: now,
        created_by: Some(principal.to_string()),
    };

    put_model_spec(store, &req.model_uid, &spec).await?;
    Ok(spec)
}

pub async fn get_model(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<ModelSpec, ServiceError> {
    get_model_spec(store, model_uid).await
}
