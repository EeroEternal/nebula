use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use nebula_meta::LeaderGate;

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

#[derive(Clone)]
pub struct AppState {
    pub metrics: Arc<SharedMetrics>,
    pub leader: LeaderGate,
}

/// GET /metrics — Prometheus text exposition format.
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    let metrics = &state.metrics;
    let (is_leader, epoch) = state.leader.snapshot();
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
         nebula_scheduler_xtrace_truncated_total {}\n\
         # HELP nebula_scheduler_is_leader 1 if this instance holds scheduler leadership.\n\
         # TYPE nebula_scheduler_is_leader gauge\n\
         nebula_scheduler_is_leader {}\n\
         # HELP nebula_scheduler_leader_epoch Current known leader fencing epoch.\n\
         # TYPE nebula_scheduler_leader_epoch gauge\n\
         nebula_scheduler_leader_epoch {}\n",
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
        if is_leader { 1 } else { 0 },
        epoch,
    );
    (axum::http::StatusCode::OK, body)
}

/// GET /healthz — readiness for LB: leader 200, follower 503.
pub async fn healthz_handler(State(state): State<AppState>) -> impl IntoResponse {
    if state.leader.is_leader() {
        (axum::http::StatusCode::OK, "ok")
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "secondary scheduler",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn healthz_leader_ok_follower_503() {
        let leader = LeaderGate::new();
        let state = AppState {
            metrics: Arc::new(SharedMetrics::default()),
            leader: leader.clone(),
        };
        let app = axum::Router::new()
            .route("/healthz", axum::routing::get(healthz_handler))
            .with_state(state.clone());

        let resp = app
            .clone()
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        leader.force_leader(3);
        let app = axum::Router::new()
            .route("/healthz", axum::routing::get(healthz_handler))
            .with_state(AppState {
                metrics: Arc::new(SharedMetrics::default()),
                leader,
            });
        let resp = app
            .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
