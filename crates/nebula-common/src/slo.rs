//! Model-level SLO objects and evaluation results (Product P4).

use serde::{Deserialize, Serialize};

/// Thresholds for a model's service objectives.
///
/// etcd key: `/slos/{model_uid}`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelSlo {
    pub model_uid: String,
    /// Availability as fraction of non-5xx responses, e.g. `0.99`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_target: Option<f64>,
    /// TTFT p95 target in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_p95_ms: Option<f64>,
    /// TPOT / inter-token p95 target in milliseconds (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tpot_p95_ms: Option<f64>,
    /// End-to-end latency p95 target in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_p95_ms: Option<f64>,
    /// Optional minimum successful tokens/sec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput_tps: Option<f64>,
    /// Evaluation window label, e.g. `"15m"`, `"1h"`.
    #[serde(default = "default_window")]
    pub window: String,
    /// Abort / client disconnect must not count toward 5xx error budget.
    #[serde(default = "default_true")]
    pub exclude_abort_from_error_budget: bool,
    /// Active Drain must not count toward 5xx error budget.
    #[serde(default = "default_true")]
    pub exclude_drain_from_error_budget: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub updated_at_ms: u64,
}

fn default_window() -> String {
    "15m".into()
}

fn default_true() -> bool {
    true
}

