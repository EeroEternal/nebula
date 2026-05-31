use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;

use crate::state::AppState;

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

pub async fn metrics_handler(State(st): State<AppState>) -> impl IntoResponse {
    let mut body = String::new();

    // Global counters
    body.push_str(&format!(
        "# HELP nebula_router_requests_total Total requests handled by router.\n\
         # TYPE nebula_router_requests_total counter\n\
         nebula_router_requests_total {}\n",
        st.metrics.requests_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_requests_inflight Currently in-flight requests.\n\
         # TYPE nebula_router_requests_inflight gauge\n\
         nebula_router_requests_inflight {}\n",
        st.metrics.requests_inflight.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_responses_2xx {}\nnebula_router_responses_4xx {}\nnebula_router_responses_5xx {}\n",
        st.metrics.status_2xx.load(Ordering::Relaxed),
        st.metrics.status_4xx.load(Ordering::Relaxed),
        st.metrics.status_5xx.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_retry_total Total retry attempts to upstream.\n\
         # TYPE nebula_router_retry_total counter\n\
         nebula_router_retry_total {}\n",
        st.metrics.retry_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_retry_success_total Retry attempts that succeeded.\n\
         # TYPE nebula_router_retry_success_total counter\n\
         nebula_router_retry_success_total {}\n",
        st.metrics.retry_success_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_request_too_large_total Rejected requests due to max body size.\n\
         # TYPE nebula_router_request_too_large_total counter\n\
         nebula_router_request_too_large_total {}\n",
        st.metrics.request_too_large_total.load(Ordering::Relaxed),
    ));
    body.push_str("# HELP nebula_router_upstream_error_total Upstream errors by kind.\n# TYPE nebula_router_upstream_error_total counter\n");
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"connect\"}} {}\n",
        st.metrics
            .upstream_error_connect_total
            .load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"timeout\"}} {}\n",
        st.metrics
            .upstream_error_timeout_total
            .load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"upstream_5xx\"}} {}\n",
        st.metrics.upstream_error_5xx_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_router_upstream_error_total{{kind=\"other\"}} {}\n",
        st.metrics
            .upstream_error_other_total
            .load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_xtrace_query_errors_total xtrace query errors in stats sync loop.\n\
         # TYPE nebula_router_xtrace_query_errors_total counter\n\
         nebula_router_xtrace_query_errors_total {}\n",
        st.router.xtrace_query_errors_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_xtrace_rate_limited_total xtrace 429 responses observed in stats sync loop.\n\
         # TYPE nebula_router_xtrace_rate_limited_total counter\n\
         nebula_router_xtrace_rate_limited_total {}\n",
        st.router.xtrace_rate_limited_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_xtrace_stale_total stale xtrace metric responses skipped.\n\
         # TYPE nebula_router_xtrace_stale_total counter\n\
         nebula_router_xtrace_stale_total {}\n",
        st.router.xtrace_stale_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_xtrace_truncated_total truncated xtrace metric responses observed.\n\
         # TYPE nebula_router_xtrace_truncated_total counter\n\
         nebula_router_xtrace_truncated_total {}\n",
        st.router.xtrace_truncated_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_route_stale_stats_dropped_total stale routing stats dropped at route-time freshness gate.\n\
         # TYPE nebula_router_route_stale_stats_dropped_total counter\n\
         nebula_router_route_stale_stats_dropped_total {}\n",
        st.router.route_stale_stats_dropped_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_route_circuit_skipped_total candidates skipped due to open endpoint circuit breaker.\n\
         # TYPE nebula_router_route_circuit_skipped_total counter\n\
         nebula_router_route_circuit_skipped_total {}\n",
        st.router.route_circuit_skipped_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_circuit_open_total endpoint circuit breaker openings.\n\
         # TYPE nebula_router_circuit_open_total counter\n\
         nebula_router_circuit_open_total {}\n",
        st.router.circuit_open_total(),
    ));

    // Per-model counters
    body.push_str(
        "# HELP nebula_route_total Per-model request count.\n# TYPE nebula_route_total counter\n",
    );
    for entry in st.metrics.model_counters.iter() {
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

    // E2E latency histograms
    body.push_str("# HELP nebula_route_latency_seconds E2E request latency.\n# TYPE nebula_route_latency_seconds histogram\n");
    for entry in st.metrics.e2e_latency.iter() {
        body.push_str(
            &entry
                .value()
                .format_prometheus("nebula_route_latency_seconds", entry.key()),
        );
    }

    // TTFT histograms
    body.push_str("# HELP nebula_route_ttft_seconds Time to first token (streaming only).\n# TYPE nebula_route_ttft_seconds histogram\n");
    for entry in st.metrics.ttft.iter() {
        body.push_str(
            &entry
                .value()
                .format_prometheus("nebula_route_ttft_seconds", entry.key()),
        );
    }

    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

pub async fn track_requests(
    State(st): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, std::convert::Infallible> {
    st.metrics.requests_inflight.fetch_add(1, Ordering::Relaxed);
    let resp = next.run(req).await;
    st.metrics.requests_inflight.fetch_sub(1, Ordering::Relaxed);
    st.metrics.requests_total.fetch_add(1, Ordering::Relaxed);

    let status = resp.status().as_u16();
    if status >= 500 {
        st.metrics.status_5xx.fetch_add(1, Ordering::Relaxed);
    } else if status >= 400 {
        st.metrics.status_4xx.fetch_add(1, Ordering::Relaxed);
    } else if status >= 200 {
        st.metrics.status_2xx.fetch_add(1, Ordering::Relaxed);
    }

    Ok(resp)
}
