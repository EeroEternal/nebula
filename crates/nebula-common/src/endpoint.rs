use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    GrpcShim,
    NativeHttp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EndpointStatus {
    Starting,
    Ready,
    Unhealthy,
    Draining,
    /// Recovery budget exhausted; waiting for human / scheduler intervention.
    Failed,
}

fn default_endpoint_kind() -> EndpointKind {
    EndpointKind::NativeHttp
}

fn default_api_flavor() -> String {
    "openai".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointInfo {
    pub model_uid: String,
    pub replica_id: u32,
    #[serde(default)]
    pub plan_version: u64,
    #[serde(default)]
    pub node_id: String,

    #[serde(default = "default_endpoint_kind")]
    pub endpoint_kind: EndpointKind,
    #[serde(default = "default_api_flavor")]
    pub api_flavor: String,

    pub status: EndpointStatus,
    #[serde(default)]
    pub last_heartbeat_ms: u64,

    /// Human-readable detail when status is unhealthy/failed (OOM, exit code, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_detail: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_target: Option<String>,
    pub base_url: Option<String>,

    /// Underlying engine type if known (e.g. "vllm", "powerllm", "sglang").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_type: Option<String>,
}

/// Real-time control-plane stats for a replica.
///
/// `None` on optional metric fields means the value is unavailable
/// (unsupported by the engine version, scrape miss, or unknown) — never treat
/// absence as zero.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EndpointStats {
    pub model_uid: String,
    pub replica_id: u32,
    pub last_updated_ms: u64,

    pub pending_requests: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_cache_hit_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_hit_rate: Option<f64>,

    /// KV / cache occupancy ratio in `[0.0, 1.0]`.
    ///
    /// Replaces the legacy `kv_cache_used_bytes` / `kv_cache_free_bytes` fields,
    /// which stored a permille scale falsely labeled as bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kv_cache_usage: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct EndpointStatsDe {
    model_uid: String,
    replica_id: u32,
    last_updated_ms: u64,
    pending_requests: u64,
    #[serde(default)]
    prefix_cache_hit_rate: Option<f64>,
    #[serde(default)]
    prompt_cache_hit_rate: Option<f64>,
    #[serde(default)]
    kv_cache_usage: Option<f64>,
    /// Legacy permille-as-bytes fields; accepted on read only.
    #[serde(default)]
    kv_cache_used_bytes: Option<u64>,
    #[serde(default)]
    kv_cache_free_bytes: Option<u64>,
}

impl From<EndpointStatsDe> for EndpointStats {
    fn from(d: EndpointStatsDe) -> Self {
        let kv_cache_usage =
            d.kv_cache_usage
                .or_else(|| match (d.kv_cache_used_bytes, d.kv_cache_free_bytes) {
                    (Some(used), Some(free)) => {
                        let total = used.saturating_add(free);
                        if total > 0 {
                            Some(used as f64 / total as f64)
                        } else {
                            None
                        }
                    }
                    _ => None,
                });
        Self {
            model_uid: d.model_uid,
            replica_id: d.replica_id,
            last_updated_ms: d.last_updated_ms,
            pending_requests: d.pending_requests,
            prefix_cache_hit_rate: d.prefix_cache_hit_rate,
            prompt_cache_hit_rate: d.prompt_cache_hit_rate,
            kv_cache_usage,
        }
    }
}

impl<'de> Deserialize<'de> for EndpointStats {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        EndpointStatsDe::deserialize(deserializer).map(Into::into)
    }
}

impl EndpointStats {
    pub fn new(model_uid: impl Into<String>, replica_id: u32, last_updated_ms: u64) -> Self {
        Self {
            model_uid: model_uid.into(),
            replica_id,
            last_updated_ms,
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_kv_permille_as_usage() {
        let json = r#"{
            "model_uid": "m",
            "replica_id": 0,
            "last_updated_ms": 1,
            "pending_requests": 2,
            "kv_cache_used_bytes": 450,
            "kv_cache_free_bytes": 550
        }"#;
        let stats: EndpointStats = serde_json::from_str(json).unwrap();
        assert!((stats.kv_cache_usage.unwrap() - 0.45).abs() < 1e-9);
    }

    #[test]
    fn deserializes_minimal_third_party_endpoint_info() {
        let json = r#"{
            "model_uid": "qwen2.5-7b",
            "replica_id": 0,
            "status": "ready",
            "base_url": "http://10.99.255.102:9997",
            "engine_type": "powerllm"
        }"#;
        let ep: EndpointInfo = serde_json::from_str(json).unwrap();
        assert_eq!(ep.model_uid, "qwen2.5-7b");
        assert_eq!(ep.replica_id, 0);
        assert_eq!(ep.status, EndpointStatus::Ready);
        assert_eq!(ep.base_url.as_deref(), Some("http://10.99.255.102:9997"));
        assert_eq!(ep.endpoint_kind, EndpointKind::NativeHttp);
        assert_eq!(ep.api_flavor, "openai");
        assert_eq!(ep.plan_version, 0);
        assert_eq!(ep.node_id, "");
        assert_eq!(ep.engine_type.as_deref(), Some("powerllm"));
    }
}
