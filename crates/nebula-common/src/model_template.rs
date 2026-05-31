use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model_request::ModelConfig;
use crate::model_spec::ModelSource;

/// Origin of a template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TemplateSource {
    /// Built-in preset.
    System,
    /// User-created.
    User,
    /// Saved from a running model.
    Saved,
}

/// Model category for frontend grouping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TemplateCategory {
    Llm,
    Embedding,
    Rerank,
    Vlm,
    Audio,
}

/// Reusable model configuration template for one-click deployment.
///
/// Stored in etcd under `/templates/{template_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelTemplate {
    /// Globally unique template identifier (user-specified or auto-generated).
    pub template_id: String,

    /// Human-readable template name.
    pub name: String,

    /// Description of the template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Model category for frontend grouping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<TemplateCategory>,

    /// Model name / HuggingFace ID.
    pub model_name: String,

    /// Model file source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_source: Option<ModelSource>,

    /// Engine type: "vllm", "sglang", etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_type: Option<String>,

    /// Docker image override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_image: Option<String>,

    /// Default inference configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ModelConfig>,

    /// Default number of replicas when deploying from this template.
    #[serde(default = "default_replicas")]
    pub default_replicas: u32,

    /// Free-form key-value labels.
    #[serde(default)]
    pub labels: HashMap<String, String>,

    /// Template origin.
    pub source: TemplateSource,

    /// Creation timestamp (ms since epoch).
    #[serde(default)]
    pub created_at_ms: u64,

    /// Last update timestamp (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_replicas() -> u32 {
    1
}
