//! Live tenant admission counters (Gateway / Router boundary).
//!
//! Quotas are tenant-scoped so one tenant exhausting RPS cannot block another.
//! Prometheus metrics must use low-cardinality `reason` labels only — never tenant_id.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::quota_shard::{shard_quota, shard_quota_ranked, MemberView};
use crate::tenant::{admit_static, AdmitDecision, Tenant, TenantDenyCode, TenantQuota};

#[derive(Debug, Default)]
struct TenantLive {
    rps_window_start: Option<Instant>,
    rps_count: u64,
    concurrency: u64,
    token_window_start: Option<Instant>,
    token_count: u64,
    pin_window_start: Option<Instant>,
    pin_count: u64,
}

/// Where the cluster-quota divisor comes from (issue #68).
#[derive(Debug, Clone)]
pub enum QuotaScope {
    /// Pinned divisor (`NEBULA_GATEWAY_QUOTA_MEMBERS`): no coordination.
    Fixed(u32),
    /// Live `/gateways/` membership: rank-ordered shards that re-shard on
    /// join/leave and use the whole limit.
    Live {
        view: Arc<MemberView>,
        self_id: String,
    },
}

impl Default for QuotaScope {
    fn default() -> Self {
        QuotaScope::Fixed(1)
    }
}

/// What to do when the live membership view cannot prove the cluster cap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DegradePolicy {
    /// Refuse the request: an unverifiable quota is not a quota.
    #[default]
    FailClosed,
    /// Serve with the tenant's full (unsharded) limit and count it.
    FailOpen,
}

/// In-process admission state keyed by tenant_id.
///
/// Every limit is enforced on this replica's shard of the cluster quota, so N
/// Gateways never multiply a tenant quota by N. With [`QuotaScope::Live`] the
/// shard follows the `/gateways/` registry (join/leave re-shards; the remainder
/// is handed out by rank so the shards sum to the limit exactly).
#[derive(Debug, Clone)]
pub struct TenantAdmission {
    inner: Arc<Mutex<HashMap<String, TenantLive>>>,
    scope: QuotaScope,
    degrade: DegradePolicy,
    /// Requests served while the membership view could not be trusted.
    degraded_total: Arc<AtomicU64>,
}

impl Default for TenantAdmission {
    fn default() -> Self {
        Self {
            inner: Arc::default(),
            scope: QuotaScope::Fixed(1),
            degrade: DegradePolicy::FailClosed,
            degraded_total: Arc::default(),
        }
    }
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

    /// Split every tenant quota across a pinned `members` count (issue #68).
    pub fn with_members(members: u32) -> Self {
        Self {
            scope: QuotaScope::Fixed(members),
            ..Self::default()
        }
    }

    /// Follow the live `/gateways/` registry instead of a pinned count.
    pub fn with_live_members(mut self, view: Arc<MemberView>, self_id: impl Into<String>) -> Self {
        self.scope = QuotaScope::Live {
            view,
            self_id: self_id.into(),
        };
        self
    }

    /// Policy when the membership view is stale (default: fail closed).
    pub fn with_degrade_policy(mut self, policy: DegradePolicy) -> Self {
        self.degrade = policy;
        self
    }

    /// Requests served under a degraded (untrusted) membership view.
    pub fn degraded_total(&self) -> u64 {
        self.degraded_total.load(Ordering::Relaxed)
    }

    /// Fresh live member count when the scope is live and the view is trusted.
    pub fn live_member_count(&self) -> Option<u32> {
        match &self.scope {
            QuotaScope::Live { view, .. } => view.count(),
            QuotaScope::Fixed(_) => None,
        }
    }

