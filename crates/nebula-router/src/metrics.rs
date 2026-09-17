//! Router observability primitives shared by the standalone `nebula-router` binary and the
//! embedded in-process router inside `nebula-gateway`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use dashmap::DashMap;

use crate::Router;

/// Fixed histogram buckets (seconds), Prometheus standard for latency.
const HISTOGRAM_BUCKETS: &[f64] = &[
    0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0,
];

/// A simple histogram with fixed buckets, safe for concurrent use.
#[derive(Debug)]
pub struct Histogram {
    buckets: Vec<(f64, AtomicU64)>,
    count: AtomicU64,
    sum: Mutex<f64>,
}

impl Histogram {
    pub fn new(buckets: &[f64]) -> Self {
        Self {
            buckets: buckets.iter().map(|&b| (b, AtomicU64::new(0))).collect(),
            count: AtomicU64::new(0),
            sum: Mutex::new(0.0),
        }
    }

    pub fn observe(&self, value: f64) {
        for (le, count) in &self.buckets {
            if value <= *le {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut s) = self.sum.lock() {
            *s += value;
        }
    }

    /// Format as Prometheus histogram lines for a given metric name and model label.
    pub fn format_prometheus(&self, name: &str, model_uid: &str) -> String {
        let mut out = String::new();
        for (le, count) in &self.buckets {
            out.push_str(&format!(
                "{name}_bucket{{model_uid=\"{model_uid}\",le=\"{le}\"}} {}\n",
                count.load(Ordering::Relaxed)
            ));
        }
        out.push_str(&format!(
            "{name}_bucket{{model_uid=\"{model_uid}\",le=\"+Inf\"}} {}\n",
            self.count.load(Ordering::Relaxed)
        ));
        let sum = self.sum.lock().map(|s| *s).unwrap_or(0.0);
        out.push_str(&format!("{name}_sum{{model_uid=\"{model_uid}\"}} {sum}\n"));
        out.push_str(&format!(
            "{name}_count{{model_uid=\"{model_uid}\"}} {}\n",
            self.count.load(Ordering::Relaxed)
        ));
        out
    }
}

/// Per-model request counters.
#[derive(Debug, Default)]
pub struct ModelCounter {
    pub total: AtomicU64,
    pub status_2xx: AtomicU64,
    pub status_4xx: AtomicU64,
    pub status_5xx: AtomicU64,
}

#[derive(Debug, Default)]
pub struct Metrics {
    pub requests_total: AtomicU64,
    pub requests_inflight: AtomicU64,
    pub status_2xx: AtomicU64,
    pub status_4xx: AtomicU64,
    pub status_5xx: AtomicU64,
    pub retry_total: AtomicU64,
    pub retry_success_total: AtomicU64,
    pub request_too_large_total: AtomicU64,
    pub upstream_error_connect_total: AtomicU64,
    pub upstream_error_timeout_total: AtomicU64,
    pub upstream_error_5xx_total: AtomicU64,
    pub upstream_error_other_total: AtomicU64,
    /// Client disconnect / abort while proxying — not counted as 5xx.
    pub requests_aborted_total: AtomicU64,

    /// Per-model E2E latency histogram (seconds).
    pub e2e_latency: DashMap<String, Histogram>,
    /// Per-model TTFT histogram (seconds) — only for SSE streaming responses.
    pub ttft: DashMap<String, Histogram>,
    /// Per-model request counters.
    pub model_counters: DashMap<String, ModelCounter>,
}

impl Metrics {
    pub fn observe_e2e_latency(&self, model_uid: &str, seconds: f64) {
        self.e2e_latency
            .entry(model_uid.to_string())
            .or_insert_with(|| Histogram::new(HISTOGRAM_BUCKETS))
            .observe(seconds);
    }

    pub fn observe_ttft(&self, model_uid: &str, seconds: f64) {
        self.ttft
            .entry(model_uid.to_string())
            .or_insert_with(|| Histogram::new(HISTOGRAM_BUCKETS))
            .observe(seconds);
    }

