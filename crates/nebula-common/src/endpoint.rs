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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EndpointStats {
    pub model_uid: String,
    pub replica_id: u32,
    pub last_updated_ms: u64,

    pub pending_requests: u64,

    pub prefix_cache_hit_rate: Option<f64>,
    pub prompt_cache_hit_rate: Option<f64>,

    pub kv_cache_used_bytes: Option<u64>,
    pub kv_cache_free_bytes: Option<u64>,
}
