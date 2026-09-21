//! Engine (vLLM) `/metrics` parsing shared by the node and the k8s-controller.
//!
//! Kept in `nebula-common` so both execution planes (bare-metal `nebula-node`
//! and the in-cluster `nebula-k8s-controller`) can turn a vLLM scrape into
//! [`EndpointStats`] without duplicating the metric-name handling.

use crate::endpoint::EndpointStats;

/// Parse a Prometheus `/metrics` body into `EndpointStats`.
pub fn parse_vllm_metrics_text(
    text: &str,
    model_uid: &str,
    replica_id: u32,
    last_updated_ms: u64,
) -> EndpointStats {
    let mut pending_requests: u64 = 0;
    let mut running_requests: u64 = 0;
    let mut kv_cache_usage: Option<f64> = None;
    let mut prefix_cache_hit_rate: Option<f64> = None;
    let mut prefix_cache_hits: Option<f64> = None;
    let mut prefix_cache_queries: Option<f64> = None;

    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }

        // vLLM metric formats (v0.11+):
        //   vllm:num_requests_waiting{...} 3
        //   vllm:num_requests_running{...} 1
        //   vllm:kv_cache_usage_perc{...} 0.45
        //   vllm:prefix_cache_hits_total{...} 100
        //   vllm:prefix_cache_queries_total{...} 200
        //
        // Older versions may use:
        //   vllm:gpu_cache_usage_perc{...} 0.45
        //   vllm:gpu_prefix_cache_hit_rate{...} 0.8
        //
        // Also handle underscore variants without colon:
        //   vllm_num_requests_waiting{...} 3

        if let Some(val) = extract_metric(line, "num_requests_waiting") {
            pending_requests = val as u64;
        } else if let Some(val) = extract_metric(line, "num_requests_running") {
            running_requests = val as u64;
        } else if let Some(val) = extract_metric(line, "kv_cache_usage_perc") {
            kv_cache_usage = Some(val);
        } else if kv_cache_usage.is_none() {
            // Fallback for older vLLM versions
            if let Some(val) = extract_metric(line, "gpu_cache_usage_perc") {
                kv_cache_usage = Some(val);
            }
        }

        // prefix cache: prefer direct hit_rate gauge, else compute from counters
        if let Some(val) = extract_metric(line, "gpu_prefix_cache_hit_rate") {
            prefix_cache_hit_rate = Some(val);
        } else if let Some(val) = extract_metric(line, "cpu_prefix_cache_hit_rate") {
            if prefix_cache_hit_rate.is_none() {
                prefix_cache_hit_rate = Some(val);
            }
        }
        if let Some(val) = extract_metric(line, "prefix_cache_hits_total") {
            prefix_cache_hits = Some(val);
        } else if let Some(val) = extract_metric(line, "prefix_cache_queries_total") {
            prefix_cache_queries = Some(val);
        }
    }

    // Compute prefix cache hit rate from counters if no direct gauge was found
    if prefix_cache_hit_rate.is_none() {
        if let (Some(hits), Some(queries)) = (prefix_cache_hits, prefix_cache_queries) {
            if queries > 0.0 {
                prefix_cache_hit_rate = Some(hits / queries);
            }
        }
    }

    EndpointStats {
        model_uid: model_uid.to_string(),
        replica_id,
        last_updated_ms,
        pending_requests: pending_requests + running_requests,
        prefix_cache_hit_rate,
        prompt_cache_hit_rate: None,
        kv_cache_usage,
    }
}

/// Extract a numeric value from a Prometheus metric line, matching the metric
/// name *exactly* (full name before `{`/whitespace, allowing a `vllm:`/`vllm_`
/// namespace prefix). Exact matching avoids e.g. `prefix_cache_hits_total`
/// matching `external_prefix_cache_hits_total`.
fn extract_metric(line: &str, metric_suffix: &str) -> Option<f64> {
    let name_end = line.find(|c: char| c == '{' || c.is_whitespace())?;
    let name = &line[..name_end];
    let matched = name == metric_suffix
        || name == format!("vllm:{metric_suffix}")
        || name == format!("vllm_{metric_suffix}");
    if !matched {
        return None;
    }

    let value_str = line.rsplit_once(|c: char| c.is_whitespace())?.1;
    value_str.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v1_metrics() {
        let text = "\
# HELP vllm:num_requests_waiting waiting\n\
vllm:num_requests_waiting{model_name=\"m\"} 3\n\
vllm:num_requests_running{model_name=\"m\"} 1\n\
vllm:kv_cache_usage_perc{model_name=\"m\"} 0.45\n\
vllm:prefix_cache_hits_total{model_name=\"m\"} 100\n\
vllm:prefix_cache_queries_total{model_name=\"m\"} 200\n";
        let s = parse_vllm_metrics_text(text, "m", 0, 1);
        assert_eq!(s.pending_requests, 4);
        assert_eq!(s.kv_cache_usage, Some(0.45));
        assert_eq!(s.prefix_cache_hit_rate, Some(0.5));
    }

    #[test]
    fn prefers_direct_hit_rate_gauge() {
        let text = "vllm:gpu_prefix_cache_hit_rate{model_name=\"m\"} 0.8\n";
        let s = parse_vllm_metrics_text(text, "m", 1, 1);
        assert_eq!(s.prefix_cache_hit_rate, Some(0.8));
    }

    #[test]
    fn extract_metric_forms() {
        assert_eq!(
            extract_metric(
                "vllm:num_requests_waiting{model=\"m\"} 3",
                "num_requests_waiting"
            ),
            Some(3.0)
        );
        assert_eq!(
            extract_metric("vllm_gpu_cache_usage_perc{} 0.45", "gpu_cache_usage_perc"),
            Some(0.45)
        );
        assert_eq!(
            extract_metric("unrelated_metric{} 1.0", "num_requests_waiting"),
            None
        );
        // Must NOT match a longer name that merely contains the suffix.
        assert_eq!(
            extract_metric(
                "vllm:external_prefix_cache_hits_total{engine=\"0\"} 0.0",
                "prefix_cache_hits_total"
            ),
            None
        );
    }

    #[test]
    fn ignores_external_prefix_cache_counters() {
        let text = "\
vllm:prefix_cache_queries_total{model_name=\"m\"} 200\n\
vllm:prefix_cache_hits_total{model_name=\"m\"} 100\n\
vllm:external_prefix_cache_queries_total{model_name=\"m\"} 0\n\
vllm:external_prefix_cache_hits_total{model_name=\"m\"} 0\n";
        let s = parse_vllm_metrics_text(text, "m", 0, 1);
        assert_eq!(s.prefix_cache_hit_rate, Some(0.5));
    }
}
