//! Engine capability contracts.
//!
//! These types define which engine features and topologies Nebula can manage
//! as ordinary replicas (standalone / replicated).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Three-state support for a capability or metric field.
///
/// Never encode "unsupported" as numeric zero.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Supported,
    Unsupported,
    #[default]
    Unknown,
}

impl SupportLevel {
    pub fn is_supported(self) -> bool {
        matches!(self, Self::Supported)
    }
}

/// Declared compute format, not inferred from a chip or image name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum QuantFormat {
    W4A4,
    W4A16,
    Fp8,
    W8A8,
    Int8,
}

/// How a model serving topology is owned and entered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ServingTopologyKind {
    /// Single local engine process managed by Nebula.
    Standalone,
    /// Homogeneous Nebula-managed replicas; Nebula Router selects among them.
    Replicated,
}

/// Persisted per-replica capability snapshot (etcd `/capabilities/{model_uid}/{replica_id}`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplicaCapability {
    pub model_uid: String,
    pub replica_id: u32,
    pub capability: EngineCapability,
    pub updated_at_ms: u64,
}

/// Declared adapter / engine version support range (static table; not a live probe).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineVersionSupport {
    pub engine_type: String,
    /// Inclusive lower bound, e.g. `"0.6.0"`. `None` = no floor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_inclusive: Option<String>,
    /// Exclusive upper bound, e.g. `"1.0.0"`. `None` = no ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_exclusive: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_issues: Vec<String>,
}

/// Static version ranges Nebula adapters claim to support today.
pub fn static_version_support(engine_type: &str) -> Vec<EngineVersionSupport> {
    match engine_type {
        "vllm" => vec![EngineVersionSupport {
            engine_type: "vllm".into(),
            min_inclusive: Some("0.6.0".into()),
            max_exclusive: None,
            known_issues: vec!["prefix_cache metrics differ across minor versions".into()],
        }],
        "sglang" => vec![EngineVersionSupport {
            engine_type: "sglang".into(),
            min_inclusive: Some("0.3.0".into()),
            max_exclusive: None,
            known_issues: vec![
                "prompt/prefix cache SLI mapped only when official metrics exist".into(),
            ],
        }],
        "tokenspeed" => vec![EngineVersionSupport {
            engine_type: "tokenspeed".into(),
            min_inclusive: Some("0.1.0".into()),
            max_exclusive: None,
            known_issues: Vec::new(),
        }],
        "zstep" => vec![EngineVersionSupport {
            engine_type: "zstep".into(),
            min_inclusive: Some("0.1.0".into()),
            max_exclusive: None,
            known_issues: vec!["tool_calling is not enabled in online serving".into()],
        }],
        _ => Vec::new(),
    }
}

