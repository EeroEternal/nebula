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
    }

    let diag = parse_engine_scrape_diagnostics(Some("vllm"), text);
    // Compute prefix cache hit rate from counters if no direct gauge was found
    if prefix_cache_hit_rate.is_none() {
        if let (Some(hits), Some(queries)) = (
            diag.prefix_cache_hits_total,
            diag.prefix_cache_queries_total,
        ) {
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
        prefix_cache_queries_total: diag.prefix_cache_queries_total,
    }
}

/// Extract a numeric value from a Prometheus metric line, matching the metric
/// name *exactly* (full name before `{`/whitespace, allowing a `vllm:`/`vllm_`
/// namespace prefix). Exact matching avoids e.g. `prefix_cache_hits_total`
/// matching `external_prefix_cache_hits_total`.
fn extract_metric(line: &str, metric_suffix: &str) -> Option<f64> {
    extract_ns_metric(line, "vllm", metric_suffix)
}

/// Same as [`extract_metric`] but for the SGLang namespace (`sglang:`/`sglang_`).
fn extract_sglang_metric(line: &str, metric_suffix: &str) -> Option<f64> {
    extract_ns_metric(line, "sglang", metric_suffix)
}

/// Match a Prometheus metric line whose name is exactly `suffix`, optionally
/// prefixed by `{namespace}:` or `{namespace}_`.
fn extract_ns_metric(line: &str, namespace: &str, metric_suffix: &str) -> Option<f64> {
    let name_end = line.find(|c: char| c == '{' || c.is_whitespace())?;
    let name = &line[..name_end];
    let matched = name == metric_suffix
        || name == format!("{namespace}:{metric_suffix}")
        || name == format!("{namespace}_{metric_suffix}");
    if !matched {
        return None;
    }

    let value_str = line.rsplit_once(|c: char| c.is_whitespace())?.1;
    value_str.parse::<f64>().ok()
}

/// Parse SGLang `/metrics` into `EndpointStats`.
///
/// SGLang exposes `sglang:num_requests_waiting` / `num_requests_running` and
/// `sglang:token_usage` (KV cache usage ratio).
pub fn parse_sglang_metrics_text(
    text: &str,
    model_uid: &str,
    replica_id: u32,
    last_updated_ms: u64,
) -> EndpointStats {
    let mut pending_requests: u64 = 0;
    let mut running_requests: u64 = 0;
    let mut kv_cache_usage: Option<f64> = None;

    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }

        if let Some(val) = extract_sglang_metric(line, "num_requests_waiting") {
            pending_requests = val as u64;
        } else if let Some(val) = extract_sglang_metric(line, "num_requests_running") {
            running_requests = val as u64;
        } else if let Some(val) = extract_sglang_metric(line, "token_usage") {
            kv_cache_usage = Some(val);
        } else if kv_cache_usage.is_none() {
            if let Some(val) = extract_sglang_metric(line, "cache_usage") {
                kv_cache_usage = Some(val);
            }
        }
    }

    let diag = parse_engine_scrape_diagnostics(Some("sglang"), text);
    let prefix_cache_hit_rate = match (
        diag.prefix_cache_hits_total,
        diag.prefix_cache_queries_total,
    ) {
        (Some(hits), Some(queries)) if queries > 0.0 => Some(hits / queries),
        _ => None,
    };

    EndpointStats {
        model_uid: model_uid.to_string(),
        replica_id,
        last_updated_ms,
        pending_requests: pending_requests + running_requests,
        prefix_cache_hit_rate,
        prompt_cache_hit_rate: None,
        kv_cache_usage,
        prefix_cache_queries_total: diag.prefix_cache_queries_total,
    }
}

/// Dispatch `/metrics` parsing by engine type (`sglang` vs vLLM default), so both
/// execution planes (node / k8s-controller) treat engines uniformly.
pub fn parse_engine_metrics(
    engine_type: Option<&str>,
    text: &str,
    model_uid: &str,
    replica_id: u32,
    last_updated_ms: u64,
) -> EndpointStats {
    match engine_type.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("sglang") => parse_sglang_metrics_text(text, model_uid, replica_id, last_updated_ms),
        _ => parse_vllm_metrics_text(text, model_uid, replica_id, last_updated_ms),
    }
}

/// Extended engine scrape diagnostics (Batch 5.6): speculative decoding mean
/// acceptance length ($\tau$, the true speedup metric rather than raw acceptance
/// rate) and `prefix_cache_queries_total` for detecting silent prefix-cache
/// disablement on hybrid-attention models (e.g. `Qwen3.8` `is_hybrid=True`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EngineScrapeDiagnostics {
    pub prefix_cache_queries_total: Option<f64>,
    pub prefix_cache_hits_total: Option<f64>,
    /// Mean accepted tokens per forward pass ($\tau \ge 1.0$, e.g. `1.0 + accepted / drafts`).
    pub spec_decode_mean_acceptance_length: Option<f64>,
}

