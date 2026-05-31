use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model_request::ModelConfig;

/// Source of model files — determines how the Node downloads the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    HuggingFace,
    ModelScope,
    Local,
}

/// Persistent model identity — the model's "ID card".
///
/// Once created, persists until the user explicitly deletes it.
/// Independent of runtime state.
///
/// Stored in etcd under `/models/{model_uid}/spec`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Globally unique identifier (e.g. "qwen2-5-7b-instruct").
    /// Format: `[a-z0-9][a-z0-9-]*`, max 63 chars. Immutable after creation.
    pub model_uid: String,

    /// User-readable model path (e.g. HuggingFace ID "Qwen/Qwen2.5-7B-Instruct").
    pub model_name: String,

    /// Model file source — determines download strategy on Node.
    pub model_source: ModelSource,

    /// For `Local` source: absolute path to model files on the node
    /// (e.g. "/DATA/Model/Qwen2.5-7B-Instruct/").
    /// For `HuggingFace`/`ModelScope` sources: optional override for download/cache root directory.
    /// If not set, node default model directory is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,

    /// Engine type: "vllm", "sglang", etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_type: Option<String>,

    /// Docker image override for the engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_image: Option<String>,

    /// Default inference configuration — serves as fallback for Deployment overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ModelConfig>,

    /// Free-form key-value labels for grouping and filtering.
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Creation timestamp (ms since epoch).
    #[serde(default)]
    pub created_at_ms: u64,

    /// Last update timestamp (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,

    /// User who created this model spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}
