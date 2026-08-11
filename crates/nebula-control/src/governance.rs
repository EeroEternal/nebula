//! Read-only governance helpers for `/platform/v1` (I4).

use nebula_common::{CanaryRelease, ModelSlo};
use nebula_meta::MetaStore;

use crate::error::ServiceError;

fn slo_key(model_uid: &str) -> String {
    format!("/slos/{model_uid}")
}

fn canary_key(id: &str) -> String {
    format!("/canaries/{id}")
}

pub async fn get_slo(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<Option<ModelSlo>, ServiceError> {
    match store.get(&slo_key(model_uid)).await? {
        Some((data, _)) => Ok(Some(serde_json::from_slice(&data)?)),
        None => Ok(None),
    }
}

pub async fn list_slos(store: &dyn MetaStore) -> Result<Vec<ModelSlo>, ServiceError> {
    let entries = store.list_prefix("/slos/").await?;
    let mut out: Vec<ModelSlo> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| a.model_uid.cmp(&b.model_uid));
    Ok(out)
}

pub async fn list_canaries(store: &dyn MetaStore) -> Result<Vec<CanaryRelease>, ServiceError> {
    let entries = store.list_prefix("/canaries/").await?;
    let mut out: Vec<CanaryRelease> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    out.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    Ok(out)
}

pub async fn get_canary(
    store: &dyn MetaStore,
    canary_id: &str,
) -> Result<CanaryRelease, ServiceError> {
    match store.get(&canary_key(canary_id)).await? {
        Some((data, _)) => Ok(serde_json::from_slice(&data)?),
        None => Err(ServiceError::NotFound(format!(
            "canary '{canary_id}' not found"
        ))),
    }
}

pub fn filter_canaries_by_model(
    canaries: Vec<CanaryRelease>,
    model_uid: Option<&str>,
) -> Vec<CanaryRelease> {
    match model_uid {
        Some(uid) => canaries
            .into_iter()
            .filter(|c| c.model_uid == uid)
            .collect(),
        None => canaries,
    }
}
