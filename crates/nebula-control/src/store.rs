use nebula_common::{ModelDeployment, ModelSpec};
use nebula_meta::MetaStore;

use crate::error::ServiceError;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub async fn get_model_spec(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<ModelSpec, ServiceError> {
    match store.get(&format!("/models/{model_uid}/spec")).await? {
        Some((data, _)) => serde_json::from_slice(&data).map_err(Into::into),
        None => Err(ServiceError::NotFound(format!("model '{model_uid}' not found"))),
    }
}

pub async fn get_model_deployment(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<Option<ModelDeployment>, ServiceError> {
    match store.get(&format!("/deployments/{model_uid}")).await? {
        Some((data, _)) => Ok(Some(serde_json::from_slice(&data)?)),
        None => Ok(None),
    }
}

pub async fn put_model_spec(
    store: &dyn MetaStore,
    model_uid: &str,
    spec: &ModelSpec,
) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(spec)?;
    store
        .put(&format!("/models/{model_uid}/spec"), val, None)
        .await?;
    Ok(())
}

pub async fn put_model_deployment(
    store: &dyn MetaStore,
    model_uid: &str,
    dep: &ModelDeployment,
) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(dep)?;
    store
        .put(&format!("/deployments/{model_uid}"), val, None)
        .await?;
    Ok(())
}

pub fn is_valid_model_uid(uid: &str) -> bool {
    if uid.is_empty() || uid.len() > 63 {
        return false;
    }
    let mut chars = uid.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c.is_ascii_digit() => {}
        _ => return false,
    }
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return false;
        }
    }
    true
}

pub fn infer_model_source(model_name: &str, model_path: Option<&str>) -> nebula_common::ModelSource {
    let pathish = model_path.unwrap_or(model_name);
    if pathish.starts_with('/') || pathish.starts_with('.') {
        nebula_common::ModelSource::Local
    } else {
        nebula_common::ModelSource::HuggingFace
    }
}
