use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;

/// Shared metrics for the scheduler, safe for concurrent access.
#[derive(Debug, Default)]
pub struct SharedMetrics {
    /// Total number of reconcile loop iterations.
    pub reconcile_total: AtomicU64,
    /// Number of reconcile errors.
    pub reconcile_errors: AtomicU64,
    /// Current placement count (gauge).
    pub placements_total: AtomicU64,
    /// Detected unhealthy / stale endpoints.
    pub unhealthy_endpoints_total: AtomicU64,
    /// Scale-up decisions made.
    pub scale_up_total: AtomicU64,
    /// Scale-down decisions made.
    pub scale_down_total: AtomicU64,
    /// xtrace metric query errors.
    pub xtrace_query_errors_total: AtomicU64,
    /// xtrace rate-limited responses (429).
    pub xtrace_rate_limited_total: AtomicU64,
    /// xtrace stale metric responses skipped.
    pub xtrace_stale_total: AtomicU64,
    /// xtrace truncated metric responses observed.
    pub xtrace_truncated_total: AtomicU64,
}

/// GET /metrics — Prometheus text exposition format.
pub async fn metrics_handler(State(metrics): State<Arc<SharedMetrics>>) -> impl IntoResponse {
    let body = format!(
        "# HELP nebula_scheduler_reconcile_total Total reconcile loop iterations.\n\
         # TYPE nebula_scheduler_reconcile_total counter\n\
         nebula_scheduler_reconcile_total {}\n\
         # HELP nebula_scheduler_reconcile_errors Total reconcile errors.\n\
         # TYPE nebula_scheduler_reconcile_errors counter\n\
         nebula_scheduler_reconcile_errors {}\n\
         # HELP nebula_scheduler_placements_total Current placement count.\n\
         # TYPE nebula_scheduler_placements_total gauge\n\
         nebula_scheduler_placements_total {}\n\
         # HELP nebula_scheduler_unhealthy_endpoints_total Detected unhealthy endpoints.\n\
         # TYPE nebula_scheduler_unhealthy_endpoints_total counter\n\
         nebula_scheduler_unhealthy_endpoints_total {}\n\
         # HELP nebula_scheduler_scale_up_total Scale-up decisions.\n\
         # TYPE nebula_scheduler_scale_up_total counter\n\
         nebula_scheduler_scale_up_total {}\n\
         # HELP nebula_scheduler_scale_down_total Scale-down decisions.\n\
         # TYPE nebula_scheduler_scale_down_total counter\n\
         nebula_scheduler_scale_down_total {}\n\
         # HELP nebula_scheduler_xtrace_query_errors_total xtrace query errors while fetching autoscaling signals.\n\
         # TYPE nebula_scheduler_xtrace_query_errors_total counter\n\
         nebula_scheduler_xtrace_query_errors_total {}\n\
         # HELP nebula_scheduler_xtrace_rate_limited_total xtrace 429 responses while fetching autoscaling signals.\n\
         # TYPE nebula_scheduler_xtrace_rate_limited_total counter\n\
         nebula_scheduler_xtrace_rate_limited_total {}\n\
         # HELP nebula_scheduler_xtrace_stale_total stale xtrace responses skipped for autoscaling.\n\
         # TYPE nebula_scheduler_xtrace_stale_total counter\n\
         nebula_scheduler_xtrace_stale_total {}\n\
         # HELP nebula_scheduler_xtrace_truncated_total truncated xtrace responses observed for autoscaling.\n\
         # TYPE nebula_scheduler_xtrace_truncated_total counter\n\
         nebula_scheduler_xtrace_truncated_total {}\n",
        metrics.reconcile_total.load(Ordering::Relaxed),
        metrics.reconcile_errors.load(Ordering::Relaxed),
        metrics.placements_total.load(Ordering::Relaxed),
        metrics.unhealthy_endpoints_total.load(Ordering::Relaxed),
        metrics.scale_up_total.load(Ordering::Relaxed),
        metrics.scale_down_total.load(Ordering::Relaxed),
        metrics.xtrace_query_errors_total.load(Ordering::Relaxed),
        metrics.xtrace_rate_limited_total.load(Ordering::Relaxed),
        metrics.xtrace_stale_total.load(Ordering::Relaxed),
        metrics.xtrace_truncated_total.load(Ordering::Relaxed),
    );
    (axum::http::StatusCode::OK, body)
}

/// GET /healthz — simple liveness probe.
pub async fn healthz_handler() -> impl IntoResponse {
    (axum::http::StatusCode::OK, "ok")
}
