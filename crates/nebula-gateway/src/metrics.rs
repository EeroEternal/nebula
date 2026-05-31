use std::sync::atomic::{AtomicU64, Ordering};

use axum::{
    body::Body,
    extract::State,
    http::Request,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension,
};

use crate::auth::{require_role, AuthContext, Role};
use crate::state::AppState;

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
    pub request_too_large_total: AtomicU64,
    pub upstream_error_connect_total: AtomicU64,
    pub upstream_error_timeout_total: AtomicU64,
    pub upstream_error_other_total: AtomicU64,
}

impl Metrics {
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
        "nebula_gateway_upstream_error_total{{kind=\"other\"}} {}\n",
        metrics.upstream_error_other_total.load(Ordering::Relaxed),
    ));

    body
}

pub async fn metrics_handler(State(st): State<AppState>) -> impl IntoResponse {
    let body = render_metrics(&st.metrics);
    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
}

pub async fn admin_metrics(
    State(st): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> impl IntoResponse {
    if let Some(resp) = require_role(&st.metrics, &ctx, Role::Viewer) {
        return resp;
    }
    let body = render_metrics(&st.metrics);
    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
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
