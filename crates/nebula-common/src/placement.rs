use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlacementAssignment {
    pub replica_id: u32,
    pub node_id: String,
    pub engine_config_path: String,
    pub port: u16,

    /// Legacy single-GPU field (kept for backward compatibility with existing etcd data)
    #[serde(default)]
    pub gpu_index: Option<u32>,

    /// Multi-GPU indices for tensor-parallel deployment
    #[serde(default)]
    pub gpu_indices: Option<Vec<u32>>,

    #[serde(default)]
    pub extra_args: Option<Vec<String>>,

    /// Engine type: "vllm", "sglang", etc. Defaults to "vllm" if absent.
    #[serde(default)]
    pub engine_type: Option<String>,

    /// Override docker image for this assignment. If set, takes precedence over
    /// the node-level engine docker_image CLI arg.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_image: Option<String>,
}

impl PlacementAssignment {
    /// Resolve effective GPU indices: prefer gpu_indices, fall back to gpu_index
    pub fn effective_gpu_indices(&self) -> Option<Vec<u32>> {
        if let Some(indices) = &self.gpu_indices {
            if !indices.is_empty() {
                return Some(indices.clone());
            }
        }
        self.gpu_index.map(|i| vec![i])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlacementPlan {
    #[serde(default)]
    pub request_id: Option<String>,
    pub model_uid: String,
    pub model_name: String,
    /// Logical monotonic version. Bumped as `old.version + 1` on each CAS write.
    /// Do not use wall-clock timestamps here.
    pub version: u64,
    /// Wall-clock timestamp of the last successful write (observability / grace / cooldown).
    #[serde(default)]
    pub updated_at_ms: u64,
    /// Fencing token from scheduler leader election. Nodes reject plans with a
    /// lower epoch than the last applied plan for the same model.
    #[serde(default)]
    pub leader_epoch: u64,
    pub assignments: Vec<PlacementAssignment>,
}

impl PlacementPlan {
    /// Effective write time for grace/cooldown. Falls back to legacy `version`
    /// when it looks like a millisecond timestamp (pre-0.2.0 plans).
    pub fn effective_updated_at_ms(&self) -> u64 {
        if self.updated_at_ms > 0 {
            self.updated_at_ms
        } else if self.version > 1_000_000_000_000 {
            self.version
        } else {
            0
        }
    }
}

/// Next logical placement version after a successful CAS read of `prev`.
/// Never use wall-clock here (B4).
pub fn next_placement_version(prev: u64) -> u64 {
    prev.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_updated_at_prefers_explicit_field() {
        let plan = PlacementPlan {
            request_id: None,
            model_uid: "m".into(),
            model_name: "m".into(),
            version: 3,
            updated_at_ms: 42,
            leader_epoch: 0,
            assignments: vec![],
        };
        assert_eq!(plan.effective_updated_at_ms(), 42);
    }

    #[test]
    fn effective_updated_at_falls_back_to_legacy_timestamp_version() {
        let plan = PlacementPlan {
            request_id: None,
            model_uid: "m".into(),
            model_name: "m".into(),
            version: 1_700_000_000_000,
            updated_at_ms: 0,
            leader_epoch: 0,
            assignments: vec![],
        };
        assert_eq!(plan.effective_updated_at_ms(), 1_700_000_000_000);
    }

    #[test]
    fn missing_updated_at_deserializes_default() {
        let json = r#"{"model_uid":"m","model_name":"n","version":2,"assignments":[]}"#;
        let plan: PlacementPlan = serde_json::from_str(json).unwrap();
        assert_eq!(plan.version, 2);
        assert_eq!(plan.updated_at_ms, 0);
    }

    #[test]
    fn next_placement_version_is_strictly_monotonic() {
        assert_eq!(next_placement_version(0), 1);
        assert_eq!(next_placement_version(1), 2);
        // Same-ms wall clock would collide; logical bump does not.
        let same_ms = 1_700_000_000_000u64;
        assert_eq!(next_placement_version(same_ms), same_ms + 1);
        assert_eq!(
            next_placement_version(next_placement_version(same_ms)),
            same_ms + 2
        );
    }
}
