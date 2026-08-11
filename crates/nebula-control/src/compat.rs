use nebula_common::{
    default_compatibility_rules, evaluate_compatibility, CompatCheckInput, CompatibilityRule,
    EngineImage, NodeStatus,
};
use nebula_meta::MetaStore;

use crate::error::ServiceError;
use crate::store::now_ms;

pub async fn list_compat_rules(store: &dyn MetaStore) -> Result<Vec<CompatibilityRule>, ServiceError> {
    let entries = store.list_prefix("/compat/").await?;
    let mut out: Vec<CompatibilityRule> = entries
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice(&v).ok())
        .collect();
    if out.is_empty() {
        out = default_compatibility_rules(now_ms());
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

async fn load_image(
    store: &dyn MetaStore,
    image_id: Option<&str>,
    docker_image: Option<&str>,
) -> Result<Option<EngineImage>, ServiceError> {
    let wanted = image_id
        .or(docker_image)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(wanted) = wanted else {
        return Ok(None);
    };
    let entries = store.list_prefix("/images/").await?;
    for (_, v, _) in entries {
        let Ok(img) = serde_json::from_slice::<EngineImage>(&v) else {
            continue;
        };
        if img.id == wanted || img.image == wanted {
            return Ok(Some(img));
        }
    }
    Ok(None)
}

/// Validate deploy intent against compatibility matrix (+ optional target node).
pub async fn validate_deploy_compat(
    store: &dyn MetaStore,
    engine_type: &str,
    engine_version: Option<&str>,
    image_id: Option<&str>,
    docker_image: Option<&str>,
    node_id: Option<&str>,
    override_reason: Option<&str>,
) -> Result<Vec<String>, ServiceError> {
    let rules = list_compat_rules(store).await?;
    let image = load_image(store, image_id, docker_image).await?;
    let platforms = image
        .as_ref()
        .map(|i| i.platforms.clone())
        .unwrap_or_default();

    let node = if let Some(nid) = node_id {
        store
            .get(&format!("/nodes/{nid}"))
            .await?
            .and_then(|(data, _)| serde_json::from_slice::<NodeStatus>(&data).ok())
    } else {
        None
    };

    let input = CompatCheckInput {
        engine_type,
        engine_version,
        platforms: &platforms,
        node: node.as_ref(),
        image_id: image_id.or(docker_image),
    };

    match evaluate_compatibility(&rules, &input) {
        Ok(ids) => Ok(ids),
        Err(reason) => {
            if override_reason.map(str::trim).filter(|s| !s.is_empty()).is_some()
                && reason.code == "compat_denied"
            {
                tracing::warn!(
                    override_reason = ?override_reason,
                    rule = ?reason.rule_id,
                    "compat deny overridden by operator"
                );
                return Ok(vec![format!("override:{}", reason.rule_id.unwrap_or_default())]);
            }
            Err(ServiceError::BadRequest(reason.format_error()))
        }
    }
}
