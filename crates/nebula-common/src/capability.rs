//! Engine capability and Serving Cell contracts (plan P0/P1).
//!
//! These types define ownership boundaries. Native gateway / PD topologies are
//! recorded as ingress + capabilities only — Nebula must not invent P/D worker
//! assignment or write operations against them.

use serde::{Deserialize, Serialize};

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

/// How a model serving topology is owned and entered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ServingTopologyKind {
    /// Single local engine process managed by Nebula.
    Standalone,
    /// Homogeneous Nebula-managed replicas; Nebula Router selects among them.
    Replicated,
    /// External engine-native gateway (vLLM Router, SGLang Model Gateway, …)
    /// registered as a whole Cell Ingress. Nebula does not manage workers.
    NativeGateway,
    /// Prefill/Decode disaggregation owned by the engine serving stack.
    /// Nebula records capabilities and ingress only — no worker assignment.
    PdDisaggregated,
}

/// Declared serving topology for a model / cell.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServingTopology {
    pub kind: ServingTopologyKind,
    /// Human-readable engine serving stack identity, e.g. "sglang-model-gateway".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_stack: Option<String>,
    /// Engine-reported capability notes (read-only; not a control surface).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Whole-cell ingress for a native or Nebula-managed serving entry.
///
/// For `native_gateway` / `pd_disaggregated`, this is the only address Nebula
/// routes to. There are intentionally no Prefill/Decode worker lists or
/// scale/write fields here.
///
/// etcd key: `/cells/{model_uid}/{cell_id}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CellIngress {
    pub cell_id: String,
    pub model_uid: String,
    /// OpenAI-compatible base URL of the cell ingress.
    pub base_url: String,
    pub topology: ServingTopology,
    /// Optional read-only health URL; defaults to `{base_url}/health` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_url: Option<String>,
    /// Optional read-only metrics URL; defaults to `{base_url}/metrics` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,

    /// Ingress-level health from Nebula's last probe (not worker pool size).
    #[serde(default)]
    pub status: CellHealthStatus,

    /// Always `not_visible` unless a stable official worker read API is wired later.
    #[serde(default)]
    pub internal_topology: InternalTopologyVisibility,

    #[serde(default)]
    pub last_checked_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
}

/// Health of the Cell Ingress as a whole (never invents worker counts).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum CellHealthStatus {
    Ready,
    Unhealthy,
    #[default]
    Unknown,
}

/// Visibility of native Cell internal topology to Nebula.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum InternalTopologyVisibility {
    /// Default: Nebula must not invent Prefill/Decode/worker counts.
    #[default]
    NotVisible,
    /// Reserved for a future official read-only worker API.
    VisibleReadonly,
}

impl CellIngress {
    pub fn resolved_health_url(&self) -> String {
        self.health_url
            .clone()
            .unwrap_or_else(|| format!("{}/health", self.base_url.trim_end_matches('/')))
    }

    pub fn resolved_metrics_url(&self) -> String {
        self.metrics_url
            .clone()
            .unwrap_or_else(|| format!("{}/metrics", self.base_url.trim_end_matches('/')))
    }

    /// Convert to a synthetic route endpoint so Router can proxy without worker selection.
    /// `replica_id` is always 0; `node_id` is `cell:{cell_id}`.
    pub fn as_route_endpoint(&self) -> crate::endpoint::EndpointInfo {
        use crate::endpoint::{EndpointInfo, EndpointKind, EndpointStatus};
        let status = match self.status {
            CellHealthStatus::Ready => EndpointStatus::Ready,
            CellHealthStatus::Unhealthy => EndpointStatus::Unhealthy,
            CellHealthStatus::Unknown => EndpointStatus::Starting,
        };
        EndpointInfo {
            model_uid: self.model_uid.clone(),
            replica_id: 0,
            plan_version: 0,
            node_id: format!("cell:{}", self.cell_id),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".to_string(),
            status,
            last_heartbeat_ms: self.last_checked_ms,
            grpc_target: None,
            base_url: Some(self.base_url.trim_end_matches('/').to_string()),
        }
    }
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
            known_issues: vec![
                "prefix_cache metrics differ across minor versions".into(),
            ],
        }],
        "sglang" => vec![EngineVersionSupport {
            engine_type: "sglang".into(),
            min_inclusive: Some("0.3.0".into()),
            max_exclusive: None,
            known_issues: vec![
                "prompt/prefix cache SLI mapped only when official metrics exist".into(),
            ],
        }],
        _ => Vec::new(),
    }
}

/// Parse a dotted numeric version prefix (`"0.8.3+cu124"` → `[0,8,3]`).
pub fn parse_version_tuple(version: &str) -> Option<Vec<u64>> {
    let core = version
        .split(|c: char| c == '+' || c == '-' || c == '_')
        .next()
        .unwrap_or(version);
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

    /// Topologies this engine release can participate in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topologies: Vec<ServingTopologyKind>,

    #[serde(default)]
    pub observability: ObservabilityCapability,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Engine types Nebula can launch as ordinary replicas today.
pub const KNOWN_ENGINE_TYPES: &[&str] = &["vllm", "sglang"];

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
        _ => None,
    }
}

