use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelLoadRequest {
    /// The user-facing model name (e.g., "Qwen/Qwen2.5-0.5B-Instruct")
    pub model_name: String,

    /// The internal unique ID for the model (e.g., "qwen2_5_0_5b")
    pub model_uid: String,

    /// Number of replicas desired (default: 1)
    #[serde(default = "default_replicas")]
    pub replicas: u32,

    /// Optional configuration overrides
    #[serde(default)]
    pub config: Option<ModelConfig>,

    /// Optional target node for manual placement
    pub node_id: Option<String>,

    /// Optional target GPU index for manual placement (legacy single-GPU)
    pub gpu_index: Option<u32>,

    /// Optional target GPU indices for multi-GPU placement
    #[serde(default)]
    pub gpu_indices: Option<Vec<u32>>,

    /// Minimum number of replicas (for autoscaling). Defaults to None (use `replicas` as fixed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_replicas: Option<u32>,

    /// Maximum number of replicas (for autoscaling). Defaults to None (use `replicas` as fixed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_replicas: Option<u32>,

    /// Engine type to use for this model (e.g. "vllm", "sglang"). Defaults to "vllm".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_type: Option<String>,

    /// Docker image override for the engine. If set, takes precedence over node CLI config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker_image: Option<String>,
}

fn default_replicas() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
    pub tensor_parallel_size: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub max_model_len: Option<u32>,

    #[serde(default)]
    pub required_vram_mb: Option<u64>,

    #[serde(default)]
    pub lora_modules: Option<Vec<String>>,

    /// OpenAI API model id exposed by the engine (`--served-model-name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub served_model_name: Option<String>,

    /// vLLM KV cache dtype (e.g. `fp8` for DeepSeek V4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv_cache_dtype: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_remote_code: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_expert_parallel: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_size: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokenizer_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelRequestStatus {
    Pending,
    Scheduled,
    Running,
    Unloading,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRequest {
    pub id: String,
    pub request: ModelLoadRequest,
    pub status: ModelRequestStatus,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    String(String),
    Array(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}
