use std::sync::atomic::{AtomicU64, Ordering};

use crate::state::AppState;
use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

#[derive(Debug, Default)]
pub struct Metrics {
    pub requests_total: AtomicU64,
    pub requests_inflight: AtomicU64,
    pub status_2xx: AtomicU64,
    pub status_4xx: AtomicU64,
    pub status_5xx: AtomicU64,
    pub auth_missing: AtomicU64,
    pub auth_invalid: AtomicU64,
    pub auth_forbidden: AtomicU64,
    pub auth_rate_limited: AtomicU64,
    /// Low-cardinality tenant denials by reason (never label by tenant_id).
    pub tenant_denied_rps: AtomicU64,
    pub tenant_denied_concurrency: AtomicU64,
    pub tenant_denied_model: AtomicU64,
    pub tenant_denied_token_budget: AtomicU64,
    pub tenant_denied_disabled: AtomicU64,
    pub tenant_denied_pin_admission: AtomicU64,
    pub request_too_large_total: AtomicU64,
    pub upstream_error_connect_total: AtomicU64,
    pub upstream_error_5xx_total: AtomicU64,
    pub upstream_error_timeout_total: AtomicU64,
    pub upstream_error_other_total: AtomicU64,
    /// Embedded-mode upstream retries (parity with nebula_router_retry_total).
    pub retry_total: AtomicU64,
    pub retry_success_total: AtomicU64,
    pub hint_received_total: AtomicU64,
    pub hint_forwarded_total: AtomicU64,
    pub hint_rejected_total: AtomicU64,
    pub hint_untrusted_total: AtomicU64,
    /// Client disconnect / explicit abort — not counted as 5xx error budget.
    pub requests_aborted_total: AtomicU64,
    /// Engine queue admission denials by low-cardinality reason.
    pub queue_denied_full_total: AtomicU64,
    pub queue_denied_tenant_total: AtomicU64,
    pub queue_denied_timeout_total: AtomicU64,
}

