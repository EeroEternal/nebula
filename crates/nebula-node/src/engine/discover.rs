//! Runtime capability discovery (plan P1).
//!
//! Probes the live engine HTTP surface and merges hints onto the static table.
//! Discovery failures must not block serving — callers keep the static profile.

use nebula_common::{
    static_capability, CapabilitySource, EngineCapability, SupportLevel,
};

/// Best-effort runtime hints collected from a live engine.
#[derive(Debug, Clone, Default)]
pub struct RuntimeCapabilityHints {
    pub engine_version: Option<String>,
    pub openai_reachable: Option<bool>,
    pub metrics_reachable: Option<bool>,
    pub pending_requests: Option<SupportLevel>,
    pub kv_cache_usage: Option<SupportLevel>,
    pub prefix_cache_hit_rate: Option<SupportLevel>,
}

/// Merge static capability with runtime hints. Always sets `source = RuntimeDiscovery`
/// when any hint is present; otherwise returns the static profile unchanged.
pub fn merge_runtime_capability(
    engine_type: &str,
    hints: RuntimeCapabilityHints,
) -> EngineCapability {
    let mut cap = static_capability(engine_type).unwrap_or_else(|| EngineCapability {
        engine_type: engine_type.to_string(),
        engine_version: None,
        source: CapabilitySource::StaticTable,
        openai_compatible: None,
        tensor_parallel: None,
        data_parallel: None,
        lora: None,
        structured_output: None,
        kv_connector: None,
        grpc: None,
        topologies: vec![],
        observability: Default::default(),
        notes: Some("no static capability table".into()),
    });

    let has_hint = hints.engine_version.is_some()
        || hints.openai_reachable.is_some()
        || hints.metrics_reachable.is_some()
        || hints.pending_requests.is_some()
        || hints.kv_cache_usage.is_some()
        || hints.prefix_cache_hit_rate.is_some();
    if !has_hint {
        return cap;
    }

    cap.source = CapabilitySource::RuntimeDiscovery;
    if let Some(v) = hints.engine_version {
        cap.engine_version = Some(v);
    }
    if let Some(ok) = hints.openai_reachable {
        cap.openai_compatible = Some(ok);
    }
    if let Some(level) = hints.pending_requests {
        cap.observability.pending_requests = level;
    }
    if let Some(level) = hints.kv_cache_usage {
        cap.observability.kv_cache_usage = level;
    }
    if let Some(level) = hints.prefix_cache_hit_rate {
        cap.observability.prefix_cache_hit_rate = level;
    }
    if hints.metrics_reachable == Some(false) {
        // Metrics endpoint down: mark control-plane scrape fields unknown (not zero).
        if hints.pending_requests.is_none() {
            cap.observability.pending_requests = SupportLevel::Unknown;
        }
        if hints.kv_cache_usage.is_none() {
            cap.observability.kv_cache_usage = SupportLevel::Unknown;
        }
    }
    cap.notes = Some(match hints.metrics_reachable {
        Some(true) => format!(
            "runtime discovery ok; version={:?}",
            cap.engine_version.as_deref().unwrap_or("n/a")
        ),
        Some(false) => "runtime discovery: /metrics unreachable".into(),
        None => "runtime discovery partial".into(),
    });
    cap
}

/// Probe a running engine and return a capability profile.
///
/// Never returns Err for probe misses — returns static/merged capability instead.
pub async fn discover_runtime_capability(
    http: &reqwest::Client,
    engine_type: &str,
    base_url: &str,
) -> EngineCapability {
    let base = base_url.trim_end_matches('/');
    let mut hints = RuntimeCapabilityHints::default();

    // OpenAI-compatible surface
    let models_url = format!("{base}/v1/models");
    match http.get(&models_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            hints.openai_reachable = Some(true);
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                // Prefer root "version" if present; else model id as weak version hint.
                if let Some(ver) = v.get("version").and_then(|x| x.as_str()) {
                    hints.engine_version = Some(ver.to_string());
                } else if let Some(id) = v
                    .get("data")
                    .and_then(|d| d.get(0))
                    .and_then(|m| m.get("id"))
                    .and_then(|id| id.as_str())
                {
                    hints.engine_version = Some(format!("model:{id}"));
                }
            }
        }
        Ok(_) | Err(_) => {
            hints.openai_reachable = Some(false);
        }
    }

    // Metrics surface — only reachability; field-level SupportLevel comes from scrape.
    let metrics_url = format!("{base}/metrics");
    match http.get(&metrics_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            hints.metrics_reachable = Some(true);
            if let Ok(text) = resp.text().await {
                match engine_type {
                    "vllm" => {
                        hints.pending_requests = Some(if text.contains("num_requests_waiting")
                            || text.contains("num_requests_running")
                        {
                            SupportLevel::Supported
                        } else {
                            SupportLevel::Unsupported
                        });
                        hints.kv_cache_usage = Some(if text.contains("kv_cache_usage_perc")
                            || text.contains("gpu_cache_usage_perc")
                        {
                            SupportLevel::Supported
                        } else {
                            SupportLevel::Unsupported
                        });
                        hints.prefix_cache_hit_rate = Some(
                            if text.contains("prefix_cache") || text.contains("gpu_prefix_cache") {
                                SupportLevel::Supported
                            } else {
                                SupportLevel::Unsupported
                            },
                        );
                    }
                    "sglang" => {
                        hints.pending_requests = Some(if text.contains("num_requests_waiting")
                            || text.contains("num_requests_running")
                        {
                            SupportLevel::Supported
                        } else {
                            SupportLevel::Unsupported
                        });
                        hints.kv_cache_usage = Some(
                            if text.contains("token_usage") || text.contains("cache_usage") {
                                SupportLevel::Supported
                            } else {
                                SupportLevel::Unsupported
                            },
                        );
                        hints.prefix_cache_hit_rate = Some(SupportLevel::Unsupported);
                    }
                    _ => {}
                }
            }
        }
        Ok(_) | Err(_) => {
            hints.metrics_reachable = Some(false);
        }
    }

    merge_runtime_capability(engine_type, hints)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_without_hints_keeps_static_source() {
        let cap = merge_runtime_capability("vllm", RuntimeCapabilityHints::default());
        assert_eq!(cap.source, CapabilitySource::StaticTable);
        assert_eq!(cap.engine_type, "vllm");
    }

    #[test]
    fn merge_with_version_marks_runtime_discovery() {
        let cap = merge_runtime_capability(
            "sglang",
            RuntimeCapabilityHints {
                engine_version: Some("0.4.1".into()),
                metrics_reachable: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(cap.source, CapabilitySource::RuntimeDiscovery);
        assert_eq!(cap.engine_version.as_deref(), Some("0.4.1"));
    }

    #[test]
    fn metrics_down_marks_obs_unknown() {
        let cap = merge_runtime_capability(
            "vllm",
            RuntimeCapabilityHints {
                metrics_reachable: Some(false),
                ..Default::default()
            },
        );
        assert_eq!(cap.observability.pending_requests, SupportLevel::Unknown);
        assert_eq!(cap.observability.kv_cache_usage, SupportLevel::Unknown);
    }
}
