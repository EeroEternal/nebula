use std::sync::Arc;

use nebula_common::auth::AuthConfig;
use nebula_common::{DualWriteEmitter, UpstreamTransport};

use crate::metrics::Metrics;

#[derive(Clone)]
pub struct AppState {
    pub model_uid: String,
    pub router: Arc<nebula_router::Router>,
    /// Engine data-plane transport (`NEBULA_UPSTREAM_TRANSPORT`); one attempt per call.
    pub upstream: Arc<dyn UpstreamTransport>,
    pub metrics: Arc<Metrics>,
    pub dual_write: DualWriteEmitter,
    pub max_request_body_bytes: usize,
    pub retry_max: u32,
    pub retry_backoff_ms: u64,
    pub auth: AuthConfig,
}

impl AsRef<AuthConfig> for AppState {
    fn as_ref(&self) -> &AuthConfig {
        &self.auth
    }
}
