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

/// Fixed TPOT buckets (seconds per output token).///
/// Deliberately **not** [`HISTOGRAM_BUCKETS`]: the latency ladder jumps 0.01 -> 0.05,
/// which is exactly where a healthy TPOT lives (5-40 ms/token). Reusing it pinned
/// `nebula_route_tpot_seconds` p95 at 50 ms on real hardware (measured 11.35 ms) and
/// since `router_metrics` reports the bucket *boundary*, every `tpot_p95_ms` target in
/// (10, 50] ms became a permanent false `Breaching` + `scale_decode_pool`.
const TPOT_BUCKETS: &[f64] = &[
    0.002, 0.005, 0.01, 0.015, 0.02, 0.03, 0.04, 0.05, 0.075, 0.1, 0.2, 0.5, 1.0, 5.0,
];

/// Fixed TTFT buckets (seconds to first token).
///
/// `HISTOGRAM_BUCKETS` jumps 0.01 -> 0.05 -> 0.1 -> 0.5, so a 10 ms TTFT is reported
/// as 50 ms and a 60 ms one as 100 ms. Real TTFT targets are 100 ms - 10 s, which the
/// ladder resolves at 2.5x-5x steps; that is enough to turn a met target into a
/// breach. Finer below 1 s, 2x-3x above it.
const TTFT_BUCKETS: &[f64] = &[
    0.005, 0.01, 0.015, 0.02, 0.03, 0.05, 0.075, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.75, 1.0, 1.5,
    2.0, 3.0, 5.0, 10.0, 30.0, 60.0,
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
    /// Streams that ended with an upstream error before `[DONE]` (truncated 200).
    pub upstream_stream_error_total: AtomicU64,
    /// Requests that were retried on another replica because no first byte arrived
    /// within the budget (pre-first-byte commitment, issue #42).
    pub first_byte_retry_total: AtomicU64,

    /// Per-model E2E latency histogram (seconds).
    pub e2e_latency: DashMap<String, Histogram>,
    /// Per-model TTFT histogram (seconds) — only for SSE streaming responses.
    pub ttft: DashMap<String, Histogram>,
    /// Per-model TPOT (time per output token, seconds) histogram — SSE streaming responses.
    pub tpot: DashMap<String, Histogram>,
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
            .or_insert_with(|| Histogram::new(TTFT_BUCKETS))
            .observe(seconds);
    }

    pub fn observe_tpot(&self, model_uid: &str, seconds: f64) {
        self.tpot
            .entry(model_uid.to_string())
            .or_insert_with(|| Histogram::new(TPOT_BUCKETS))
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
        "# HELP nebula_router_requests_aborted_total Client disconnect/abort while proxying (excluded from 5xx).\n\
         # TYPE nebula_router_requests_aborted_total counter\n\
         nebula_router_requests_aborted_total {}\n",
        st.metrics.requests_aborted_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_first_byte_retry_total Requests retried on another replica because no first byte arrived in time.\n\
         # TYPE nebula_router_first_byte_retry_total counter\n\
         nebula_router_first_byte_retry_total {}\n",
        st.metrics.first_byte_retry_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_upstream_stream_error_total Streams ended by an upstream error before [DONE] (client sees a truncated 200).\n\
         # TYPE nebula_router_upstream_stream_error_total counter\n\
         nebula_router_upstream_stream_error_total {}\n",
        st.metrics.upstream_stream_error_total.load(Ordering::Relaxed),
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
    body.push_str(
        "# HELP nebula_router_hint_total Hint lifecycle counters by stage.\n\
         # TYPE nebula_router_hint_total counter\n",
    );
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"received\"}} {}\n",
        st.router.hint_received_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"adopted\"}} {}\n",
        st.router.hint_adopted_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"conflict_rejected\"}} {}\n",
        st.router.hint_conflict_rejected_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"expired\"}} {}\n",
        st.router.hint_expired_total(),
    ));
    body.push_str(&format!(
        "nebula_router_hint_total{{stage=\"stale_degraded\"}} {}\n",
        st.router.hint_stale_degraded_total(),
    ));
    body.push_str(
        "# HELP nebula_router_affinity_hit_total Affinity/hint hits by kind.\n\
         # TYPE nebula_router_affinity_hit_total counter\n",
    );
    body.push_str(&format!(
        "nebula_router_affinity_hit_total{{kind=\"session\"}} {}\n",
        st.router.session_affinity_hit_total(),
    ));
    body.push_str(&format!(
        "nebula_router_affinity_hit_total{{kind=\"prefix_hint\"}} {}\n",
        st.router.prefix_hint_hit_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_prefix_index_entries Live prefix-index entries (KV-event routing hints).\n\
         # TYPE nebula_router_prefix_index_entries gauge\n\
         nebula_router_prefix_index_entries {}\n\
         # HELP nebula_router_prefix_index_stale_dropped_total Stale snapshot or delta events dropped by watermark protection.\n\
         # TYPE nebula_router_prefix_index_stale_dropped_total counter\n\
         nebula_router_prefix_index_stale_dropped_total {}\n",
        st.router.prefix_index_entries(),
        st.router.prefix_index_stale_dropped_total(),
    ));
    body.push_str(
        "# HELP nebula_router_prefix_lookup_total Prefix-index lookups by model.\n\
         # TYPE nebula_router_prefix_lookup_total counter\n",
    );
    for (model, lookup, hit) in st.router.prefix_lookup_stats() {
        body.push_str(&format!(
            "nebula_router_prefix_lookup_total{{model_uid=\"{model}\"}} {lookup}\n"
        ));
        body.push_str(&format!(
            "nebula_router_prefix_hit_total{{model_uid=\"{model}\"}} {hit}\n"
        ));
    }
    body.push_str(
        "# HELP nebula_router_prefix_index_model_entries Prefix-index entries by model.\n\
         # TYPE nebula_router_prefix_index_model_entries gauge\n",
    );
    let mut by_model: Vec<_> = st
        .router
        .prefix_index_entries_by_model()
        .into_iter()
        .collect();
    by_model.sort();
    for (model, n) in by_model {
        body.push_str(&format!(
            "nebula_router_prefix_index_model_entries{{model_uid=\"{model}\"}} {n}\n"
        ));
    }
    body.push_str(
        "# HELP nebula_router_strategy_fallback_total Strategy selections that fell back to least_pending, by reason.\n\
         # TYPE nebula_router_strategy_fallback_total counter\n",
    );
    body.push_str(
        "# HELP nebula_router_stage_fallback_total pd_split stage-pool fallbacks (empty role pool -> all candidates).\n\
         # TYPE nebula_router_stage_fallback_total counter\n",
    );
    let (pd_lookup, pd_lookup_warm) = st.router.pd_lookup_stats();
    body.push_str(
        "# HELP nebula_router_pd_lookup_total pd_split LMCache controller warm lookups.\n\
         # TYPE nebula_router_pd_lookup_total counter\n",
    );
    body.push_str(&format!("nebula_router_pd_lookup_total {pd_lookup}\n"));
    body.push_str(&format!(
        "nebula_router_pd_lookup_warm_total {pd_lookup_warm}\n"
    ));
    for stage in ["prefill", "decode"] {
        body.push_str(&format!(
            "nebula_router_stage_fallback_total{{stage=\"{stage}\"}} {}\n",
            st.router.stage_fallback_total(stage),
        ));
    }
    body.push_str(&format!(
        "# HELP nebula_router_stage_prefill_spillover_total pd_split cold requests spilled over to idle Decode replicas when Prefill pool is saturated.\n\
         # TYPE nebula_router_stage_prefill_spillover_total counter\n\
         nebula_router_stage_prefill_spillover_total {}\n",
        st.router.stage_prefill_spillover_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_pd_decode_local_hit_total pd_split warm requests routed to a Decode replica holding the prefix in local GPU VRAM.\n\
         # TYPE nebula_router_pd_decode_local_hit_total counter\n\
         nebula_router_pd_decode_local_hit_total {}\n",
        st.router.pd_decode_local_hit_total(),
    ));
    body.push_str(
        "# HELP nebula_router_stage_stats_missing_total pd_split stage decisions skipped because the pool had no fresh /stats/ (needs the nebula-node/k8s-controller scrape).\n\
         # TYPE nebula_router_stage_stats_missing_total counter\n",
    );
    for decision in ["short_bypass", "prefill_spillover"] {
        body.push_str(&format!(
            "nebula_router_stage_stats_missing_total{{decision=\"{decision}\"}} {}\n",
            st.router.stage_stats_missing(decision),
        ));
    }
    body.push_str(&format!(
        "# HELP nebula_router_pd_short_bypass_total pd_split short cold requests bypassing controller lookup and prefill pool to unsaturated decode pool.\n\
         # TYPE nebula_router_pd_short_bypass_total counter\n\
         nebula_router_pd_short_bypass_total {}\n",
        st.router.pd_short_bypass_total(),
    ));
    body.push_str(&format!(
        "# HELP nebula_router_tool_pause_marked_total Coding agent sessions marked as ToolPaused on tool_calls finish_reason.\n\
         # TYPE nebula_router_tool_pause_marked_total counter\n\
         nebula_router_tool_pause_marked_total {}\n\
         # HELP nebula_router_tool_continuation_hit_total Coding agent continuation turns routed to anchored replica under active Tool-Pause lease.\n\
         # TYPE nebula_router_tool_continuation_hit_total counter\n\
         nebula_router_tool_continuation_hit_total {}\n",
        st.router.tool_pause_marked_total(),
        st.router.tool_continuation_hit_total(),
    ));
    let strategy = st.router.strategy_name();
    for reason in [
        "no_signal",
        "below_threshold",
        "owner_unavailable",
        "owner_congested",
    ] {
        body.push_str(&format!(
            "nebula_router_strategy_fallback_total{{strategy=\"{strategy}\",reason=\"{reason}\"}} {}\n",
            st.router.strategy_fallback_total(strategy, reason),
        ));
    }

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

    // TPOT histograms
    body.push_str("# HELP nebula_route_tpot_seconds Time per output token (streaming only).\n# TYPE nebula_route_tpot_seconds histogram\n");
    for entry in st.metrics.tpot.iter() {
        body.push_str(
            &entry
                .value()
                .format_prometheus("nebula_route_tpot_seconds", entry.key()),
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

#[cfg(test)]
mod route_histogram_bucket_tests {
    use super::*;

    /// Upper bound of the bucket a value is counted into — this is exactly what
    /// `nebula_control::parse_histogram_quantile_filtered` reports as the quantile.
    fn reported_upper_bound(buckets: &[f64], value: f64) -> f64 {
        buckets
            .iter()
            .copied()
            .find(|b| value <= *b)
            .unwrap_or(f64::INFINITY)
    }

    #[test]
    fn realistic_tpot_is_not_reported_as_50ms() {
        // Measured on 8x RTX 5090, Qwen2.5-14B TP=2, vLLM 0.23.0: 11.35 ms/output token
        // (sum=0.034046s over 3 requests x 63 decode tokens).
        let tpot = 0.011_35;
        assert!(
            reported_upper_bound(TPOT_BUCKETS, tpot) <= 0.015,
            "11.35ms TPOT must resolve below 20ms, got {}",
            reported_upper_bound(TPOT_BUCKETS, tpot)
        );
        // The latency ladder reported 50ms for the same sample, which made every
        // tpot_p95 target in (10, 50] ms a permanent false breach.
        assert_eq!(reported_upper_bound(HISTOGRAM_BUCKETS, tpot), 0.05);
    }

    #[test]
    fn tpot_buckets_resolve_the_5_to_50ms_band_within_2x() {
        let mut low = 0.005_f64;
        while low < 0.05 {
            let next = TPOT_BUCKETS
                .iter()
                .copied()
                .find(|b| *b > low)
                .expect("TPOT buckets must cover the 5-50ms band");
            assert!(
                next / low <= 2.0,
                "TPOT bucket gap {low} -> {next} is wider than 2x; a reported p95 could \
                 overshoot the real value (and an SLO target) by more than one step"
            );
            low = next;
        }
    }

    #[test]
    fn realistic_ttft_is_not_reported_as_50ms() {
        // Same box/run: warm TTFT 8-12 ms, one 44 s sample (torch.compile warmup).
        let ttft = 0.011;
        assert!(
            reported_upper_bound(TTFT_BUCKETS, ttft) <= 0.015,
            "11 ms TTFT must resolve below 20 ms, got {}",
            reported_upper_bound(TTFT_BUCKETS, ttft)
        );
        // The latency ladder reported 50 ms for the same sample.
        assert_eq!(reported_upper_bound(HISTOGRAM_BUCKETS, ttft), 0.05);
        // A 600 ms TTFT target must still be resolvable, not quantised to 1 s.
        assert!(reported_upper_bound(TTFT_BUCKETS, 0.62) <= 0.75);
    }

    #[test]
    fn ttft_buckets_resolve_10ms_to_10s_within_2x() {
        let mut low = 0.01_f64;
        while low < 10.0 {
            let next = TTFT_BUCKETS
                .iter()
                .copied()
                .find(|b| *b > low)
                .expect("TTFT buckets must cover 10ms-10s");
            assert!(
                next / low <= 2.0,
                "TTFT bucket gap {low} -> {next} is wider than 2x; a reported p95 could \
                 overshoot the real value (and an SLO target) by more than one step"
            );
            low = next;
        }
    }

    #[test]
    fn tpot_histogram_counts_into_the_tpot_ladder() {
        let h = Histogram::new(TPOT_BUCKETS);
        for _ in 0..9 {
            h.observe(0.011_35);
        }
        h.observe(0.045);
        let rendered = h.format_prometheus("nebula_route_tpot_seconds", "m1");
        assert!(rendered.contains("le=\"0.015\"} 9"), "{rendered}");
        assert!(rendered.contains("le=\"0.05\"} 10"), "{rendered}");
    }
}
