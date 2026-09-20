// Re-export shared auth types from nebula-common.
pub use nebula_common::auth::{AuthContext, Role};

use axum::response::Response;

use crate::metrics::Metrics;

/// Gateway-specific `require_role` that increments the `auth_forbidden` metric
/// counter before delegating to the shared implementation.
pub fn require_role(metrics: &Metrics, ctx: &AuthContext, required: Role) -> Option<Response> {
    let resp = nebula_common::auth::require_role(ctx, required);
    if resp.is_some() {
        metrics
            .auth_forbidden
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    resp
}