    pub fn record_model_status(&self, model_uid: &str, status: u16) {
        let counter = self
            .model_counters
            .entry(model_uid.to_string())
            .or_default();
        counter.total.fetch_add(1, Ordering::Relaxed);
        if status >= 500 {
            counter.status_5xx.fetch_add(1, Ordering::Relaxed);
        } else if status >= 400 {
            counter.status_4xx.fetch_add(1, Ordering::Relaxed);
        } else if status >= 200 {
            counter.status_2xx.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_upstream_error(&self, kind: &str) {
        match kind {
            "connect" => {
                self.upstream_error_connect_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            "timeout" => {
                self.upstream_error_timeout_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            "upstream_5xx" => {
                self.upstream_error_5xx_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            _ => {
                self.upstream_error_other_total
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Render the full Prometheus text body for router-level metrics.
///
/// Shared by the standalone router `/metrics` endpoint and the embedded-router path inside the
/// gateway (which also exposes its own `nebula_gateway_*` surface).
pub fn render(metrics: &Metrics, router: &Router) -> String {
    let mut body = String::new();

    body.push_str(&format!(
        "# HELP nebula_router_requests_total Total requests handled by router.\n\
         # TYPE nebula_router_requests_total counter\n\
         nebula_router_requests_total {}\n",
        metrics.requests_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_requests_inflight Currently in-flight requests.\n\
         # TYPE nebula_router_requests_inflight gauge\n\
         nebula_router_requests_inflight {}\n",
        metrics.requests_inflight.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_responses_2xx {}\nnebula_router_responses_4xx {}\nnebula_router_responses_5xx {}\n",
        metrics.status_2xx.load(Ordering::Relaxed),
        metrics.status_4xx.load(Ordering::Relaxed),
        metrics.status_5xx.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_retry_total Total retry attempts to upstream.\n\
         # TYPE nebula_router_retry_total counter\n\
         nebula_router_retry_total {}\n",
        metrics.retry_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_retry_success_total Retry attempts that succeeded.\n\
         # TYPE nebula_router_retry_success_total counter\n\
         nebula_router_retry_success_total {}\n",
        metrics.retry_success_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_request_too_large_total Rejected requests due to max body size.\n\
         # TYPE nebula_router_request_too_large_total counter\n\
         nebula_router_request_too_large_total {}\n",
        metrics.request_too_large_total.load(Ordering::Relaxed),
    ));
    body.push_str("# HELP nebula_router_upstream_error_total Upstream errors by kind.\n# TYPE nebula_router_upstream_error_total counter\n");
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"connect\"}} {}\n",
        metrics.upstream_error_connect_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"timeout\"}} {}\n",
        metrics.upstream_error_timeout_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"upstream_5xx\"}} {}\n",
        metrics.upstream_error_5xx_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"other\"}} {}\n",
        metrics.upstream_error_other_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_requests_aborted_total Client disconnect/abort while proxying (excluded from 5xx).\n\
         # TYPE nebula_router_requests_aborted_total counter\n\
         nebula_router_requests_aborted_total {}\n",
        metrics.requests_aborted_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_route_stale_stats_dropped_total stale routing stats dropped at route-time freshness gate.\n\
         # TYPE nebula_router_route_stale_stats_dropped_total counter\n\
         nebula_router_route_stale_stats_dropped_total {}\n",
        router.route_stale_stats_dropped_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_route_circuit_skipped_total candidates skipped due to open endpoint circuit breaker.\n\
         # TYPE nebula_router_route_circuit_skipped_total counter\n\
         nebula_router_route_circuit_skipped_total {}\n",
        router.route_circuit_skipped_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_circuit_open_total endpoint circuit breaker openings.\n\
         # TYPE nebula_router_circuit_open_total counter\n\
         nebula_router_circuit_open_total {}\n",
        router.circuit_open_total(),
    ));
    body.push_str(
        "# HELP nebula_router_hint_total Hint lifecycle counters by stage.\n\
         # TYPE nebula_router_hint_total counter\n",
    );
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"received\"}} {}\n",
        router.hint_received_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"adopted\"}} {}\n",
        router.hint_adopted_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"conflict_rejected\"}} {}\n",
        router.hint_conflict_rejected_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"expired\"}} {}\n",
        router.hint_expired_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"stale_degraded\"}} {}\n",
        router.hint_stale_degraded_total(),
    ));
    body.push_str(
        "# HELP nebula_router_affinity_hit_total Affinity/hint hits by kind.\n\
         # TYPE nebula_router_affinity_hit_total counter\n",
    );
    body.push_str(&format!(
        "nebula_router_affinity_hit_total{{kind=\"session\"}} {}\n",
        router.session_affinity_hit_total(),
    ));
    body.push_str(&format!(
        "nebula_router_affinity_hit_total{{kind=\"prefix_hint\"}} {}\n",
        router.prefix_hint_hit_total(),
    ));

    body.push_str(
        "# HELP nebula_route_total Per-model request count.\n# TYPE nebula_route_total counter\n",
    );
    for entry in metrics.model_counters.iter() {
        let model = entry.key();
        let c = entry.value();
        body.push_str(&format!(
            "nebula_route_total{{model_uid=\"{model}\",status=\"2xx\"}} {}\n",
            c.status_2xx.load(Ordering::Relaxed)
        ));
        body.push_str(&format!(
            "nebula_route_total{{model_uid=\"{model}\",status=\"4xx\"}} {}\n",
            c.status_4xx.load(Ordering::Relaxed)
        ));
        body.push_str(&format!(
            "nebula_route_total{{model_uid=\"{model}\",status=\"5xx\"}} {}\n",
            c.status_5xx.load(Ordering::Relaxed)
        ));
    }

    body.push_str("# HELP nebula_route_latency_seconds E2E request latency.\n# TYPE nebula_route_latency_seconds histogram\n");
    for entry in metrics.e2e_latency.iter() {
        body.push_str(
            &entry
                .value()
                .format_prometheus("nebula_route_latency_seconds", entry.key()),
        );
    }

    body.push_str("# HELP nebula_route_ttft_seconds Time to first token (streaming only).\n# TYPE nebula_route_ttft_seconds histogram\n");
    for entry in metrics.ttft.iter() {
        body.push_str(
            &entry
                .value()
                .format_prometheus("nebula_route_ttft_seconds", entry.key()),
        );
    }

    body
}
