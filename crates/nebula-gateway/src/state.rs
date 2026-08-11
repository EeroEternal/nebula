use std::sync::Arc;

use nebula_common::admission::TenantAdmission;
use nebula_common::DualWriteEmitter;
use nebula_meta::EtcdMetaStore;

use crate::audit::AuditWriter;
use crate::platform_auth::GatewayAuth;
use crate::engine::EngineClient;
use crate::metrics::Metrics;

#[derive(Clone)]
pub struct AppState {
    pub _noop: Arc<()>,
    pub engine: Arc<dyn EngineClient>,
    pub router_base_url: String,
    pub http: reqwest::Client,
    pub store: Arc<EtcdMetaStore>,
    pub auth: GatewayAuth,
    pub metrics: Arc<Metrics>,
    pub dual_write: DualWriteEmitter,
    pub max_request_body_bytes: usize,
    pub log_path: String,
    pub audit: Option<Arc<AuditWriter>>,
    pub xtrace_url: Option<String>,
    pub xtrace_token: Option<String>,
    pub bff_url: String,
    pub tenant_admission: TenantAdmission,
}

impl AsRef<nebula_common::auth::AuthConfig> for AppState {
    fn as_ref(&self) -> &nebula_common::auth::AuthConfig {
        &self.auth.env
    }
}
