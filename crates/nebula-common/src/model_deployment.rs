use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model_request::ModelConfig;

/// Desired runtime state for a model deployment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    Running,
    Stopped,
}

/// Per-replica placement desired state (subset of runtime `PlacementAssignment`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplicaPlacementSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_indices: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_overrides: Option<ModelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,
}

/// Declares how a model should run — similar to a K8s Deployment spec.
///
/// Stored in etcd under `/deployments/{model_uid}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDeployment {
    /// References the ModelSpec's model_uid.
    pub model_uid: String,

    /// User-writable desired state: `running` or `stopped`.
    pub desired_state: DesiredState,

    /// Number of desired replicas.
    #[serde(default = "default_replicas")]
    pub replicas: u32,

    /// Minimum replicas for autoscaling. None means use `replicas` as fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_replicas: Option<u32>,

    /// Maximum replicas for autoscaling. None means use `replicas` as fixed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_replicas: Option<u32>,

    /// Optional node affinity constraint. None means Scheduler decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_affinity: Option<String>,

    /// Optional GPU affinity constraint. None means Scheduler decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_affinity: Option<Vec<u32>>,

    /// Per-replica placement overrides; length must equal `replicas` when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replica_specs: Option<Vec<ReplicaPlacementSpec>>,

    /// Overrides for ModelSpec.config fields (merge semantics — only specified fields override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_overrides: Option<ModelConfig>,

    /// Pin to a registered `EngineImage.id` (preferred over free-form docker_image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_id: Option<String>,

    /// Optional pool affinity constraints. If non-empty, Scheduler only assigns to nodes in these pools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_pools: Option<Vec<String>>,

    /// Human reason when forcing an image / platform override past defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_override_reason: Option<String>,

    /// Compatibility rule ids applied when this deployment was last validated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compat_rule_ids: Vec<String>,

    /// Monotonically increasing version; bumped on every change.
    /// Scheduler uses this to detect whether a re-plan is needed.
    #[serde(default)]
    pub version: u64,

    /// Last update timestamp (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_replicas() -> u32 {
    1
}

/// Validate heterogeneous per-replica placement specs.
pub fn validate_replica_specs(replicas: u32, specs: &[ReplicaPlacementSpec]) -> Result<(), String> {
    if specs.len() != replicas as usize {
        return Err(format!(
            "replica_specs length {} must match replicas {replicas}",
            specs.len()
        ));
    }

    let mut used_gpus: HashMap<String, HashSet<u32>> = HashMap::new();
    for (idx, spec) in specs.iter().enumerate() {
        let Some(node) = spec.node_id.as_deref() else {
            continue;
        };
        let Some(gpus) = spec.gpu_indices.as_ref() else {
            continue;
        };
        let entry = used_gpus.entry(node.to_string()).or_default();
        for gpu in gpus {
            if !entry.insert(*gpu) {
                return Err(format!(
                    "replica_specs[{idx}] duplicates gpu {gpu} on node '{node}'"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_replica_specs_length_mismatch() {
        let err = validate_replica_specs(2, &[ReplicaPlacementSpec::default()]).unwrap_err();
        assert!(err.contains("must match replicas"));
    }

    #[test]
    fn validate_replica_specs_rejects_duplicate_gpu() {
        let specs = vec![
            ReplicaPlacementSpec {
                node_id: Some("n1".into()),
                gpu_indices: Some(vec![0, 1]),
                ..Default::default()
            },
            ReplicaPlacementSpec {
                node_id: Some("n1".into()),
                gpu_indices: Some(vec![1]),
                ..Default::default()
            },
        ];
        let err = validate_replica_specs(2, &specs).unwrap_err();
        assert!(err.contains("duplicates gpu 1"));
    }
}
