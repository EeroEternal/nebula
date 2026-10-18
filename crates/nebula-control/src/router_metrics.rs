//! Parse Router Prometheus text for SLO evaluation (shared Gateway / BFF).

use nebula_common::{ModelSlo, SloEvaluation};

use crate::store::now_ms;

fn metric_line_matches(line: &str, metric: &str) -> bool {
    if !line.starts_with(metric) {
        return false;
    }
    matches!(line.as_bytes().get(metric.len()), Some(b' ') | Some(b'{'))
}

pub fn parse_metric_sum(metrics_text: &str, metric: &str) -> f64 {
    metrics_text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter(|line| metric_line_matches(line, metric))
        .filter_map(|line| line.split_whitespace().last())
        .filter_map(|value| value.parse::<f64>().ok())
        .sum()
}

fn extract_label_value(line: &str, label: &str) -> Option<String> {
    let token = format!(r#"{label}=""#);
    let start = line.find(&token)? + token.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

pub fn parse_histogram_quantile_filtered(
    metrics_text: &str,
    metric: &str,
    quantile: f64,
    model_uid: Option<&str>,
) -> f64 {
    let bucket_metric = format!("{metric}_bucket");
    let mut by_le: std::collections::BTreeMap<u64, f64> = std::collections::BTreeMap::new();
    let mut total = 0.0;

    for line in metrics_text.lines().filter(|line| !line.starts_with('#')) {
        if !metric_line_matches(line, &bucket_metric) {
            continue;
        }
        if let Some(uid) = model_uid {
            let Some(label) = extract_label_value(line, "model_uid") else {
                continue;
            };
            if label != uid {
                continue;
            }
        }

        let le = match extract_label_value(line, "le") {
            Some(v) => v,
            None => continue,
        };

        let value = match line
            .split_whitespace()
            .last()
            .and_then(|v| v.parse::<f64>().ok())
        {
            Some(v) => v,
            None => continue,
        };

        if le == "+Inf" {
            total += value;
            continue;
        }

        if let Ok(boundary) = le.parse::<f64>() {
            let key = boundary.to_bits();
            *by_le.entry(key).or_insert(0.0) += value;
        }
    }

    if total <= 0.0 || by_le.is_empty() {
        return 0.0;
    }

    let target = total * quantile.clamp(0.0, 1.0);
    let mut largest: Option<f64> = None;
    for (bits, cumulative) in by_le {
        let boundary = f64::from_bits(bits);
        largest = Some(boundary);
        if cumulative >= target {
            return boundary;
        }
    }

    // The quantile falls inside the `+Inf` bucket (a handful of samples where one
    // outlier exceeds the largest finite bound). Report that bound as a lower bound:
    // returning 0.0 made callers drop the sample entirely ("no TTFT data" instead of
    // "at least 300s").
    largest.unwrap_or(0.0)
}

fn normalize_zero(value: f64) -> f64 {
    if value.abs() < 1e-12 {
        0.0
    } else {
        value
    }
}

/// Evaluate SLO using Router metrics text. Low/no traffic → insufficient_data.
pub fn evaluate_slo_from_router_metrics(slo: &ModelSlo, metrics_text: &str) -> SloEvaluation {
    evaluate_slo_from_router_metrics_with_pools(
        slo,
        metrics_text,
        &nebula_common::PdPoolTelemetry::default(),
    )
}

/// Evaluate SLO using Router metrics text and optional `pd_split` role-pool telemetry (Batch 5.7).
pub fn evaluate_slo_from_router_metrics_with_pools(
    slo: &ModelSlo,
    metrics_text: &str,
    pool_telemetry: &nebula_common::PdPoolTelemetry,
) -> SloEvaluation {
    let window_secs = match slo.window.as_str() {
        "5m" => 300.0,
        "15m" => 900.0,
        "1h" => 3600.0,
        "6h" => 21600.0,
        "24h" | "1d" => 86400.0,
        _ => 900.0,
    };
    let router_req = parse_metric_sum(metrics_text, "nebula_router_requests_total");
    let (req, err5) = if router_req > 0.0 {
        (
            router_req,
            parse_metric_sum(metrics_text, "nebula_router_responses_5xx"),
        )
    } else {
        (
            parse_metric_sum(metrics_text, "nebula_gateway_requests_total"),
            parse_metric_sum(metrics_text, "nebula_gateway_responses_5xx"),
        )
    };
    let availability = if req > 0.0 {
        Some(1.0 - (err5 / req))
    } else {
        None
    };
    let request_rate = if window_secs > 0.0 {
        Some(req / window_secs)
    } else {
        None
    };
    let model = Some(slo.model_uid.as_str());
    let ttft = {
        let s = parse_histogram_quantile_filtered(
            metrics_text,
            "nebula_route_ttft_seconds",
            0.95,
            model,
        );
        if s > 0.0 {
            Some(normalize_zero(s * 1000.0))
        } else {
            None
        }
    };
    let latency = {
        let s = parse_histogram_quantile_filtered(
            metrics_text,
            "nebula_route_latency_seconds",
            0.95,
            model,
        );
        if s > 0.0 {
            Some(normalize_zero(s * 1000.0))
        } else {
            None
        }
    };
    let tpot_from_metrics = {
        let s = parse_histogram_quantile_filtered(
            metrics_text,
            "nebula_route_tpot_seconds",
            0.95,
            model,
        );
        if s > 0.0 {
            Some(normalize_zero(s * 1000.0))
        } else {
            None
        }
    };
    let mut effective_pool_telemetry = pool_telemetry.clone();
    if effective_pool_telemetry.tpot_p95_ms.is_none() {
        effective_pool_telemetry.tpot_p95_ms = tpot_from_metrics;
    }
    nebula_common::evaluate_slo_with_pools(
        slo,
        availability,
        ttft,
        latency,
        request_rate,
        &effective_pool_telemetry,
        now_ms(),
    )
}

/// Load `PdPoolTelemetry` (`is_pd_split`, `prefill_pending_max`, `decode_avg_kv_usage`)
/// from etcd `/deployments/`, `/endpoints/`, and `/stats/` for a model.
pub async fn load_pd_pool_telemetry(
    store: &dyn nebula_meta::MetaStore,
    model_uid: &str,
) -> nebula_common::PdPoolTelemetry {
    use nebula_common::{
        DeploymentTopology, EndpointInfo, EndpointStats, ModelDeployment, ReplicaRole,
    };

    let dep_is_pd = match store.get(&format!("/deployments/{model_uid}")).await {
        Ok(Some((bytes, _))) => serde_json::from_slice::<ModelDeployment>(&bytes)
            .ok()
            .and_then(|d| d.topology)
            .map(|t| matches!(t, DeploymentTopology::PdSplit))
            .unwrap_or(false),
        _ => false,
    };

    let eps: Vec<EndpointInfo> = store
        .list_prefix("/endpoints/")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice::<EndpointInfo>(&v).ok())
        .filter(|e| e.model_uid == model_uid)
        .collect();

    let stats: Vec<EndpointStats> = store
        .list_prefix("/stats/")
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(_, v, _)| serde_json::from_slice::<EndpointStats>(&v).ok())
        .filter(|s| s.model_uid == model_uid)
        .collect();

    let is_pd_split = dep_is_pd || eps.iter().any(|e| e.role != ReplicaRole::Mixed);
    let prefill_pending_max = eps
        .iter()
        .filter(|e| e.role == ReplicaRole::Prefill)
        .filter_map(|e| stats.iter().find(|s| s.replica_id == e.replica_id))
        .map(|s| s.pending_requests)
        .max();
    let decode_kvs: Vec<f64> = eps
        .iter()
        .filter(|e| e.role == ReplicaRole::Decode)
        .filter_map(|e| stats.iter().find(|s| s.replica_id == e.replica_id))
        .filter_map(|s| s.kv_cache_usage)
        .collect();
    let decode_avg_kv_usage = if decode_kvs.is_empty() {
        None
    } else {
        Some(decode_kvs.iter().sum::<f64>() / decode_kvs.len() as f64)
    };

    nebula_common::PdPoolTelemetry {
        is_pd_split,
        tpot_p95_ms: None,
        prefill_pending_max,
        decode_avg_kv_usage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_common::{PdPoolTelemetry, SloComplianceStatus};

    #[test]
    fn parses_tpot_histogram_from_router_metrics_for_pd_split_evaluation() {
        let slo = ModelSlo {
            model_uid: "m1".into(),
            availability_target: Some(0.99),
            ttft_p95_ms: Some(1000.0),
            tpot_p95_ms: Some(40.0),
            latency_p95_ms: Some(10000.0),
            throughput_tps: None,
            window: "5m".into(),
            exclude_abort_from_error_budget: true,
            exclude_drain_from_error_budget: true,
            notes: None,
            updated_at_ms: 1,
        };
        let metrics = "\
nebula_router_requests_total 500\n\
nebula_router_responses_5xx 0\n\
nebula_route_ttft_seconds_bucket{model_uid=\"m1\",le=\"0.2\"} 500\n\
nebula_route_ttft_seconds_bucket{model_uid=\"m1\",le=\"+Inf\"} 500\n\
nebula_route_latency_seconds_bucket{model_uid=\"m1\",le=\"2.0\"} 500\n\
nebula_route_latency_seconds_bucket{model_uid=\"m1\",le=\"+Inf\"} 500\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"0.02\"} 10\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"0.08\"} 500\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"+Inf\"} 500\n";
        let ev = evaluate_slo_from_router_metrics_with_pools(
            &slo,
            metrics,
            &PdPoolTelemetry {
                is_pd_split: true,
                tpot_p95_ms: None,
                prefill_pending_max: Some(0),
                decode_avg_kv_usage: Some(0.50),
            },
        );
        assert_eq!(ev.status, SloComplianceStatus::Breaching);
        assert!(ev.breaches.iter().any(|b| b.contains("tpot_p95")));
        assert!(ev.suggestions.iter().any(|s| s.kind == "scale_decode_pool"));
    }

    #[test]
    fn quantile_landing_in_inf_bucket_reports_largest_finite_bound() {
        // 9 of 10 samples fall in the finite buckets, one exceeds the largest finite
        // bound: p95 lives in `+Inf`. Reporting the largest finite bound keeps the
        // sample visible; returning 0.0 made callers report "no data" instead.
        let metrics = "\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"0.02\"} 8\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"0.05\"} 9\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"+Inf\"} 10\n";
        assert_eq!(
            parse_histogram_quantile_filtered(
                metrics,
                "nebula_route_tpot_seconds",
                0.95,
                Some("m1")
            ),
            0.05
        );

        // No samples at all still reports "no data", not a fabricated bound.
        assert_eq!(
            parse_histogram_quantile_filtered("", "nebula_route_tpot_seconds", 0.95, Some("m1")),
            0.0
        );
    }

    #[test]
    fn evaluates_slo_from_embedded_gateway_metrics() {
        let slo = ModelSlo {
            model_uid: "m1".into(),
            availability_target: Some(0.99),
            ttft_p95_ms: Some(1000.0),
            tpot_p95_ms: Some(40.0),
            latency_p95_ms: Some(10000.0),
            throughput_tps: None,
            window: "5m".into(),
            exclude_abort_from_error_budget: true,
            exclude_drain_from_error_budget: true,
            notes: None,
            updated_at_ms: 1,
        };
        let metrics = "\
nebula_gateway_requests_total 500\n\
nebula_gateway_responses_5xx 0\n\
nebula_route_ttft_seconds_bucket{model_uid=\"m1\",le=\"0.2\"} 500\n\
nebula_route_ttft_seconds_bucket{model_uid=\"m1\",le=\"+Inf\"} 500\n\
nebula_route_latency_seconds_bucket{model_uid=\"m1\",le=\"2.0\"} 500\n\
nebula_route_latency_seconds_bucket{model_uid=\"m1\",le=\"+Inf\"} 500\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"0.02\"} 10\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"0.075\"} 500\n\
nebula_route_tpot_seconds_bucket{model_uid=\"m1\",le=\"+Inf\"} 500\n";
        let ev = evaluate_slo_from_router_metrics_with_pools(
            &slo,
            metrics,
            &PdPoolTelemetry {
                is_pd_split: true,
                tpot_p95_ms: None,
                prefill_pending_max: Some(0),
                decode_avg_kv_usage: Some(0.50),
            },
        );
        assert!(!ev.breaches.iter().any(|b| b.contains("availability")));
        assert_eq!(ev.status, SloComplianceStatus::Breaching);
        assert!(ev.breaches.iter().any(|b| b.contains("tpot_p95")));
        assert!(ev.suggestions.iter().any(|s| s.kind == "scale_decode_pool"));
    }
}