pub fn static_capability_vllm() -> EngineCapability {
    EngineCapability {
        engine_type: "vllm".into(),
        engine_version: None,
        source: CapabilitySource::StaticTable,
        openai_compatible: Some(true),
        tensor_parallel: Some(true),
        data_parallel: Some(true),
        lora: Some(true),
        structured_output: Some(true),
        kv_connector: Some(true),
        grpc: Some(false),
        topologies: vec![
            ServingTopologyKind::Standalone,
            ServingTopologyKind::Replicated,
            ServingTopologyKind::NativeGateway,
            ServingTopologyKind::PdDisaggregated,
        ],
        observability: ObservabilityCapability {
            pending_requests: SupportLevel::Supported,
            kv_cache_usage: SupportLevel::Supported,
            prefix_cache_hit_rate: SupportLevel::Supported,
            prompt_cache_hit_rate: SupportLevel::Unsupported,
            ttft: SupportLevel::Unknown,
            tpot: SupportLevel::Unknown,
        },
        notes: Some(
            "Ordinary Nebula-managed replicas; native vLLM Router is Cell Ingress only".into(),
        ),
    }
}

pub fn static_capability_sglang() -> EngineCapability {
    EngineCapability {
        engine_type: "sglang".into(),
        engine_version: None,
        source: CapabilitySource::StaticTable,
        openai_compatible: Some(true),
        tensor_parallel: Some(true),
        data_parallel: Some(true),
        lora: Some(true),
        structured_output: Some(true),
        kv_connector: Some(true),
        grpc: Some(false),
        topologies: vec![
            ServingTopologyKind::Standalone,
            ServingTopologyKind::Replicated,
            ServingTopologyKind::NativeGateway,
            ServingTopologyKind::PdDisaggregated,
        ],
        observability: ObservabilityCapability {
            pending_requests: SupportLevel::Supported,
            kv_cache_usage: SupportLevel::Supported,
            prefix_cache_hit_rate: SupportLevel::Unsupported,
            prompt_cache_hit_rate: SupportLevel::Unsupported,
            ttft: SupportLevel::Unknown,
            tpot: SupportLevel::Unknown,
        },
        notes: Some(
            "Ordinary Nebula-managed replicas; SGLang Model Gateway is Cell Ingress only".into(),
        ),
    }
}

/// Validate `ModelConfig` against a resolved engine type's static capability.
pub fn validate_model_config(
    engine_type: &str,
    config: &crate::model_request::ModelConfig,
) -> Result<(), String> {
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
    }

    if let Some(max_len) = config.max_model_len {
        if max_len == 0 {
            return Err("max_model_len must be >= 1".into());
        }
    }

    if let Some(ref modules) = config.lora_modules {
        if !modules.is_empty() && cap.lora == Some(false) {
            return Err(format!(
                "engine '{engine_type}' does not support LoRA modules"
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
    use crate::model_request::ModelConfig;

    #[test]
    fn native_gateway_ingress_has_no_worker_fields() {
        let ingress = CellIngress {
            cell_id: "cell-1".into(),
            model_uid: "m".into(),
            base_url: "http://127.0.0.1:30000".into(),
            topology: ServingTopology {
                kind: ServingTopologyKind::NativeGateway,
                native_stack: Some("sglang-model-gateway".into()),
                notes: None,
            },
            health_url: None,
            metrics_url: None,
            engine_type: Some("sglang".into()),
            engine_version: Some("0.4.0".into()),
            status: CellHealthStatus::Unknown,
            internal_topology: InternalTopologyVisibility::NotVisible,
            last_checked_ms: 0,
            updated_at_ms: 0,
        };
        let v = serde_json::to_value(&ingress).unwrap();
        assert!(v.get("workers").is_none());
        assert!(v.get("prefill").is_none());
        assert!(v.get("decode").is_none());
        assert_eq!(v["topology"]["kind"].as_str(), Some("native_gateway"));
        assert_eq!(v["internal_topology"].as_str(), Some("not_visible"));
        let ep = ingress.as_route_endpoint();
        assert_eq!(ep.replica_id, 0);
        assert_eq!(ep.node_id, "cell:cell-1");
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
            tensor_parallel_size: None,
            gpu_memory_utilization: Some(1.5),
            max_model_len: None,
            required_vram_mb: None,
            lora_modules: None,
        };
        assert!(validate_model_config("vllm", &bad_util)
            .unwrap_err()
            .contains("gpu_memory_utilization"));

        let bad_tp = ModelConfig {
            tensor_parallel_size: Some(0),
            gpu_memory_utilization: Some(0.9),
            max_model_len: None,
            required_vram_mb: None,
            lora_modules: None,
        };
        assert!(validate_model_config("sglang", &bad_tp)
            .unwrap_err()
            .contains("tensor_parallel_size"));
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

        let s = static_capability_sglang();
        assert_eq!(
            s.observability.prefix_cache_hit_rate,
            SupportLevel::Unsupported
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
