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

    /// Per-replica placement; length must equal `replicas` when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replica_specs: Option<Vec<crate::model_deployment::ReplicaPlacementSpec>>,

    /// One-shot webhook URL for the async Operation lifecycle (HTTPS recommended).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_url: Option<String>,
}

fn default_replicas() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeculativeMethod {
    Mtp,
    DraftModel,
    Dspark,
    Dflash2,
    Ngram,
}

impl SpeculativeMethod {
    pub fn as_vllm_method(&self) -> &'static str {
        match self {
            Self::Mtp => "mtp",
            Self::DraftModel => "draft_model",
            Self::Dspark => "dspark",
            Self::Dflash2 => "dflash2",
            Self::Ngram => "ngram",
        }
    }

    pub fn as_sglang_algorithm(&self) -> &'static str {
        match self {
            Self::Mtp => "NEXTN",
            Self::DraftModel => "EAGLE",
            Self::Dspark => "DSPARK",
            Self::Dflash2 => "DFLASH2",
            Self::Ngram => "LOOKAHEAD",
        }
    }
}

/// Declarative speculative decoding configuration (Batch 5.6: DSpark / DFlash2 + Context Lookup).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SpeculativeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<SpeculativeMethod>,
    /// Companion draft model HF id or local path (e.g. DSpark-1B or DFlash2 drafter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_model: Option<String>,
    /// Verification block size (`k`, e.g. 7, 14, or 16 tokens per forward pass).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_speculative_tokens: Option<u32>,
    /// Context lookup augmentation window (`prompt_lookup_max` for n-gram / DFlash2 suffix fill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_lookup_max: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_lookup_min: Option<u32>,
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

    /// Explicit prefix-caching toggle (`--enable-prefix-caching` on vLLM, guarding
    /// against silent disable on hybrid-attention models such as Qwen3.8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_prefix_caching: Option<bool>,

    /// Declarative speculative decoding kit (DSpark / DFlash2 / MTP + Context Lookup).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speculative_config: Option<SpeculativeConfig>,
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
