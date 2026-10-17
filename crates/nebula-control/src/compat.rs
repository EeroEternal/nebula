use nebula_common::{
    default_compatibility_rules, evaluate_compatibility, CompatCheckInput, CompatibilityRule,
    EngineImage, NodeStatus,
};
use nebula_meta::MetaStore;

use crate::error::ServiceError;
use crate::store::now_ms;

pub async fn list_compat_rules(
    store: &dyn MetaStore,
) -> Result<Vec<CompatibilityRule>, ServiceError> {
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

/// Resolve an image_id or docker_image string to the concrete container image
/// reference (`EngineImage.image` when registered in `/images/`, else the
/// raw reference itself).
pub async fn resolve_concrete_image_ref(
    store: &dyn MetaStore,
    image_ref: Option<&str>,
) -> Result<Option<String>, ServiceError> {
    let Some(raw) = image_ref.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if let Some(img) = load_image(store, Some(raw), None).await? {
        return Ok(Some(img.image));
    }
    Ok(Some(raw.to_string()))
}

async fn load_node_status(
    store: &dyn MetaStore,
    node_id: &str,
) -> Result<Option<NodeStatus>, ServiceError> {
    let status_key = format!("/nodes/{node_id}/status");
    if let Some((data, _)) = store.get(&status_key).await? {
        if let Ok(node) = serde_json::from_slice::<NodeStatus>(&data) {
            return Ok(Some(node));
        }
    }
    let legacy_key = format!("/nodes/{node_id}");
    if let Some((data, _)) = store.get(&legacy_key).await? {
        if let Ok(node) = serde_json::from_slice::<NodeStatus>(&data) {
            return Ok(Some(node));
        }
    }
    Ok(None)
}

/// Resolve the target platform string for a replica from its target node (when
/// known in `/nodes/{id}/status`) or its registered `EngineImage` platforms.
pub async fn resolve_replica_platform(
    store: &dyn MetaStore,
    image_ref: Option<&str>,
    node_id: Option<&str>,
) -> Result<Option<String>, ServiceError> {
    if let Some(nid) = node_id {
        if let Some(node) = load_node_status(store, nid).await? {
            return Ok(Some(nebula_common::resolve_node_platform(&node)));
        }
    }
    if let Some(img) = load_image(store, image_ref, None).await? {
        if let Some(p) = img
            .platforms
            .iter()
            .find(|p| !p.eq_ignore_ascii_case("any") && !p.trim().is_empty())
        {
            return Ok(Some(p.clone()));
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
        load_node_status(store, nid).await?
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
            if override_reason
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_some()
                && reason.code == "compat_denied"
            {
                tracing::warn!(
                    override_reason = ?override_reason,
                    rule = ?reason.rule_id,
                    "compat deny overridden by operator"
                );
                return Ok(vec![format!(
                    "override:{}",
                    reason.rule_id.unwrap_or_default()
                )]);
            }
            Err(ServiceError::BadRequest(reason.format_error()))
        }
    }
}