/// Parse a dotted numeric version prefix (`"0.8.3+cu124"` → `[0,8,3]`).
pub fn parse_version_tuple(version: &str) -> Option<Vec<u64>> {
    let core = version.split(['+', '-', '_']).next().unwrap_or(version);
    let parts: Vec<u64> = core
        .split('.')
        .take(4)
        .map(|p| p.parse::<u64>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn cmp_version(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let len = a.len().max(b.len());
    for i in 0..len {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

/// Validate an optional engine version against the static support table.
/// Missing / unparseable version → accepted (unknown, not a hard fail).
pub fn validate_engine_version(engine_type: &str, version: Option<&str>) -> Result<(), String> {
    let Some(raw) = version.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    let Some(ver) = parse_version_tuple(raw) else {
        return Ok(());
    };
    let ranges = static_version_support(engine_type);
    if ranges.is_empty() {
        return Ok(());
    }
    for range in &ranges {
        let min_ok = range
            .min_inclusive
            .as_deref()
            .and_then(parse_version_tuple)
            .map(|m| cmp_version(&ver, &m) != std::cmp::Ordering::Less)
            .unwrap_or(true);
        let max_ok = range
            .max_exclusive
            .as_deref()
            .and_then(parse_version_tuple)
            .map(|m| cmp_version(&ver, &m) == std::cmp::Ordering::Less)
            .unwrap_or(true);
        if min_ok && max_ok {
            return Ok(());
        }
    }
    Err(format!(
        "engine_type '{engine_type}' version '{raw}' is outside Nebula adapter support ranges"
    ))
}

/// Where a capability claim came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySource {
    /// Static compatibility table shipped with Nebula.
    StaticTable,
    /// Discovered at runtime from the engine / ingress.
    RuntimeDiscovery,
    /// Operator override.
    ManualOverride,
}

/// Metric / feature support matrix for an engine version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ObservabilityCapability {
    #[serde(default)]
    pub pending_requests: SupportLevel,
    #[serde(default)]
    pub kv_cache_usage: SupportLevel,
    #[serde(default)]
    pub prefix_cache_hit_rate: SupportLevel,
    #[serde(default)]
    pub prompt_cache_hit_rate: SupportLevel,
    #[serde(default)]
    pub ttft: SupportLevel,
    #[serde(default)]
    pub tpot: SupportLevel,
}

/// Declared capabilities of an engine release (or discovered runtime profile).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineCapability {
    pub engine_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
    pub source: CapabilitySource,

    /// Active format in a model-scoped snapshot; static engine tables leave it unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantization: Option<QuantFormat>,
    /// Platform/runtime-specific native paths. Missing entries are Unknown, not Unsupported.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub native_low_precision_paths: BTreeMap<QuantFormat, SupportLevel>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_compatible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tensor_parallel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_parallel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv_connector: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc: Option<bool>,

    /// Whether the engine can honor OpenAI-style `tools` / `tool_choice`.
    /// C5: Unsupported → Gateway rejects with stable `unsupported`; Unknown → allow + warn.
    #[serde(default)]
    pub tool_calling: SupportLevel,

    /// Whether the engine exposes `/v1/score` single-pass prefill probability scoring
    /// (Batch 5.4: SGLang native `/v1/score` + JEV compaction gating).
    #[serde(default)]
    pub score_api: SupportLevel,

    /// Topologies this engine release can participate in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topologies: Vec<ServingTopologyKind>,

    #[serde(default)]
    pub observability: ObservabilityCapability,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Engine types Nebula can launch as ordinary replicas today.
pub const KNOWN_ENGINE_TYPES: &[&str] = &["vllm", "sglang", "tokenspeed", "zstep"];

/// Default engine when `engine_type` is omitted (backward compatible).
pub const DEFAULT_ENGINE_TYPE: &str = "vllm";

/// Normalize an optional engine_type: empty/`None` → default `vllm`.
pub fn resolve_engine_type(engine_type: Option<&str>) -> String {
    match engine_type.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.to_ascii_lowercase(),
        None => DEFAULT_ENGINE_TYPE.to_string(),
    }
}

/// Reject unknown engine types with an actionable message.
pub fn validate_engine_type(engine_type: Option<&str>) -> Result<String, String> {
    let resolved = resolve_engine_type(engine_type);
    if KNOWN_ENGINE_TYPES.contains(&resolved.as_str()) {
        Ok(resolved)
    } else {
        Err(format!(
            "unknown engine_type '{resolved}'; supported: {}",
            KNOWN_ENGINE_TYPES.join(", ")
        ))
    }
}

/// Static capability table for a known engine type (`source = StaticTable`).
pub fn static_capability(engine_type: &str) -> Option<EngineCapability> {
    match engine_type {
        "vllm" => Some(static_capability_vllm()),
        "sglang" => Some(static_capability_sglang()),
        "tokenspeed" => Some(static_capability_tokenspeed()),
        "zstep" => Some(static_capability_zstep()),
        _ => None,
    }
}

pub fn static_capability_zstep() -> EngineCapability {
    EngineCapability {
        engine_type: "zstep".into(),
        engine_version: None,
        source: CapabilitySource::StaticTable,
        quantization: None,
        native_low_precision_paths: BTreeMap::from([
            (QuantFormat::Fp8, SupportLevel::Supported),
            (QuantFormat::W4A4, SupportLevel::Supported),
        ]),
        openai_compatible: Some(true),
        tensor_parallel: Some(true),
        data_parallel: Some(false),
        lora: Some(false),
        structured_output: Some(false),
        kv_connector: Some(false),
        grpc: Some(false),
        tool_calling: SupportLevel::Unsupported,
        score_api: SupportLevel::Unsupported,
        topologies: vec![
            ServingTopologyKind::Standalone,
            ServingTopologyKind::Replicated,
        ],
        observability: ObservabilityCapability {
            pending_requests: SupportLevel::Supported,
            kv_cache_usage: SupportLevel::Supported,
            prefix_cache_hit_rate: SupportLevel::Supported,
            prompt_cache_hit_rate: SupportLevel::Unsupported,
            ttft: SupportLevel::Supported,
            tpot: SupportLevel::Supported,
        },
        notes: Some("zstep native Rust/CUDA inference engine replicas".into()),
    }
}

pub fn static_capability_tokenspeed() -> EngineCapability {
    EngineCapability {
        engine_type: "tokenspeed".into(),
        engine_version: None,
        source: CapabilitySource::StaticTable,
        quantization: None,
        native_low_precision_paths: BTreeMap::new(),
        openai_compatible: Some(true),
        tensor_parallel: Some(true),
        data_parallel: Some(true),
        lora: Some(false),
        structured_output: Some(true),
        kv_connector: Some(true),
        grpc: Some(false),
        tool_calling: SupportLevel::Supported,
        score_api: SupportLevel::Unsupported,
        topologies: vec![
            ServingTopologyKind::Standalone,
            ServingTopologyKind::Replicated,
        ],
        observability: ObservabilityCapability {
            pending_requests: SupportLevel::Supported,
            kv_cache_usage: SupportLevel::Supported,
            prefix_cache_hit_rate: SupportLevel::Supported,
            prompt_cache_hit_rate: SupportLevel::Unsupported,
            ttft: SupportLevel::Unknown,
            tpot: SupportLevel::Unknown,
        },
        notes: Some("TokenSpeed unified-path inference engine replicas".into()),
    }
}

pub fn static_capability_vllm() -> EngineCapability {
    EngineCapability {
        engine_type: "vllm".into(),
        engine_version: None,
        source: CapabilitySource::StaticTable,
        quantization: None,
        native_low_precision_paths: BTreeMap::new(),
        openai_compatible: Some(true),
        tensor_parallel: Some(true),
        data_parallel: Some(true),
        lora: Some(true),
        structured_output: Some(true),
        kv_connector: Some(true),
        grpc: Some(false),
        tool_calling: SupportLevel::Supported,
        score_api: SupportLevel::Unsupported,
        topologies: vec![
            ServingTopologyKind::Standalone,
            ServingTopologyKind::Replicated,
        ],
        observability: ObservabilityCapability {
            pending_requests: SupportLevel::Supported,
            kv_cache_usage: SupportLevel::Supported,
            prefix_cache_hit_rate: SupportLevel::Supported,
            prompt_cache_hit_rate: SupportLevel::Unsupported,
            ttft: SupportLevel::Unknown,
            tpot: SupportLevel::Unknown,
        },
        notes: Some("Ordinary Nebula-managed vLLM replicas".into()),
    }
}

pub fn static_capability_sglang() -> EngineCapability {
    EngineCapability {
        engine_type: "sglang".into(),
        engine_version: None,
        source: CapabilitySource::StaticTable,
        quantization: None,
        native_low_precision_paths: BTreeMap::new(),
        openai_compatible: Some(true),
        tensor_parallel: Some(true),
        data_parallel: Some(true),
        lora: Some(true),
        structured_output: Some(true),
        kv_connector: Some(true),
        grpc: Some(false),
        tool_calling: SupportLevel::Supported,
        score_api: SupportLevel::Supported,
        topologies: vec![
            ServingTopologyKind::Standalone,
            ServingTopologyKind::Replicated,
        ],
        observability: ObservabilityCapability {
            pending_requests: SupportLevel::Supported,
            kv_cache_usage: SupportLevel::Supported,
            prefix_cache_hit_rate: SupportLevel::Unsupported,
            prompt_cache_hit_rate: SupportLevel::Unsupported,
            ttft: SupportLevel::Unknown,
            tpot: SupportLevel::Unknown,
        },
        notes: Some("Ordinary Nebula-managed SGLang replicas".into()),
    }
}

/// Resolve `tool_calling` from the static table for a known engine type.
/// Unknown / empty engine → `Unknown` (C5: allow + warn at Gateway).
pub fn tool_calling_for_engine(engine_type: Option<&str>) -> SupportLevel {
    let Some(et) = engine_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return SupportLevel::Unknown;
    };
    static_capability(&et.to_ascii_lowercase())
        .map(|c| c.tool_calling)
        .unwrap_or(SupportLevel::Unknown)
}

