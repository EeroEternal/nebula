use std::sync::Arc;

use nebula_common::admission::TenantAdmission;
use nebula_common::DualWriteEmitter;
use nebula_meta::EtcdMetaStore;

use crate::audit::AuditWriter;
use crate::metrics::Metrics;
use crate::platform_auth::GatewayAuth;

#[derive(Clone)]
pub struct AppState {
    pub _noop: Arc<()>,
    pub router_base_url: String,
    /// Present when `NEBULA_ROUTER_MODE=embedded`: route in-process instead of
    /// forwarding to a standalone `nebula-router` over HTTP (saves one hop).
    pub router: Option<Arc<nebula_router::Router>>,
    /// Embedded-mode upstream retry policy (parity with `nebula-router`).
    pub retry_max: u32,
    pub retry_backoff_ms: u64,
    pub http: reqwest::Client,
    pub store: Arc<EtcdMetaStore>,
    pub auth: GatewayAuth,
    pub metrics: Arc<Metrics>,
    pub dual_write: DualWriteEmitter,
    pub max_request_body_bytes: usize,
    #[allow(dead_code)] // surfaced in startup logging / future audit wiring
    pub log_path: String,
    pub audit: Option<Arc<AuditWriter>>,
    pub xtrace_url: Option<String>,
    pub xtrace_token: Option<String>,
    #[allow(dead_code)] // retained for BFF reverse-linking from platform routes
    pub bff_url: String,
    pub tenant_admission: TenantAdmission,
}

impl AsRef<nebula_common::auth::AuthConfig> for AppState {
    fn as_ref(&self) -> &nebula_common::auth::AuthConfig {
        &self.auth.env
    }
}
