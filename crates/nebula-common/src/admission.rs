//! Live tenant admission counters (Gateway / Router boundary).
//!
//! Quotas are tenant-scoped so one tenant exhausting RPS cannot block another.
//! Prometheus metrics must use low-cardinality `reason` labels only — never tenant_id.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::tenant::{admit_static, AdmitDecision, Tenant, TenantDenyCode, TenantQuota};

#[derive(Debug, Default)]
struct TenantLive {
    rps_window_start: Option<Instant>,
    rps_count: u64,
    concurrency: u64,
    token_window_start: Option<Instant>,
    token_count: u64,
}

/// In-process admission state keyed by tenant_id.
#[derive(Debug, Clone, Default)]
pub struct TenantAdmission {
    inner: Arc<Mutex<HashMap<String, TenantLive>>>,
}

pub struct ConcurrencyGuard {
    admission: TenantAdmission,
    tenant_id: String,
}

impl Drop for ConcurrencyGuard {
    fn drop(&mut self) {
        let admission = self.admission.clone();
        let tenant_id = self.tenant_id.clone();
        tokio::spawn(async move {
            let mut guard = admission.inner.lock().await;
            if let Some(live) = guard.get_mut(&tenant_id) {
                live.concurrency = live.concurrency.saturating_sub(1);
            }
        });
    }
}

impl TenantAdmission {
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit a request under `tenant` quotas. Returns a concurrency guard on success.
    pub async fn try_admit(
        &self,
        tenant: &Tenant,
        model: Option<&str>,
        estimated_tokens: u32,
    ) -> Result<ConcurrencyGuard, TenantDenyCode> {
        match admit_static(tenant, model) {
            AdmitDecision::Allow => {}
            AdmitDecision::Deny(code) => return Err(code),
        }

        let quotas = &tenant.quotas;
        let mut map = self.inner.lock().await;
        let live = map.entry(tenant.tenant_id.clone()).or_default();
        let now = Instant::now();

        if let Some(limit) = quotas.rps_per_minute {
            reset_minute_window(&mut live.rps_window_start, &mut live.rps_count, now);
            if live.rps_count >= limit {
                return Err(TenantDenyCode::RpsExceeded);
            }
            live.rps_count += 1;
        }

        if let Some(limit) = quotas.max_concurrency {
            if live.concurrency >= limit {
                return Err(TenantDenyCode::ConcurrencyExceeded);
            }
        }

        if let Some(limit) = quotas.max_tokens_per_minute {
            reset_minute_window(&mut live.token_window_start, &mut live.token_count, now);
            let add = estimated_tokens as u64;
            if live.token_count.saturating_add(add) > limit {
                return Err(TenantDenyCode::TokenBudgetExceeded);
            }
            live.token_count = live.token_count.saturating_add(add);
        }

        live.concurrency += 1;
        Ok(ConcurrencyGuard {
            admission: self.clone(),
            tenant_id: tenant.tenant_id.clone(),
        })
    }
}

fn reset_minute_window(start: &mut Option<Instant>, count: &mut u64, now: Instant) {
    match *start {
        Some(s) if now.duration_since(s) < std::time::Duration::from_secs(60) => {}
        _ => {
            *start = Some(now);
            *count = 0;
        }
    }
}

/// Effective RPS for rate-limit key selection when multi-tenant is off.
pub fn rate_limit_key(tenant_id: Option<&str>, principal: &str) -> String {
    match tenant_id {
        Some(t) if !t.is_empty() => format!("tenant:{t}"),
        _ => format!("token:{principal}"),
    }
}

/// Merge optional override onto defaults (used when tenant missing from etcd).
pub fn effective_quota(tenant: Option<&Tenant>, fallback: &TenantQuota) -> TenantQuota {
    tenant.map(|t| t.quotas.clone()).unwrap_or_else(|| fallback.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tenant(rps: u64, conc: u64) -> Tenant {
        Tenant {
            tenant_id: "a".into(),
            display_name: "A".into(),
            enabled: true,
            quotas: TenantQuota {
                rps_per_minute: Some(rps),
                max_concurrency: Some(conc),
                max_tokens_per_minute: Some(1000),
                allowed_models: None,
            },
            api_token_principals: vec![],
            priority_default: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[tokio::test]
    async fn tenants_isolated_on_rps() {
        let adm = TenantAdmission::new();
        let a = tenant(1, 10);
        let mut b = tenant(10, 10);
        b.tenant_id = "b".into();

        let _g = adm.try_admit(&a, Some("m"), 1).await.expect("a1");
        assert_eq!(
            adm.try_admit(&a, Some("m"), 1).await.err(),
            Some(TenantDenyCode::RpsExceeded)
        );
        // Tenant B still allowed.
        adm.try_admit(&b, Some("m"), 1).await.expect("b ok");
    }

    #[tokio::test]
    async fn concurrency_releases_on_drop() {
        let adm = TenantAdmission::new();
        let t = tenant(100, 1);
        {
            let _g = adm.try_admit(&t, None, 0).await.unwrap();
            assert_eq!(
                adm.try_admit(&t, None, 0).await.err(),
                Some(TenantDenyCode::ConcurrencyExceeded)
            );
        }
        // Allow Drop's spawned task to run.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        adm.try_admit(&t, None, 0).await.expect("after drop");
    }

    #[test]
    fn rate_limit_key_prefers_tenant() {
        assert_eq!(rate_limit_key(Some("t1"), "tok"), "tenant:t1");
        assert_eq!(rate_limit_key(None, "tok"), "token:tok");
    }
}