/// Parse speculative-decoding $\tau$ and prefix-cache query counters from a
/// vLLM or SGLang `/metrics` body.
pub fn parse_engine_scrape_diagnostics(
    engine_type: Option<&str>,
    text: &str,
) -> EngineScrapeDiagnostics {
    let is_sglang = matches!(
        engine_type.map(|s| s.to_ascii_lowercase()).as_deref(),
        Some("sglang")
    );
    let ns = if is_sglang { "sglang" } else { "vllm" };

    let mut queries: Option<f64> = None;
    let mut hits: Option<f64> = None;
    let mut mean_tau: Option<f64> = None;
    let mut accepted_tokens: Option<f64> = None;
    let mut draft_rounds: Option<f64> = None;

    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        if let Some(v) = extract_ns_metric(line, ns, "prefix_cache_queries_total") {
            queries = Some(v);
        } else if let Some(v) = extract_ns_metric(line, ns, "prefix_cache_hits_total") {
            hits = Some(v);
        } else if let Some(v) = extract_ns_metric(line, ns, "spec_decode_mean_acceptance_length")
            .or_else(|| extract_ns_metric(line, ns, "spec_accept_length"))
        {
            mean_tau = Some(v);
        } else if let Some(v) = extract_ns_metric(line, ns, "spec_decode_num_accepted_tokens_total")
        {
            accepted_tokens = Some(v);
        } else if let Some(v) = extract_ns_metric(line, ns, "spec_decode_num_drafts_total")
            .or_else(|| extract_ns_metric(line, ns, "spec_decode_draft_rounds_total"))
        {
            draft_rounds = Some(v);
        }
    }

    if mean_tau.is_none() {
        if let (Some(acc), Some(rounds)) = (accepted_tokens, draft_rounds) {
            if rounds > 0.0 {
                mean_tau = Some(1.0 + (acc / rounds));
            }
        }
    }

    EngineScrapeDiagnostics {
        prefix_cache_queries_total: queries,
        prefix_cache_hits_total: hits,
        spec_decode_mean_acceptance_length: mean_tau,
    }
}

/// Emit a `DiagnosticEvent` when a model expects prefix caching (`prefix_caching_expected`)
/// and is actively serving (`active_traffic`), yet the scraped `prefix_cache_queries_total`
/// counter is explicitly `Some(0.0)` while `prefix_cache_hit_rate` is `None`
/// (the vLLM `is_hybrid=True` silent prefix-cache disablement trap).
///
/// Never fires when `prefix_cache_queries_total` is `None` (e.g. SGLang or engines
/// that do not expose `prefix_cache_queries_total`).
pub fn diagnose_hybrid_prefix_cache(
    model_uid: &str,
    replica_id: u32,
    prefix_caching_expected: bool,
    active_traffic: bool,
    prefix_cache_hit_rate: Option<f64>,
    prefix_cache_queries_total: Option<f64>,
    ts_ms: u64,
) -> Option<crate::slo::DiagnosticEvent> {
    if !prefix_caching_expected || !active_traffic {
        return None;
    }
    let silently_disabled = match prefix_cache_queries_total {
        Some(q) => q <= 0.0 && prefix_cache_hit_rate.is_none(),
        None => false,
    };
    if !silently_disabled {
        return None;
    }
    Some(crate::slo::DiagnosticEvent {
        ts_ms,
        kind: "scrape".into(),
        summary: format!(
            "replica {replica_id} has prefix_cache_queries=0 under active load: hybrid-attention model may have prefix caching silently disabled; set enable_prefix_caching=true"
        ),
        model_uid: Some(model_uid.to_string()),
        node_id: None,
        data_source: Some("engine".into()),
    })
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

    #[test]
    fn parses_sglang_metrics() {
        let text = "\
sglang:num_requests_waiting{model=\"m\"} 3\n\
sglang:num_requests_running{model=\"m\"} 1\n\
sglang:token_usage{model=\"m\"} 0.45\n";
        let s = parse_sglang_metrics_text(text, "m", 0, 1);
        assert_eq!(s.pending_requests, 4);
        assert_eq!(s.kv_cache_usage, Some(0.45));
        assert_eq!(s.prefix_cache_hit_rate, None);
    }

    #[test]
    fn dispatch_by_engine() {
        let v = "vllm:num_requests_waiting{} 2\n";
        let sg = "sglang:num_requests_waiting{} 5\n";
        assert_eq!(
            parse_engine_metrics(Some("vllm"), v, "m", 0, 1).pending_requests,
            2
        );
        assert_eq!(
            parse_engine_metrics(Some("sglang"), sg, "m", 0, 1).pending_requests,
            5
        );
        // Unknown engine falls back to vLLM.
        assert_eq!(parse_engine_metrics(None, v, "m", 0, 1).pending_requests, 2);
    }

    #[test]
    fn parses_spec_decode_mean_acceptance_tau_and_detects_silent_hybrid_prefix_cache_trap() {
        let text = "\
vllm:prefix_cache_queries_total{model=\"qwen3.8\"} 0\n\
vllm:prefix_cache_hits_total{model=\"qwen3.8\"} 0\n\
vllm:spec_decode_num_accepted_tokens_total{model=\"qwen3.8\"} 1400\n\
vllm:spec_decode_num_drafts_total{model=\"qwen3.8\"} 100\n";
        let diag = parse_engine_scrape_diagnostics(Some("vllm"), text);
        assert_eq!(diag.prefix_cache_queries_total, Some(0.0));
        // tau = 1 + 1400/100 = 15.0 tokens per verification pass
        assert_eq!(diag.spec_decode_mean_acceptance_length, Some(15.0));

        let ev = diagnose_hybrid_prefix_cache(
            "qwen3.8-27b",
            0,
            true,
            true,
            None,
            diag.prefix_cache_queries_total,
            1000,
        )
        .expect("should emit hybrid prefix cache diagnostic event");
        assert_eq!(ev.kind, "scrape");
        assert!(ev.summary.contains("enable_prefix_caching=true"));

        // Engines that do not expose prefix_cache_queries_total (e.g. SGLang) must NOT trigger false positives.
        assert!(
            diagnose_hybrid_prefix_cache("sglang-model", 0, true, true, None, None, 1000).is_none()
        );
    }
}