    /// Quotas as enforced by this replica, or `Err` when the cap cannot be proven.
    fn enforced(&self, tenant: &Tenant) -> Result<TenantQuota, TenantDenyCode> {
        match &self.scope {
            QuotaScope::Fixed(members) => Ok(shard_quota(&tenant.quotas, *members)),
            QuotaScope::Live { view, self_id } => match view.shard_for(self_id) {
                Some((rank, count)) => Ok(shard_quota_ranked(&tenant.quotas, rank, count)),
                None => {
                    self.degraded_total.fetch_add(1, Ordering::Relaxed);
                    match self.degrade {
                        DegradePolicy::FailOpen => Ok(tenant.quotas.clone()),
                        DegradePolicy::FailClosed => Err(TenantDenyCode::QuotaUnavailable),
                    }
                }
            },
        }
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

        let quotas = self.enforced(tenant)?;
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

    pub async fn try_admit_pin(&self, tenant: &Tenant) -> Result<(), TenantDenyCode> {
        match admit_static(tenant, None) {
            AdmitDecision::Allow => {}
            AdmitDecision::Deny(code) => return Err(code),
        }
        let Some(limit) = self.enforced(tenant)?.max_pin_requests_per_minute else {
            return Ok(());
        };

        let mut map = self.inner.lock().await;
        let live = map.entry(tenant.tenant_id.clone()).or_default();
        let now = Instant::now();
        reset_minute_window(&mut live.pin_window_start, &mut live.pin_count, now);
        if live.pin_count >= limit {
            return Err(TenantDenyCode::PinAdmissionExceeded);
        }
        live.pin_count += 1;
        Ok(())
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
    tenant
        .map(|t| t.quotas.clone())
        .unwrap_or_else(|| fallback.clone())
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
                max_pin_requests_per_minute: Some(100),
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

    #[tokio::test]
    async fn pin_admission_quota_enforced() {
        let adm = TenantAdmission::new();
        let mut t = tenant(100, 10);
        t.quotas.max_pin_requests_per_minute = Some(1);
        adm.try_admit_pin(&t).await.expect("pin1");
        assert_eq!(
            adm.try_admit_pin(&t).await.err(),
            Some(TenantDenyCode::PinAdmissionExceeded)
        );
    }

    #[tokio::test]
    async fn cluster_quota_shards_across_members() {
        // Two Gateways enforce half of the tenant's RPS each: 5 + 5 = 10, not 20.
        let adm = TenantAdmission::with_members(2);
        let t = tenant(10, 10);
        let mut admitted = 0;
        for _ in 0..10 {
            if adm.try_admit(&t, None, 0).await.is_ok() {
                admitted += 1;
            }
        }
        assert_eq!(admitted, 5);
        assert_eq!(
            adm.try_admit(&t, None, 0).await.err(),
            Some(TenantDenyCode::RpsExceeded)
        );
    }

    fn live(members: &[&str], self_id: &str, stale_ms: u64) -> TenantAdmission {
        let view = Arc::new(MemberView::new(std::time::Duration::from_millis(stale_ms)));
        view.observe(members.iter().map(|m| m.to_string()).collect());
        TenantAdmission::default().with_live_members(view, self_id)
    }

    #[tokio::test]
    async fn live_scope_shards_by_rank_and_uses_the_remainder() {
        // rps=10 across 3 members: rank 0 takes 4, ranks 1..2 take 3.
        let adm = live(&["a", "b", "c"], "a", 10_000);
        let t = tenant(10, 10);
        let mut admitted = 0;
        for _ in 0..10 {
            if adm.try_admit(&t, None, 0).await.is_ok() {
                admitted += 1;
            }
        }
        assert_eq!(admitted, 4, "rank 0 must take the remainder unit");
        assert_eq!(adm.degraded_total(), 0);

        let adm_b = live(&["a", "b", "c"], "b", 10_000);
        let mut admitted_b = 0;
        for _ in 0..10 {
            if adm_b.try_admit(&t, None, 0).await.is_ok() {
                admitted_b += 1;
            }
        }
        assert_eq!(admitted_b, 3);
    }

    #[tokio::test]
    async fn stale_membership_view_fails_closed_by_default() {
        let adm = live(&["a", "b"], "a", 30);
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        let t = tenant(10, 10);
        assert_eq!(
            adm.try_admit(&t, None, 0).await.err(),
            Some(TenantDenyCode::QuotaUnavailable),
            "an unverifiable cap must not admit"
        );
        assert_eq!(adm.degraded_total(), 1);
        // Pin admission takes the same path.
        assert_eq!(
            adm.try_admit_pin(&t).await.err(),
            Some(TenantDenyCode::QuotaUnavailable)
        );
    }

    #[tokio::test]
    async fn stale_view_with_fail_open_serves_at_the_full_limit() {
        let view = Arc::new(MemberView::new(std::time::Duration::from_millis(30)));
        view.observe(vec!["a".into(), "b".into()]);
        let adm = TenantAdmission::default()
            .with_live_members(view, "a")
            .with_degrade_policy(DegradePolicy::FailOpen);
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        let t = tenant(10, 10);
        let mut admitted = 0;
        for _ in 0..12 {
            if adm.try_admit(&t, None, 0).await.is_ok() {
                admitted += 1;
            }
        }
        // Fail-open means the documented cluster limit, not a multiplied one.
        assert_eq!(admitted, 10);
        assert!(adm.degraded_total() >= 10);
    }

    #[tokio::test]
    async fn absent_member_is_never_counted_as_a_shard() {
        // A Gateway whose own key is missing from the view cannot answer "which
        // share am I?" — it must degrade, not guess.
        let adm = live(&["b", "c"], "a", 10_000);
        let t = tenant(10, 10);
        assert_eq!(
            adm.try_admit(&t, None, 0).await.err(),
            Some(TenantDenyCode::QuotaUnavailable)
        );
    }

    #[test]
    fn rate_limit_key_prefers_tenant() {
        assert_eq!(rate_limit_key(Some("t1"), "tok"), "tenant:t1");
        assert_eq!(rate_limit_key(None, "tok"), "token:tok");
    }
}
