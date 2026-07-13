//! Best-effort allowlist parser for Serving Cell Ingress `/metrics`.
//!
//! Used by BFF for read-only observation. Missing fields stay `None` (UI: n/a);
//! never invent worker counts or treat absence as zero.

use serde::{Deserialize, Serialize};

/// Outcome of scraping a Cell Ingress metrics / health endpoint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CellScrapeStatus {
    Ok,
    Unreachable,
    HttpError,
    Empty,
    Skipped,
}

/// Ingress-level stats snapshot. Optional fields are unsupported / missing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CellIngressStats {
    pub scraped_at_ms: u64,
    pub metrics_url: String,
    /// Always `cell_ingress` — never mixed with Router/Node series.
    pub data_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_requests: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kv_cache_usage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_cache_hit_rate: Option<f64>,
    pub scrape_status: CellScrapeStatus,
}

impl CellIngressStats {
    pub fn skipped(metrics_url: String, scraped_at_ms: u64) -> Self {
        Self {
            scraped_at_ms,
            metrics_url,
            data_source: "cell_ingress".into(),
            pending_requests: None,
            kv_cache_usage: None,
            prefix_cache_hit_rate: None,
            scrape_status: CellScrapeStatus::Skipped,
        }
    }
}

/// Parse Prometheus text from a Cell Ingress (vLLM Router / SGLang Gateway / engine).
///
/// Allowlist only: waiting/running requests, KV/cache usage, prefix hit rate.
/// Does not invent Prefill/Decode or worker pool sizes.
pub fn parse_cell_ingress_metrics(text: &str, metrics_url: &str, scraped_at_ms: u64) -> CellIngressStats {
    let mut waiting: Option<u64> = None;
    let mut running: Option<u64> = None;
    let mut kv_cache_usage: Option<f64> = None;
    let mut prefix_cache_hit_rate: Option<f64> = None;
    let mut prefix_hits: Option<f64> = None;
    let mut prefix_queries: Option<f64> = None;
    let mut saw_any = false;

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        if let Some(val) = extract_metric(line, "num_requests_waiting") {
            waiting = Some(val as u64);
            saw_any = true;
        } else if let Some(val) = extract_metric(line, "num_requests_running") {
            running = Some(val as u64);
            saw_any = true;
        }

        if let Some(val) = extract_metric(line, "kv_cache_usage_perc") {
            kv_cache_usage = Some(val);
            saw_any = true;
        } else if kv_cache_usage.is_none() {
            if let Some(val) = extract_metric(line, "gpu_cache_usage_perc") {
                kv_cache_usage = Some(val);
                saw_any = true;
            } else if let Some(val) = extract_metric(line, "token_usage") {
                kv_cache_usage = Some(val);
                saw_any = true;
            } else if let Some(val) = extract_metric(line, "cache_usage") {
                kv_cache_usage = Some(val);
                saw_any = true;
            }
        }

        if let Some(val) = extract_metric(line, "gpu_prefix_cache_hit_rate") {
            prefix_cache_hit_rate = Some(val);
            saw_any = true;
        } else if prefix_cache_hit_rate.is_none() {
            if let Some(val) = extract_metric(line, "cpu_prefix_cache_hit_rate") {
                prefix_cache_hit_rate = Some(val);
                saw_any = true;
            }
        }
        if let Some(val) = extract_metric(line, "prefix_cache_hits_total") {
            prefix_hits = Some(val);
            saw_any = true;
        }
        if let Some(val) = extract_metric(line, "prefix_cache_queries_total") {
            prefix_queries = Some(val);
            saw_any = true;
        }
    }

    if prefix_cache_hit_rate.is_none() {
        if let (Some(hits), Some(queries)) = (prefix_hits, prefix_queries) {
            if queries > 0.0 {
                prefix_cache_hit_rate = Some(hits / queries);
            }
        }
    }

    let pending_requests = match (waiting, running) {
        (None, None) => None,
        (w, r) => Some(w.unwrap_or(0).saturating_add(r.unwrap_or(0))),
    };

    let scrape_status = if saw_any
        || pending_requests.is_some()
        || kv_cache_usage.is_some()
        || prefix_cache_hit_rate.is_some()
    {
        CellScrapeStatus::Ok
    } else if text.trim().is_empty() {
        CellScrapeStatus::Empty
    } else {
        // Reachable body but no allowlisted series — still Ok at transport level;
        // fields remain None so UI shows n/a.
        CellScrapeStatus::Ok
    };

    CellIngressStats {
        scraped_at_ms,
        metrics_url: metrics_url.to_string(),
        data_source: "cell_ingress".into(),
        pending_requests,
        kv_cache_usage,
        prefix_cache_hit_rate,
        scrape_status,
    }
}

fn extract_metric(line: &str, metric_suffix: &str) -> Option<f64> {
    let has_metric =
        line.contains(&format!(":{metric_suffix}")) || line.contains(&format!("_{metric_suffix}"));
    if !has_metric {
        return None;
    }
    let value_str = line.rsplit_once(|c: char| c.is_whitespace())?.1;
    value_str.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vllm_style_ingress() {
        let text = r#"
# HELP vllm:num_requests_waiting Waiting
vllm:num_requests_waiting{engine="0"} 2
vllm:num_requests_running{engine="0"} 1
vllm:kv_cache_usage_perc{engine="0"} 0.42
"#;
        let s = parse_cell_ingress_metrics(text, "http://gw/metrics", 100);
        assert_eq!(s.pending_requests, Some(3));
        assert!((s.kv_cache_usage.unwrap() - 0.42).abs() < 1e-9);
        assert_eq!(s.data_source, "cell_ingress");
        assert_eq!(s.scrape_status, CellScrapeStatus::Ok);
    }

    #[test]
    fn parses_sglang_token_usage() {
        let text = "sglang:token_usage{model=\"m\"} 0.55\n";
        let s = parse_cell_ingress_metrics(text, "http://gw/metrics", 1);
        assert_eq!(s.pending_requests, None);
        assert!((s.kv_cache_usage.unwrap() - 0.55).abs() < 1e-9);
    }

    #[test]
    fn missing_fields_stay_none() {
        let s = parse_cell_ingress_metrics("unrelated_metric 1\n", "http://gw/metrics", 1);
        assert_eq!(s.pending_requests, None);
        assert_eq!(s.kv_cache_usage, None);
        assert_eq!(s.scrape_status, CellScrapeStatus::Ok);
    }
}
