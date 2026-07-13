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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointInfo {
    pub model_uid: String,
    pub replica_id: u32,
    pub plan_version: u64,
    pub node_id: String,

    pub endpoint_kind: EndpointKind,
    pub api_flavor: String,

    pub status: EndpointStatus,
    pub last_heartbeat_ms: u64,

    pub grpc_target: Option<String>,
    pub base_url: Option<String>,
}

impl EndpointInfo {
    /// Synthetic Serving Cell ingress endpoints use `node_id = "cell:{cell_id}"`.
    pub fn is_cell_ingress(&self) -> bool {
        self.node_id.starts_with("cell:")
    }
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
        let kv_cache_usage = d.kv_cache_usage.or_else(|| {
            match (d.kv_cache_used_bytes, d.kv_cache_free_bytes) {
                (Some(used), Some(free)) => {
                    let total = used.saturating_add(free);
                    if total > 0 {
                        Some(used as f64 / total as f64)
                    } else {
                        None
                    }
                }
                _ => None,
            }
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
    fn serializes_usage_not_legacy_bytes() {
        let stats = EndpointStats {
            model_uid: "m".into(),
            replica_id: 0,
            last_updated_ms: 1,
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.45),
        };
        let v = serde_json::to_value(&stats).unwrap();
        assert!(v.get("kv_cache_usage").is_some());
        assert!(v.get("kv_cache_used_bytes").is_none());
        assert!(v.get("kv_cache_free_bytes").is_none());
    }
}