impl Metrics {
    pub fn record_tenant_deny(&self, code: &str) {
        match code {
            "tenant_rps_exceeded" => {
                self.tenant_denied_rps.fetch_add(1, Ordering::Relaxed);
                self.auth_rate_limited.fetch_add(1, Ordering::Relaxed);
            }
            "tenant_concurrency_exceeded" => {
                self.tenant_denied_concurrency
                    .fetch_add(1, Ordering::Relaxed);
            }
            "tenant_model_denied" => {
                self.tenant_denied_model.fetch_add(1, Ordering::Relaxed);
            }
            "tenant_token_budget_exceeded" => {
                self.tenant_denied_token_budget
                    .fetch_add(1, Ordering::Relaxed);
            }
            "tenant_disabled" => {
                self.tenant_denied_disabled.fetch_add(1, Ordering::Relaxed);
            }
            "tenant_pin_admission_exceeded" => {
                self.tenant_denied_pin_admission
                    .fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    pub fn record_queue_deny(&self, code: &str) {
        match code {
            "tenant_queue_full" => self
                .queue_denied_tenant_total
                .fetch_add(1, Ordering::Relaxed),
            "queue_timeout" => self
                .queue_denied_timeout_total
                .fetch_add(1, Ordering::Relaxed),
            _ => self.queue_denied_full_total.fetch_add(1, Ordering::Relaxed),
        };
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

pub fn render_metrics(metrics: &Metrics) -> String {
    let mut body = String::new();

    body.push_str(&format!(
        "# HELP nebula_gateway_requests_total Total requests handled by gateway.\n\
         # TYPE nebula_gateway_requests_total counter\n\
         nebula_gateway_requests_total {}\n",
        metrics.requests_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_requests_inflight Currently in-flight requests.\n\
         # TYPE nebula_gateway_requests_inflight gauge\n\
         nebula_gateway_requests_inflight {}\n",
        metrics.requests_inflight.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_responses_2xx Total 2xx responses.\n\
         # TYPE nebula_gateway_responses_2xx counter\n\
         nebula_gateway_responses_2xx {}\n",
        metrics.status_2xx.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_responses_4xx Total 4xx responses.\n\
         # TYPE nebula_gateway_responses_4xx counter\n\
         nebula_gateway_responses_4xx {}\n",
        metrics.status_4xx.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_responses_5xx Total 5xx responses.\n\
         # TYPE nebula_gateway_responses_5xx counter\n\
         nebula_gateway_responses_5xx {}\n",
        metrics.status_5xx.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_auth_missing Requests with missing auth credentials.\n\
         # TYPE nebula_gateway_auth_missing counter\n\
         nebula_gateway_auth_missing {}\n",
        metrics.auth_missing.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_auth_invalid Requests with invalid auth credentials.\n\
         # TYPE nebula_gateway_auth_invalid counter\n\
         nebula_gateway_auth_invalid {}\n",
        metrics.auth_invalid.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_auth_forbidden Requests denied due to insufficient permissions.\n\
         # TYPE nebula_gateway_auth_forbidden counter\n\
         nebula_gateway_auth_forbidden {}\n",
        metrics.auth_forbidden.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_auth_rate_limited Requests rejected due to rate limiting.\n\
         # TYPE nebula_gateway_auth_rate_limited counter\n\
         nebula_gateway_auth_rate_limited {}\n",
        metrics.auth_rate_limited.load(Ordering::Relaxed),
    ));
    body.push_str(
        "# HELP nebula_gateway_tenant_denied_total Tenant quota denials by reason (no tenant_id label).\n\
         # TYPE nebula_gateway_tenant_denied_total counter\n",
    );
    body.push_str(&format!(
        "nebula_gateway_tenant_denied_total{{reason=\"rps\"}} {}\n",
        metrics.tenant_denied_rps.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_tenant_denied_total{{reason=\"concurrency\"}} {}\n",
        metrics.tenant_denied_concurrency.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_tenant_denied_total{{reason=\"model\"}} {}\n",
        metrics.tenant_denied_model.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_tenant_denied_total{{reason=\"token_budget\"}} {}\n",
        metrics.tenant_denied_token_budget.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_tenant_denied_total{{reason=\"disabled\"}} {}\n",
        metrics.tenant_denied_disabled.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_tenant_denied_total{{reason=\"pin_admission\"}} {}\n",
        metrics.tenant_denied_pin_admission.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_request_too_large_total Requests rejected due to max body size.\n\
         # TYPE nebula_gateway_request_too_large_total counter\n\
         nebula_gateway_request_too_large_total {}\n",
        metrics.request_too_large_total.load(Ordering::Relaxed),
    ));
    body.push_str("# HELP nebula_gateway_upstream_error_total Upstream proxy errors by kind.\n# TYPE nebula_gateway_upstream_error_total counter\n");
    body.push_str(&format!(
        "nebula_gateway_upstream_error_total{{kind=\"connect\"}} {}\n",
        metrics.upstream_error_connect_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_upstream_error_total{{kind=\"timeout\"}} {}\n",
        metrics.upstream_error_timeout_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_upstream_error_total{{kind=\"upstream_5xx\"}} {}\n",
        metrics.upstream_error_5xx_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_upstream_error_total{{kind=\"other\"}} {}\n",
        metrics.upstream_error_other_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_retry_total Upstream retries after 5xx/transport error (embedded router).\n\
         # TYPE nebula_gateway_retry_total counter\n\
         nebula_gateway_retry_total {}\n",
        metrics.retry_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_retry_success_total Retried requests that eventually succeeded.\n\
         # TYPE nebula_gateway_retry_success_total counter\n\
         nebula_gateway_retry_success_total {}\n",
        metrics.retry_success_total.load(Ordering::Relaxed),
    ));
    body.push_str(
        "# HELP nebula_gateway_hint_total Hint processing counters by stage.\n\
         # TYPE nebula_gateway_hint_total counter\n",
    );
    body.push_str(&format!(
        "nebula_gateway_hint_total{{stage=\"received\"}} {}\n",
        metrics.hint_received_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_hint_total{{stage=\"forwarded\"}} {}\n",
        metrics.hint_forwarded_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_hint_total{{stage=\"rejected\"}} {}\n",
        metrics.hint_rejected_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_hint_total{{stage=\"untrusted\"}} {}\n",
        metrics.hint_untrusted_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "# HELP nebula_gateway_requests_aborted_total Client disconnect/abort (excluded from 5xx error budget).\n\
         # TYPE nebula_gateway_requests_aborted_total counter\n\
         nebula_gateway_requests_aborted_total {}\n",
        metrics.requests_aborted_total.load(Ordering::Relaxed),
    ));
    body.push_str(
        "# HELP nebula_gateway_queue_denied_total Engine queue admission denials by reason.\n\
         # TYPE nebula_gateway_queue_denied_total counter\n",
    );
    body.push_str(&format!(
        "nebula_gateway_queue_denied_total{{reason=\"queue_full\"}} {}\n",
        metrics.queue_denied_full_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_queue_denied_total{{reason=\"tenant_queue_full\"}} {}\n",
        metrics.queue_denied_tenant_total.load(Ordering::Relaxed),
    ));
    body.push_str(&format!(
        "nebula_gateway_queue_denied_total{{reason=\"queue_timeout\"}} {}\n",
        metrics.queue_denied_timeout_total.load(Ordering::Relaxed),
    ));

    body
}

/// Embedded mode: expose the in-process Router's counters using the same metric
/// names as a standalone `nebula-router`, so existing dashboards keep working.
/// Embedded mode: expose the in-process Router's counters using the same metric
/// names as a standalone `nebula-router`, so existing dashboards keep working.
pub fn render_router_metrics(router: &nebula_router::Router) -> String {
    use std::fmt::Write as _;
    let mut body = String::new();
    let _ = writeln!(
        body,
        "# HELP nebula_router_route_stale_stats_dropped_total stale routing stats dropped at route-time freshness gate.\n\
         # TYPE nebula_router_route_stale_stats_dropped_total counter\n\
         nebula_router_route_stale_stats_dropped_total {}",
        router.route_stale_stats_dropped_total()
    );
    let _ = writeln!(
        body,
        "# HELP nebula_router_route_circuit_skipped_total candidates skipped due to open endpoint circuit breaker.\n\
         # TYPE nebula_router_route_circuit_skipped_total counter\n\
         nebula_router_route_circuit_skipped_total {}",
        router.route_circuit_skipped_total()
    );
    let _ = writeln!(
        body,
        "# HELP nebula_router_circuit_open_total endpoint circuit breaker openings.\n\
         # TYPE nebula_router_circuit_open_total counter\n\
         nebula_router_circuit_open_total {}",
        router.circuit_open_total()
    );
    let _ = writeln!(
        body,
        "# HELP nebula_router_affinity_hit_total Affinity/hint hits by kind.\n\
         # TYPE nebula_router_affinity_hit_total counter\n\
         nebula_router_affinity_hit_total{{kind=\"session\"}} {}\n\
         nebula_router_affinity_hit_total{{kind=\"prefix_hint\"}} {}",
        router.session_affinity_hit_total(),
        router.prefix_hint_hit_total()
    );
    let _ = writeln!(
        body,
        "# HELP nebula_router_hint_total Hint lifecycle counters by stage.\n\
         # TYPE nebula_router_hint_total counter\n\
         nebula_router_hint_total{{stage=\"received\"}} {}\n\
         nebula_router_hint_total{{stage=\"adopted\"}} {}\n\
         nebula_router_hint_total{{stage=\"conflict_rejected\"}} {}\n\
         nebula_router_hint_total{{stage=\"expired\"}} {}\n\
         nebula_router_hint_total{{stage=\"stale_degraded\"}} {}",
        router.hint_received_total(),
        router.hint_adopted_total(),
        router.hint_conflict_rejected_total(),
        router.hint_expired_total(),
        router.hint_stale_degraded_total()
    );
    body
}
/// Per-model engine-queue gauges when queue admission is enabled.
fn render_queue_metrics(queues: &nebula_common::queue::EngineQueues) -> String {
    use std::fmt::Write as _;
    let mut body = String::new();
    let _ = writeln!(
        body,
        "# HELP nebula_gateway_queue_inflight In-flight requests per model (engine queue).\n\
         # TYPE nebula_gateway_queue_inflight gauge"
    );
    let _ = writeln!(
        body,
        "# HELP nebula_gateway_queue_depth Waiting requests per model (engine queue).\n\
         # TYPE nebula_gateway_queue_depth gauge"
    );
    for (model, q) in queues.snapshot() {
        let _ = writeln!(
            body,
            "nebula_gateway_queue_inflight{{model=\"{model}\"}} {}",
            q.inflight()
        );
        let _ = writeln!(
            body,
            "nebula_gateway_queue_depth{{model=\"{model}\"}} {}",
            q.depth()
        );
    }
    body
}

pub async fn metrics_handler(State(st): State<AppState>) -> impl IntoResponse {
    let mut body = render_metrics(&st.metrics);
    if let Some(router) = st.router.as_ref() {
        body.push_str(&render_router_metrics(router));
    }
    if let Some(queues) = st.queues.as_ref() {
        body.push_str(&render_queue_metrics(queues));
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
    let start = std::time::Instant::now();
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

    // Dual-write to xtrace (Prometheus already updated above). Abort counting is
    // separate in handlers; here we mirror aggregate gateway traffic.
    st.dual_write.emit_request_outcome(
        "nebula_gateway",
        None,
        status,
        Some(start.elapsed().as_secs_f64()),
        false,
    );

    Ok(resp)
}
