//! Parse Router Prometheus text for SLO evaluation (shared Gateway / BFF).

use nebula_common::{evaluate_slo, ModelSlo, SloEvaluation};

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
    for (bits, cumulative) in by_le {
        let boundary = f64::from_bits(bits);
        if cumulative >= target {
            return boundary;
        }
    }

    0.0
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
    let window_secs = match slo.window.as_str() {
        "5m" => 300.0,
        "15m" => 900.0,
        "1h" => 3600.0,
        "6h" => 21600.0,
        "24h" | "1d" => 86400.0,
        _ => 900.0,
    };
    let req = parse_metric_sum(metrics_text, "nebula_router_requests_total");
    let err5 = parse_metric_sum(metrics_text, "nebula_router_responses_5xx");
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
    evaluate_slo(slo, availability, ttft, latency, request_rate, now_ms())
}