/// Outcome of comparing live (or scraped) SLIs against a ModelSlo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SloComplianceStatus {
    Compliant,
    Breaching,
    /// Not enough traffic / missing scrape — never invent compliance.
    InsufficientData,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SloMetricSample {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// `router` | `gateway` | `engine` | `node`
    pub data_source: String,
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SloEvaluation {
    pub model_uid: String,
    pub window: String,
    pub status: SloComplianceStatus,
    pub samples: Vec<SloMetricSample>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaches: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<SloSuggestion>,
    pub evaluated_at_ms: u64,
    /// Always label whether abort/drain were excluded from the error-budget view.
    pub abort_excluded: bool,
    pub drain_excluded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SloSuggestion {
    pub kind: String,
    pub message: String,
    /// Scale / observe suggestions target ordinary replicas.
    pub target: String,
}

/// Unified diagnostic timeline event (P4 aggregation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticEvent {
    pub ts_ms: u64,
    /// `deployment` | `placement` | `audit` | `scrape` | `slo`
    pub kind: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source: Option<String>,
}

/// Evaluate availability / latency samples against SLO thresholds.
/// Missing samples → `InsufficientData` (never fake green).
pub fn evaluate_slo(
    slo: &ModelSlo,
    availability: Option<f64>,
    ttft_p95_ms: Option<f64>,
    latency_p95_ms: Option<f64>,
    request_rate: Option<f64>,
    now_ms: u64,
) -> SloEvaluation {
    let mut samples = vec![
        SloMetricSample {
            name: "availability".into(),
            value: availability,
            data_source: "router".into(),
            unit: "ratio".into(),
        },
        SloMetricSample {
            name: "ttft_p95".into(),
            value: ttft_p95_ms,
            data_source: "router".into(),
            unit: "ms".into(),
        },
        SloMetricSample {
            name: "latency_p95".into(),
            value: latency_p95_ms,
            data_source: "router".into(),
            unit: "ms".into(),
        },
        SloMetricSample {
            name: "request_rate".into(),
            value: request_rate,
            data_source: "router".into(),
            unit: "rps".into(),
        },
    ];

    let low_traffic = request_rate.map(|r| r < 0.1).unwrap_or(true);
    let mut breaches = Vec::new();
    let mut avail_breach: Option<(f64, f64)> = None;
    let mut ttft_breach: Option<(f64, f64)> = None;
    let mut latency_breach: Option<(f64, f64)> = None;

    if low_traffic {
        let rate_note = request_rate
            .map(|r| format!("request_rate={r:.3}"))
            .unwrap_or_else(|| "request_rate=missing".into());
        return SloEvaluation {
            model_uid: slo.model_uid.clone(),
            window: slo.window.clone(),
            status: SloComplianceStatus::InsufficientData,
            samples,
            breaches,
            suggestions: vec![SloSuggestion {
                kind: "observe".into(),
                message: format!(
                    "insufficient traffic to evaluate SLO ({rate_note}); wait for request_rate >= 0.1 before treating status as compliant or breaching"
                ),
                target: "replica".into(),
            }],
            evaluated_at_ms: now_ms,
            abort_excluded: slo.exclude_abort_from_error_budget,
            drain_excluded: slo.exclude_drain_from_error_budget,
        };
    }

    if let (Some(target), Some(actual)) = (slo.availability_target, availability) {
        if actual + f64::EPSILON < target {
            breaches.push(format!(
                "availability {actual:.4} < target {target:.4} (5xx budget; abort/drain excluded)"
            ));
            avail_breach = Some((actual, target));
        }
    }
    if let (Some(target), Some(actual)) = (slo.ttft_p95_ms, ttft_p95_ms) {
        if actual > target {
            breaches.push(format!("ttft_p95 {actual:.1}ms > target {target:.1}ms"));
            ttft_breach = Some((actual, target));
        }
    }
    if let (Some(target), Some(actual)) = (slo.latency_p95_ms, latency_p95_ms) {
        if actual > target {
            breaches.push(format!("latency_p95 {actual:.1}ms > target {target:.1}ms"));
            latency_breach = Some((actual, target));
        }
    }

    // If required metrics absent entirely → insufficient, not compliant.
    let mut missing = Vec::new();
    if slo.availability_target.is_some() && availability.is_none() {
        missing.push("availability");
    }
    if slo.ttft_p95_ms.is_some() && ttft_p95_ms.is_none() {
        missing.push("ttft_p95");
    }
    if slo.latency_p95_ms.is_some() && latency_p95_ms.is_none() {
        missing.push("latency_p95");
    }
    if !missing.is_empty() && breaches.is_empty() {
        samples.push(SloMetricSample {
            name: "note".into(),
            value: None,
            data_source: "router".into(),
            unit: "".into(),
        });
        return SloEvaluation {
            model_uid: slo.model_uid.clone(),
            window: slo.window.clone(),
            status: SloComplianceStatus::InsufficientData,
            samples,
            breaches,
            suggestions: vec![SloSuggestion {
                kind: "observe".into(),
                message: format!(
                    "required SLI missing ({}); do not treat as compliant — check router scrape / metrics export",
                    missing.join(", ")
                ),
                target: "replica".into(),
            }],
            evaluated_at_ms: now_ms,
            abort_excluded: slo.exclude_abort_from_error_budget,
            drain_excluded: slo.exclude_drain_from_error_budget,
        };
    }

    let status = if breaches.is_empty() {
        SloComplianceStatus::Compliant
    } else {
        SloComplianceStatus::Breaching
    };

    let suggestions = build_breach_suggestions(avail_breach, ttft_breach, latency_breach);

    SloEvaluation {
        model_uid: slo.model_uid.clone(),
        window: slo.window.clone(),
        status,
        samples,
        breaches,
        suggestions,
        evaluated_at_ms: now_ms,
        abort_excluded: slo.exclude_abort_from_error_budget,
        drain_excluded: slo.exclude_drain_from_error_budget,
    }
}

/// Stable suggestion kinds: `observe` | `scale` | `check_endpoints` | `review_load`.
fn build_breach_suggestions(
    avail: Option<(f64, f64)>,
    ttft: Option<(f64, f64)>,
    latency: Option<(f64, f64)>,
) -> Vec<SloSuggestion> {
    let mut out = Vec::new();

    if let Some((actual, target)) = avail {
        out.push(SloSuggestion {
            kind: "check_endpoints".into(),
            message: format!(
                "availability {actual:.4} below target {target:.4}: inspect unhealthy /endpoints and non-abort 5xx (abort/drain already excluded from budget)"
            ),
            target: "replica".into(),
        });
        out.push(SloSuggestion {
            kind: "scale".into(),
            message: format!(
                "if endpoints are healthy but still burning error budget (availability {actual:.4} < {target:.4}), consider adding replicas or reducing bad traffic"
            ),
            target: "replica".into(),
        });
    }

    if let Some((actual, target)) = ttft {
        out.push(SloSuggestion {
            kind: "review_load".into(),
            message: format!(
                "ttft_p95 {actual:.1}ms > target {target:.1}ms: check replica queueing, GPU saturation, and concurrency vs capacity"
            ),
            target: "replica".into(),
        });
        out.push(SloSuggestion {
            kind: "check_endpoints".into(),
            message: format!(
                "ttft_p95 {actual:.1}ms > {target:.1}ms: confirm ready endpoints and that slow/unhealthy replicas are not still receiving traffic"
            ),
            target: "replica".into(),
        });
    }

    if let Some((actual, target)) = latency {
        out.push(SloSuggestion {
            kind: "review_load".into(),
            message: format!(
                "latency_p95 {actual:.1}ms > target {target:.1}ms: review end-to-end load, max tokens, and downstream engine backlog"
            ),
            target: "replica".into(),
        });
        out.push(SloSuggestion {
            kind: "check_endpoints".into(),
            message: format!(
                "latency_p95 {actual:.1}ms > {target:.1}ms: verify endpoint health and whether Drain/slow replicas skew the window"
            ),
            target: "replica".into(),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slo() -> ModelSlo {
        ModelSlo {
            model_uid: "m1".into(),
            availability_target: Some(0.99),
            ttft_p95_ms: Some(2000.0),
            tpot_p95_ms: None,
            latency_p95_ms: Some(30000.0),
            throughput_tps: None,
            window: "15m".into(),
            exclude_abort_from_error_budget: true,
            exclude_drain_from_error_budget: true,
            notes: None,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn low_traffic_is_insufficient() {
        let ev = evaluate_slo(&slo(), Some(1.0), Some(100.0), Some(1000.0), Some(0.01), 1);
        assert_eq!(ev.status, SloComplianceStatus::InsufficientData);
        assert!(ev.suggestions.iter().any(|s| s.kind == "observe"));
        assert_ne!(ev.status, SloComplianceStatus::Compliant);
    }

    #[test]
    fn breach_availability() {
        let ev = evaluate_slo(&slo(), Some(0.90), Some(100.0), Some(1000.0), Some(5.0), 1);
        assert_eq!(ev.status, SloComplianceStatus::Breaching);
        assert!(!ev.breaches.is_empty());
        assert!(ev.suggestions.iter().any(|s| s.kind == "check_endpoints"));
        assert!(ev.suggestions.iter().any(|s| s.kind == "scale"));
        assert!(ev.suggestions.iter().any(|s| s.message.contains("0.9000")));
    }

    #[test]
    fn breach_ttft_suggests_review_load() {
        let ev = evaluate_slo(&slo(), Some(1.0), Some(5000.0), Some(1000.0), Some(5.0), 1);
        assert_eq!(ev.status, SloComplianceStatus::Breaching);
        assert!(ev.suggestions.iter().any(|s| s.kind == "review_load"));
        assert!(ev.suggestions.iter().any(|s| s.kind == "check_endpoints"));
        assert!(ev.suggestions.iter().any(|s| s.message.contains("ttft_p95")));
        assert!(!ev.suggestions.iter().any(|s| s.message.to_lowercase().contains("selection")));
    }

    #[test]
    fn missing_metrics_observe_not_compliant() {
        let ev = evaluate_slo(&slo(), None, None, None, Some(5.0), 1);
        assert_eq!(ev.status, SloComplianceStatus::InsufficientData);
        assert!(ev.suggestions.iter().any(|s| s.kind == "observe"));
        assert!(ev.suggestions[0].message.contains("missing"));
    }
}
