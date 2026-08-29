use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Role/purpose of a hardware pool.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PoolRole {
    General,
    Prefill,
    Decode,
    Edge,
}

impl Default for PoolRole {
    fn default() -> Self {
        Self::General
    }
}

/// A logical resource pool grouping physical nodes.
/// Stored in etcd under `/pools/{pool_id}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwarePool {
    /// Unique pool identifier, e.g. "pool-prod-a800", "pool-dev-4090".
    pub pool_id: String,

    /// Human-friendly display name.
    pub display_name: String,

    /// Optional platform constraint (e.g. "nvidia-cuda", "ascend-npu").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    /// Pool role/function.
    #[serde(default)]
    pub role: PoolRole,

    /// Explicit list of physical node IDs assigned to this pool.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_ids: Vec<String>,

    /// Optional pool labels for grouping/filtering.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,

    /// Whether this pool is eligible to receive new placement assignments.
    /// If false, Scheduler will skip nodes in this pool for new replicas.
    #[serde(default = "default_schedulable")]
    pub schedulable: bool,

    /// Timestamp of last update (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_schedulable() -> bool {
    true
}

impl HardwarePool {
    pub fn contains_node(&self, node_id: &str) -> bool {
        self.node_ids.iter().any(|id| id == node_id)
    }
}
