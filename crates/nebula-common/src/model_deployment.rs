use serde::{Deserialize, Serialize};

use crate::model_request::ModelConfig;

/// Desired runtime state for a model deployment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    Running,
    Stopped,
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

    /// Overrides for ModelSpec.config fields (merge semantics — only specified fields override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_overrides: Option<ModelConfig>,

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