/// Resolve `score_api` (`/v1/score`) from the static table for a known engine type.
/// SGLang → `Supported`; vLLM → `Unsupported`; unknown engine → `Unknown`.
pub fn score_api_for_engine(engine_type: Option<&str>) -> SupportLevel {
    let Some(et) = engine_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return SupportLevel::Unknown;
    };
    static_capability(&et.to_ascii_lowercase())
        .map(|c| c.score_api)
        .unwrap_or(SupportLevel::Unknown)
}

/// Validate `ModelConfig` against a resolved engine type's static capability.
pub fn validate_model_config(
    engine_type: &str,
    config: &crate::model_request::ModelConfig,
) -> Result<(), String> {
    if config.max_num_seqs.is_some()
        || config.max_num_batched_tokens.is_some()
        || config.enable_chunked_prefill.is_some()
    {
        if !matches!(engine_type, "vllm" | "zstep") {
            return Err(
                "scheduler tuning fields are currently supported only for vllm and zstep".into(),
            );
        }
        if config.max_num_seqs == Some(0) || config.max_num_batched_tokens == Some(0) {
            return Err("max_num_seqs and max_num_batched_tokens must be positive".into());
        }
        if engine_type == "zstep" && config.max_num_seqs.is_some_and(|s| s > 128) {
            return Err("zstep max_num_seqs must be in 1..=128 (decode CUDA Graph ceiling)".into());
        }
        if engine_type == "zstep" {
            let effective_seqs = config.max_num_seqs.unwrap_or(128);
            if let Some(tokens) = config.max_num_batched_tokens {
                if tokens < effective_seqs {
                    return Err("max_num_batched_tokens must be >= max_num_seqs".into());
                }
            }
            if config.enable_chunked_prefill == Some(false) {
                let effective_tokens = config.max_num_batched_tokens.unwrap_or(4096);
                let effective_context = config.max_model_len.unwrap_or(32768);
                if effective_tokens < effective_context {
                    return Err(
                        "without chunked prefill the token budget must cover max_model_len".into(),
                    );
                }
            }
        } else {
            if let (Some(seqs), Some(tokens)) = (config.max_num_seqs, config.max_num_batched_tokens)
            {
                if tokens < seqs {
                    return Err("max_num_batched_tokens must be >= max_num_seqs".into());
                }
            }
            if config.enable_chunked_prefill == Some(false) {
                if let (Some(tokens), Some(context)) =
                    (config.max_num_batched_tokens, config.max_model_len)
                {
                    if tokens < context {
                        return Err(
                            "without chunked prefill the token budget must cover max_model_len"
                                .into(),
                        );
                    }
                }
            }
        }
    }
    let cap = static_capability(engine_type).ok_or_else(|| {
        format!(
            "unknown engine_type '{engine_type}'; supported: {}",
            KNOWN_ENGINE_TYPES.join(", ")
        )
    })?;

    if let Some(util) = config.gpu_memory_utilization {
        if !(util > 0.0 && util <= 1.0) {
            return Err(format!(
                "gpu_memory_utilization must be in (0.0, 1.0], got {util}"
            ));
        }
    }

    if let Some(tp) = config.tensor_parallel_size {
        if tp == 0 {
            return Err("tensor_parallel_size must be >= 1".into());
        }
        if cap.tensor_parallel == Some(false) {
            return Err(format!(
                "engine '{engine_type}' does not support tensor parallelism"
            ));
        }
        if engine_type == "zstep" && tp > 8 {
            return Err("zstep tensor_parallel_size must be in 1..=8".into());
        }
    }

    if let Some(max_len) = config.max_model_len {
        if max_len == 0 {
            return Err("max_model_len must be >= 1".into());
        }
    }

    if config.block_size == Some(0) {
        return Err("block_size must be >= 1".into());
    }

    if engine_type == "zstep" {
        if let Some(ref dtype) = config.kv_cache_dtype {
            if !matches!(
                dtype.as_str(),
                "auto" | "fp8" | "fp8_e4m3" | "bf16" | "bfloat16"
            ) {
                return Err(format!(
                    "unsupported zstep kv_cache_dtype '{dtype}'; supported: auto, fp8, fp8_e4m3, bf16, bfloat16"
                ));
            }
        }
        if let Some(ref mode) = config.tokenizer_mode {
            if !matches!(mode.as_str(), "auto" | "deepseek_v41" | "chatml") {
                return Err(format!(
                    "unsupported zstep tokenizer_mode '{mode}'; supported: auto, deepseek_v41, chatml"
                ));
            }
        }
    }

    if let Some(ref modules) = config.lora_modules {
        if !modules.is_empty() && cap.lora == Some(false) {
            return Err(format!(
                "engine '{engine_type}' does not support LoRA modules"
            ));
        }
    }

    if let Some(ref spec) = config.speculative_config {
        use crate::model_request::SpeculativeMethod;

        if engine_type == "zstep" {
            if !matches!(
                spec.method,
                Some(SpeculativeMethod::Dspark | SpeculativeMethod::Mtp | SpeculativeMethod::Ngram)
            ) {
                return Err(
                    "engine 'zstep' supports speculative_config.method 'dspark', 'mtp', or 'ngram'"
                        .into(),
                );
            }
            if spec.num_speculative_tokens.is_some_and(|k| k > 7) {
                return Err(
                    "zstep speculative_config.num_speculative_tokens must be in 1..=7 (spec_verify_width <= 8)"
                        .into(),
                );
            }
            if spec
                .draft_model
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
            {
                return Err(
                    "zstep loads built-in DSpark/MTP weights from model_dir and does not accept speculative_config.draft_model"
                        .into(),
                );
            }
        } else if matches!(
            spec.method,
            Some(SpeculativeMethod::Dspark | SpeculativeMethod::Dflash2)
        ) {
            return Err(
                "speculative_config.method 'dspark' / 'dflash2' is rejected until verified on live engines; use 'draft_model', 'mtp', or 'ngram'"
                    .into(),
            );
        }
        if spec.num_speculative_tokens == Some(0) {
            return Err("speculative_config.num_speculative_tokens must be >= 1".into());
        }
        if spec.prompt_lookup_max == Some(0) || spec.prompt_lookup_min == Some(0) {
            return Err(
                "speculative_config.prompt_lookup_max / prompt_lookup_min must be >= 1".into(),
            );
        }
        if let (Some(min), Some(max)) = (spec.prompt_lookup_min, spec.prompt_lookup_max) {
            if min > max {
                return Err(
                    "speculative_config.prompt_lookup_min must be <= prompt_lookup_max".into(),
                );
            }
        }
        if (spec.prompt_lookup_max.is_some() || spec.prompt_lookup_min.is_some())
            && spec.method != Some(crate::model_request::SpeculativeMethod::Ngram)
        {
            return Err(
                "speculative_config.prompt_lookup_max / prompt_lookup_min are only supported when speculative_config.method = ngram"
                    .into(),
            );
        }
        if spec.method == Some(crate::model_request::SpeculativeMethod::DraftModel)
            && spec
                .draft_model
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err("speculative_config.draft_model is required for draft_model".into());
        }
        if matches!(engine_type, "sglang" | "tokenspeed")
            && spec.method == Some(crate::model_request::SpeculativeMethod::Ngram)
        {
            return Err(format!(
                "engine '{engine_type}' does not support speculative_config.method = ngram"
            ));
        }
    }

    Ok(())
}

