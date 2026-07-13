use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GpuStatus {
    pub index: u32,
    pub memory_total_mb: u64,
    pub memory_used_mb: u64,

    /// GPU temperature in Celsius.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature_c: Option<u32>,

    /// GPU compute utilization percentage (0-100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utilization_gpu: Option<u32>,

    /// Product name from nvidia-smi, e.g. "NVIDIA A100-SXM4-40GB".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Driver version, e.g. "550.54.15".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_version: Option<String>,

    /// CUDA version reported by the driver, e.g. "12.4".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeStatus {
    pub node_id: String,
    pub last_heartbeat_ms: u64,

    #[serde(default)]
    pub gpus: Vec<GpuStatus>,

    /// Node HTTP API address (e.g. "http://10.21.11.92:9090") for BFF to query containers/images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_addr: Option<String>,

    /// Hardware / runtime platform id, e.g. `"nvidia-cuda"`, `"ascend-cann8"`.
    /// Used to match `EngineImage.platforms`. Empty/None = unknown (planner skips platform filter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
}

/// Default platform when Node does not set one explicitly.
pub const DEFAULT_NODE_PLATFORM: &str = "nvidia-cuda";

/// Resolve node platform: explicit field, else `NEBULA_NODE_PLATFORM`, else default.
pub fn resolve_node_platform(node: &NodeStatus) -> String {
    if let Some(p) = node.platform.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return p.to_string();
    }
    std::env::var("NEBULA_NODE_PLATFORM")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_NODE_PLATFORM.to_string())
}

/// Whether an image's `platforms` allow scheduling onto `node_platform`.
/// Empty `platforms` = compatible with all (legacy).
pub fn image_platforms_match(platforms: &[String], node_platform: &str) -> bool {
    if platforms.is_empty() {
        return true;
    }
    platforms.iter().any(|p| p == node_platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_platforms_match_all() {
        assert!(image_platforms_match(&[], "nvidia-cuda"));
        assert!(image_platforms_match(&[], "ascend-cann8"));
    }

    #[test]
    fn platforms_filter_exact() {
        let plats = vec!["nvidia-cuda".into()];
        assert!(image_platforms_match(&plats, "nvidia-cuda"));
        assert!(!image_platforms_match(&plats, "ascend-cann8"));
    }

    #[test]
    fn resolve_prefers_explicit_platform() {
        let node = NodeStatus {
            node_id: "n1".into(),
            last_heartbeat_ms: 0,
            gpus: vec![],
            api_addr: None,
            platform: Some("ascend-cann8".into()),
        };
        assert_eq!(resolve_node_platform(&node), "ascend-cann8");
    }
}
