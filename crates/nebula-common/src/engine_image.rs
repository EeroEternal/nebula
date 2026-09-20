use serde::{Deserialize, Serialize};

/// Version pinning strategy for engine images.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum VersionPolicy {
    /// Pin to a specific tag — never auto-update.
    #[default]
    Pin,
    /// Rolling — periodically re-pull to track latest digest for the tag.
    Rolling,
}

/// A registered engine image in the cluster image registry.
///
/// Stored in etcd under `/images/{id}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineImage {
    /// Unique identifier, e.g. "vllm-cuda124" or "sglang-ascend".
    pub id: String,

    /// Engine type: "vllm", "sglang", etc.
    pub engine_type: String,

    /// Full Docker image reference, e.g. "vllm/vllm-openai:v0.8.3".
    pub image: String,

    /// Compatible hardware platforms, e.g. ["nvidia-cuda", "ascend-cann8"].
    /// Empty means compatible with all platforms.
    #[serde(default)]
    pub platforms: Vec<String>,

    /// Version pinning policy.
    #[serde(default)]
    pub version_policy: VersionPolicy,

    /// Whether this image should be pre-pulled on matching nodes.
    #[serde(default = "default_true")]
    pub pre_pull: bool,

    /// Human-readable description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Creation timestamp (ms since epoch).
    #[serde(default)]
    pub created_at_ms: u64,

    /// Last update timestamp (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_true() -> bool {
    true
}

/// Status of an image pull operation on a specific node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImagePullStatus {
    Pending,
    Pulling,
    Ready,
    Failed,
}

/// Per-node image pull state, stored in etcd under `/image_status/{node_id}/{image_id}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeImageStatus {
    pub node_id: String,
    pub image_id: String,
    pub image: String,
    pub status: ImagePullStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
}
