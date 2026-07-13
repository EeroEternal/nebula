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
    /// `router` | `gateway` | `engine` | `node` | `cell_ingress`
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
    /// `replica` suggestions may imply scale; `cell` must be capacity/config only.
    pub target: String,
}

/// Unified diagnostic timeline event (P4 aggregation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticEvent {
    pub ts_ms: u64,
    /// `deployment` | `placement` | `audit` | `cell` | `scrape` | `slo`
    pub kind: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
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
    is_cell: bool,
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

    if low_traffic {
        return SloEvaluation {
            model_uid: slo.model_uid.clone(),
            window: slo.window.clone(),
            status: SloComplianceStatus::InsufficientData,
            samples,
            breaches,
            suggestions: vec![SloSuggestion {
                kind: "observe".into(),
                message: "insufficient traffic to evaluate SLO; wait for request_rate >= 0.1"
                    .into(),
                target: if is_cell { "cell" } else { "replica" }.into(),
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
        }
    }
    if let (Some(target), Some(actual)) = (slo.ttft_p95_ms, ttft_p95_ms) {
        if actual > target {
            breaches.push(format!("ttft_p95 {actual:.1}ms > target {target:.1}ms"));
        }
    }
    if let (Some(target), Some(actual)) = (slo.latency_p95_ms, latency_p95_ms) {
        if actual > target {
            breaches.push(format!("latency_p95 {actual:.1}ms > target {target:.1}ms"));
        }
    }

    // If required metrics absent entirely → insufficient, not compliant.
    let needed_missing = (slo.availability_target.is_some() && availability.is_none())
        || (slo.ttft_p95_ms.is_some() && ttft_p95_ms.is_none())
        || (slo.latency_p95_ms.is_some() && latency_p95_ms.is_none());
    if needed_missing && breaches.is_empty() {
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
            suggestions: vec![],
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

    let mut suggestions = Vec::new();
    if status == SloComplianceStatus::Breaching {
        if is_cell {
            suggestions.push(SloSuggestion {
                kind: "capacity".into(),
                message: "Serving Cell breach: review ingress capacity/config; do not scale P/D workers via Nebula"
                    .into(),
                target: "cell".into(),
            });
        } else {
            suggestions.push(SloSuggestion {
                kind: "scale".into(),
                message: "Consider increasing replicas or checking unhealthy endpoints".into(),
                target: "replica".into(),
            });
        }
    }

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
        let ev = evaluate_slo(&slo(), Some(1.0), Some(100.0), Some(1000.0), Some(0.01), 1, false);
        assert_eq!(ev.status, SloComplianceStatus::InsufficientData);
    }

    #[test]
    fn breach_availability() {
        let ev = evaluate_slo(&slo(), Some(0.90), Some(100.0), Some(1000.0), Some(5.0), 1, false);
        assert_eq!(ev.status, SloComplianceStatus::Breaching);
        assert!(!ev.breaches.is_empty());
        assert_eq!(ev.suggestions[0].target, "replica");
    }

    #[test]
    fn cell_suggestions_are_capacity_only() {
        let ev = evaluate_slo(&slo(), Some(0.90), Some(100.0), Some(1000.0), Some(5.0), 1, true);
        assert_eq!(ev.suggestions[0].target, "cell");
        assert_eq!(ev.suggestions[0].kind, "capacity");
    }
}