/// Validate engine type and optional config together (BFF / control-plane entry).
pub fn validate_engine_and_config(
    engine_type: Option<&str>,
    config: Option<&crate::model_request::ModelConfig>,
) -> Result<String, String> {
    let resolved = validate_engine_type(engine_type)?;
    if let Some(cfg) = config {
        validate_model_config(&resolved, cfg)?;
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_request::{ModelConfig, SpeculativeConfig, SpeculativeMethod};

    #[test]
    fn low_precision_declarations_roundtrip_and_legacy_defaults_are_unknown() {
        let legacy = serde_json::to_value(static_capability_vllm()).unwrap();
        assert!(legacy.get("native_low_precision_paths").is_none());
        let mut capability: EngineCapability = serde_json::from_value(legacy).unwrap();
        assert_eq!(capability.quantization, None);
        assert!(capability.native_low_precision_paths.is_empty());
        for (format, wire) in [
            (QuantFormat::W4A4, "w4a4"),
            (QuantFormat::W4A16, "w4a16"),
            (QuantFormat::Fp8, "fp8"),
            (QuantFormat::W8A8, "w8a8"),
            (QuantFormat::Int8, "int8"),
        ] {
            capability.quantization = Some(format);
            capability
                .native_low_precision_paths
                .insert(format, SupportLevel::Supported);
            let json = serde_json::to_value(&capability).unwrap();
            assert_eq!(json["quantization"], wire);
            assert_eq!(json["native_low_precision_paths"][wire], "supported");
            assert_eq!(
                serde_json::from_value::<EngineCapability>(json).unwrap(),
                capability
            );
        }
    }

    #[test]
    fn support_level_roundtrip() {
        for level in [
            SupportLevel::Supported,
            SupportLevel::Unsupported,
            SupportLevel::Unknown,
        ] {
            let s = serde_json::to_string(&level).unwrap();
            let back: SupportLevel = serde_json::from_str(&s).unwrap();
            assert_eq!(level, back);
        }
    }

    #[test]
    fn unknown_engine_type_rejected() {
        let err = validate_engine_type(Some("tensorrt")).unwrap_err();
        assert!(err.contains("unknown engine_type"));
        assert!(err.contains("vllm"));
    }

    #[test]
    fn missing_engine_defaults_to_vllm() {
        assert_eq!(validate_engine_type(None).unwrap(), "vllm");
        assert_eq!(validate_engine_type(Some("")).unwrap(), "vllm");
        assert_eq!(validate_engine_type(Some("VLLM")).unwrap(), "vllm");
    }

    #[test]
    fn rejects_invalid_gpu_util_and_tp() {
        let bad_util = ModelConfig {
            gpu_memory_utilization: Some(1.5),
            ..Default::default()
        };
        assert!(validate_model_config("vllm", &bad_util)
            .unwrap_err()
            .contains("gpu_memory_utilization"));

        let bad_tp = ModelConfig {
            tensor_parallel_size: Some(0),
            gpu_memory_utilization: Some(0.9),
            ..Default::default()
        };
        assert!(validate_model_config("sglang", &bad_tp)
            .unwrap_err()
            .contains("tensor_parallel_size"));
    }

    #[test]
    fn rejects_unverified_methods_non_ngram_prompt_lookup_and_missing_draft_model() {
        let sg_ngram = ModelConfig {
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::Ngram),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_model_config("sglang", &sg_ngram)
            .unwrap_err()
            .contains("ngram"));

        for unverified in [SpeculativeMethod::Dspark, SpeculativeMethod::Dflash2] {
            let cfg = ModelConfig {
                speculative_config: Some(SpeculativeConfig {
                    method: Some(unverified),
                    draft_model: Some("draft".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            assert!(validate_model_config("vllm", &cfg)
                .unwrap_err()
                .contains("rejected until verified on live engines"));
            assert!(validate_model_config("sglang", &cfg)
                .unwrap_err()
                .contains("rejected until verified on live engines"));
        }

        let non_ngram_lookup = ModelConfig {
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::DraftModel),
                draft_model: Some("Qwen/Qwen2.5-0.5B".into()),
                prompt_lookup_max: Some(8),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_model_config("vllm", &non_ngram_lookup)
            .unwrap_err()
            .contains("method = ngram"));

        let missing_draft = ModelConfig {
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::DraftModel),
                draft_model: None,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(validate_model_config("vllm", &missing_draft)
            .unwrap_err()
            .contains("draft_model"));
    }

    #[test]
    fn static_tables_mark_source_and_obs_levels() {
        let v = static_capability_vllm();
        assert_eq!(v.source, CapabilitySource::StaticTable);
        assert_eq!(v.observability.kv_cache_usage, SupportLevel::Supported);
        assert_eq!(
            v.observability.prompt_cache_hit_rate,
            SupportLevel::Unsupported
        );
        assert_eq!(v.tool_calling, SupportLevel::Supported);
        assert_eq!(v.score_api, SupportLevel::Unsupported);

        let s = static_capability_sglang();
        assert_eq!(
            s.observability.prefix_cache_hit_rate,
            SupportLevel::Unsupported
        );
        assert_eq!(s.tool_calling, SupportLevel::Supported);
        assert_eq!(s.score_api, SupportLevel::Supported);

        let ts = static_capability_tokenspeed();
        assert_eq!(
            ts.observability.prefix_cache_hit_rate,
            SupportLevel::Supported
        );
        assert_eq!(ts.tool_calling, SupportLevel::Supported);
        assert_eq!(
            tool_calling_for_engine(Some("tokenspeed")),
            SupportLevel::Supported
        );

        let zs = static_capability_zstep();
        assert_eq!(zs.source, CapabilitySource::StaticTable);
        assert_eq!(
            zs.observability.prefix_cache_hit_rate,
            SupportLevel::Supported
        );
        assert_eq!(zs.observability.ttft, SupportLevel::Supported);
        assert_eq!(zs.observability.tpot, SupportLevel::Supported);
        assert_eq!(zs.tool_calling, SupportLevel::Unsupported);
        assert_eq!(
            tool_calling_for_engine(Some("zstep")),
            SupportLevel::Unsupported
        );
        assert_eq!(
            zs.native_low_precision_paths.get(&QuantFormat::Fp8),
            Some(&SupportLevel::Supported)
        );
        assert_eq!(
            zs.native_low_precision_paths.get(&QuantFormat::W4A4),
            Some(&SupportLevel::Supported)
        );
    }

    #[test]
    fn zstep_validates_scheduler_bounds_and_rejects_unsupported_knobs() {
        assert_eq!(validate_engine_type(Some("zstep")).unwrap(), "zstep");
        assert!(validate_engine_version("zstep", Some("0.1.0")).is_ok());

        let valid = ModelConfig {
            tensor_parallel_size: Some(8),
            gpu_memory_utilization: Some(0.9),
            max_model_len: Some(32768),
            max_num_seqs: Some(128),
            max_num_batched_tokens: Some(4096),
            enable_chunked_prefill: Some(true),
            enable_prefix_caching: Some(false),
            ..Default::default()
        };
        validate_model_config("zstep", &valid).unwrap();

        let too_many_seqs = ModelConfig {
            max_num_seqs: Some(129),
            ..valid.clone()
        };
        assert!(validate_model_config("zstep", &too_many_seqs)
            .unwrap_err()
            .contains("1..=128"));

        let tp_too_large = ModelConfig {
            tensor_parallel_size: Some(16),
            ..valid.clone()
        };
        assert!(validate_model_config("zstep", &tp_too_large)
            .unwrap_err()
            .contains("1..=8"));

        let prefix_on = ModelConfig {
            enable_prefix_caching: Some(true),
            ..valid.clone()
        };
        validate_model_config("zstep", &prefix_on).unwrap();

        let with_spec = ModelConfig {
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::Mtp),
                num_speculative_tokens: Some(2),
                ..Default::default()
            }),
            ..valid.clone()
        };
        validate_model_config("zstep", &with_spec).unwrap();

        for method in [SpeculativeMethod::Dspark, SpeculativeMethod::Ngram] {
            let supported = ModelConfig {
                speculative_config: Some(SpeculativeConfig {
                    method: Some(method),
                    num_speculative_tokens: Some(5),
                    ..Default::default()
                }),
                ..valid.clone()
            };
            validate_model_config("zstep", &supported).unwrap();
        }

        let too_many_drafts = ModelConfig {
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::Mtp),
                num_speculative_tokens: Some(8),
                ..Default::default()
            }),
            ..valid.clone()
        };
        assert!(validate_model_config("zstep", &too_many_drafts)
            .unwrap_err()
            .contains("1..=7"));

        for method in [SpeculativeMethod::DraftModel, SpeculativeMethod::Dflash2] {
            let rejected = ModelConfig {
                speculative_config: Some(SpeculativeConfig {
                    method: Some(method),
                    ..Default::default()
                }),
                ..valid.clone()
            };
            assert!(validate_model_config("zstep", &rejected)
                .unwrap_err()
                .contains("dspark', 'mtp', or 'ngram"));
        }

        let missing_method = ModelConfig {
            speculative_config: Some(SpeculativeConfig::default()),
            ..valid.clone()
        };
        assert!(validate_model_config("zstep", &missing_method)
            .unwrap_err()
            .contains("dspark', 'mtp', or 'ngram"));

        let unexpected_draft_model = ModelConfig {
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::Dspark),
                draft_model: Some("/models/custom-draft".into()),
                num_speculative_tokens: Some(5),
                ..Default::default()
            }),
            ..valid.clone()
        };
        assert!(validate_model_config("zstep", &unexpected_draft_model)
            .unwrap_err()
            .contains("draft_model"));

        // Effective defaults for zstep: max_num_seqs=128, max_num_batched_tokens=4096, max_model_len=32768.
        let batch_below_default_seqs = ModelConfig {
            max_num_batched_tokens: Some(64),
            ..Default::default()
        };
        assert!(validate_model_config("zstep", &batch_below_default_seqs)
            .unwrap_err()
            .contains("max_num_batched_tokens must be >= max_num_seqs"));

        let unchunked_without_budget = ModelConfig {
            enable_chunked_prefill: Some(false),
            ..Default::default()
        };
        assert!(validate_model_config("zstep", &unchunked_without_budget)
            .unwrap_err()
            .contains("without chunked prefill"));

        let zero_block = ModelConfig {
            block_size: Some(0),
            ..valid.clone()
        };
        assert!(validate_model_config("zstep", &zero_block)
            .unwrap_err()
            .contains("block_size"));

        let bad_dtype = ModelConfig {
            kv_cache_dtype: Some("int4".into()),
            ..valid.clone()
        };
        assert!(validate_model_config("zstep", &bad_dtype)
            .unwrap_err()
            .contains("kv_cache_dtype"));

        let bad_tok_mode = ModelConfig {
            tokenizer_mode: Some("mistral".into()),
            ..valid
        };
        assert!(validate_model_config("zstep", &bad_tok_mode)
            .unwrap_err()
            .contains("tokenizer_mode"));
    }

    #[test]
    fn tool_calling_for_engine_unknown_when_missing() {
        assert_eq!(tool_calling_for_engine(None), SupportLevel::Unknown);
        assert_eq!(tool_calling_for_engine(Some("")), SupportLevel::Unknown);
        assert_eq!(
            tool_calling_for_engine(Some("tensorrt")),
            SupportLevel::Unknown
        );
        assert_eq!(
            tool_calling_for_engine(Some("vllm")),
            SupportLevel::Supported
        );
        assert_eq!(
            score_api_for_engine(Some("vllm")),
            SupportLevel::Unsupported
        );
        assert_eq!(
            score_api_for_engine(Some("sglang")),
            SupportLevel::Supported
        );
    }

    #[test]
    fn version_support_accepts_in_range() {
        assert!(validate_engine_version("vllm", Some("0.8.3")).is_ok());
        assert!(validate_engine_version("vllm", Some("0.6.0")).is_ok());
        assert!(validate_engine_version("vllm", None).is_ok());
        assert!(validate_engine_version("vllm", Some("0.5.9"))
            .unwrap_err()
            .contains("outside"));
        assert!(validate_engine_version("sglang", Some("0.4.1")).is_ok());
        assert_eq!(parse_version_tuple("0.8.3+cu124"), Some(vec![0, 8, 3]));
    }
}
