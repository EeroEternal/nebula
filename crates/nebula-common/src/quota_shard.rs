//! Cluster quota sharding (issue #68): N Gateway replicas must not multiply a
//! tenant quota N times.
//!
//! Every Gateway enforces `limit / members`, so the shards summed over the
//! declared member count can never exceed the configured limit. The integer
//! remainder is deliberately unused — under-serving is visible, over-serving is
//! a correctness bug.
//!
//! Two ways to obtain the divisor:
//! - `NEBULA_GATEWAY_QUOTA_MEMBERS=N` pins it (air-gapped / fixed topology), or
//! - a live [`MemberView`] fed by the `/gateways/` lease registry, which
//!   re-shards on join/leave and hands out the integer remainder by rank so the
//!   shards sum to exactly the limit instead of `n * floor(limit / n)`.
//!
//! Safety rules baked into [`MemberView`]: an empty/stale view yields `None`
//! (the caller must degrade explicitly, never silently), and a member that is
//! not in the view yields `None` too.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::tenant::TenantQuota;

/// Gateway replica count the cluster tenant quota is split across.
pub const ENV_QUOTA_MEMBERS: &str = "NEBULA_GATEWAY_QUOTA_MEMBERS";

/// Pinned divisor: `Some(n)` only when `NEBULA_GATEWAY_QUOTA_MEMBERS` is set to a
/// positive count. Unset means "follow the `/gateways/` registry".
pub fn members_env_override() -> Option<u32> {
    std::env::var(ENV_QUOTA_MEMBERS)
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
}

/// Declared member count with the historical default (`1`).
pub fn members_from_env() -> u32 {
    members_env_override().unwrap_or(1)
}

/// Floor-divide every enforced limit across `members` Gateways. `allowed_models`
/// is a policy gate rather than a budget and is kept as-is.
pub fn shard_quota(quotas: &TenantQuota, members: u32) -> TenantQuota {
    let n = u64::from(members.max(1));
    if n == 1 {
        return quotas.clone();
    }
    let shard = |limit: Option<u64>| limit.map(|l| l / n);
    TenantQuota {
        rps_per_minute: shard(quotas.rps_per_minute),
        max_concurrency: shard(quotas.max_concurrency),
        max_tokens_per_minute: shard(quotas.max_tokens_per_minute),
        max_pin_requests_per_minute: shard(quotas.max_pin_requests_per_minute),
        allowed_models: quotas.allowed_models.clone(),
    }
}

/// Ranked shard of a tenant quota: members at rank `< limit % n` take one extra
/// unit, so the shards sum to exactly `limit` (nothing is wasted and nothing is
/// oversold).
pub fn shard_quota_ranked(quotas: &TenantQuota, rank: u32, count: u32) -> TenantQuota {
    let n = u64::from(count.max(1));
    if n == 1 {
        return quotas.clone();
    }
    let shard = |limit: Option<u64>| limit.map(|l| l / n + u64::from(u64::from(rank) < l % n));
    TenantQuota {
        rps_per_minute: shard(quotas.rps_per_minute),
        max_concurrency: shard(quotas.max_concurrency),
        max_tokens_per_minute: shard(quotas.max_tokens_per_minute),
        max_pin_requests_per_minute: shard(quotas.max_pin_requests_per_minute),
        allowed_models: quotas.allowed_models.clone(),
    }
}

#[derive(Debug, Clone)]
struct MemberSnapshot {
    /// Sorted, de-duplicated instance ids.
    members: Vec<String>,
    observed: Instant,
}

/// In-process snapshot of the `/gateways/` registry, refreshed by the Gateway.
///
/// `shard_for` refuses to answer from a view older than `stale_after`: a stale
/// membership list cannot prove the cluster-wide cap, and guessing is exactly
/// how a quota silently becomes N×.
#[derive(Debug)]
pub struct MemberView {
    stale_after: Duration,
    snapshot: Mutex<Option<MemberSnapshot>>,
}

impl MemberView {
    pub fn new(stale_after: Duration) -> Self {
        Self {
            stale_after,
            snapshot: Mutex::new(None),
        }
    }

    /// Replace the view. Expired members must already be filtered out.
    pub fn observe(&self, mut members: Vec<String>) {
        members.sort();
        members.dedup();
        let snapshot = MemberSnapshot {
            members,
            observed: Instant::now(),
        };
        *self.snapshot.lock().unwrap_or_else(|e| e.into_inner()) = Some(snapshot);
    }

    /// `(rank, count)` for `id`, or `None` when the view is missing, stale, or
    /// does not contain `id` (which means it is not a consistent view of self).
    pub fn shard_for(&self, id: &str) -> Option<(u32, u32)> {
        let guard = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        let snap = guard.as_ref()?;
        if snap.observed.elapsed() > self.stale_after {
            return None;
        }
        let rank = snap.members.iter().position(|m| m == id)?;
        Some((rank as u32, snap.members.len() as u32))
    }

    /// Fresh member count, or `None` when the view cannot be trusted.
    pub fn count(&self) -> Option<u32> {
        let guard = self.snapshot.lock().unwrap_or_else(|e| e.into_inner());
        let snap = guard.as_ref()?;
        if snap.observed.elapsed() > self.stale_after {
            return None;
        }
        Some(snap.members.len() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quota(rps: u64, conc: u64, tokens: u64, pins: u64) -> TenantQuota {
        TenantQuota {
            rps_per_minute: Some(rps),
            max_concurrency: Some(conc),
            max_tokens_per_minute: Some(tokens),
            max_pin_requests_per_minute: Some(pins),
            allowed_models: Some(vec!["m".into()]),
        }
    }

    #[test]
    fn single_member_is_unchanged() {
        let q = quota(10, 4, 1000, 7);
        assert_eq!(shard_quota(&q, 1), q);
        // 0 = not configured → identity, never a division by zero.
        assert_eq!(shard_quota(&q, 0), q);
    }

    #[test]
    fn shards_never_exceed_the_global_limit() {
        let q = quota(10, 5, 1001, 7);
        for members in 1..=6u32 {
            let s = shard_quota(&q, members);
            let m = u64::from(members);
            assert!(s.rps_per_minute.unwrap() * m <= 10);
            assert!(s.max_concurrency.unwrap() * m <= 5);
            assert!(s.max_tokens_per_minute.unwrap() * m <= 1001);
            assert!(s.max_pin_requests_per_minute.unwrap() * m <= 7);
            // Policy gate survives sharding.
            assert_eq!(s.allowed_models, q.allowed_models);
        }
        let s = shard_quota(&q, 2);
        assert_eq!(s.rps_per_minute, Some(5));
        assert_eq!(s.max_concurrency, Some(2));
        assert_eq!(s.max_tokens_per_minute, Some(500));
        assert_eq!(s.max_pin_requests_per_minute, Some(3));
    }

    #[test]
    fn unspecified_limits_stay_unspecified() {
        let q = TenantQuota::default();
        let s = shard_quota(&q, 4);
        assert_eq!(s, TenantQuota::default());
    }

    #[test]
    fn ranked_shards_use_the_whole_limit() {
        // rank < limit % n takes the extra unit, so the sum is exactly the limit.
        for (limit, n) in [(10u64, 3u32), (4, 2), (7, 4), (1, 2), (9, 3)] {
            let q = TenantQuota {
                rps_per_minute: Some(limit),
                ..TenantQuota::default()
            };
            let total: u64 = (0..n)
                .map(|rank| shard_quota_ranked(&q, rank, n).rps_per_minute.unwrap())
                .sum();
            assert_eq!(
                total, limit,
                "limit={limit} n={n} shards must sum to the limit"
            );
        }
        // 10 across 3 members: 4 / 3 / 3 (not 3 / 3 / 3).
        let q = quota(10, 1, 1, 1);
        assert_eq!(shard_quota_ranked(&q, 0, 3).rps_per_minute, Some(4));
        assert_eq!(shard_quota_ranked(&q, 1, 3).rps_per_minute, Some(3));
        assert_eq!(shard_quota_ranked(&q, 2, 3).rps_per_minute, Some(3));
    }

    #[test]
    fn member_view_refuses_stale_or_unknown_members() {
        let view = MemberView::new(Duration::from_millis(50));
        assert_eq!(view.shard_for("a"), None, "never observed");
        view.observe(vec!["b".into(), "a".into(), "a".into()]);
        assert_eq!(view.shard_for("a"), Some((0, 2)));
        assert_eq!(view.shard_for("b"), Some((1, 2)));
        assert_eq!(view.shard_for("c"), None, "not a member of this view");
        assert_eq!(view.count(), Some(2));
        std::thread::sleep(Duration::from_millis(70));
        assert_eq!(view.shard_for("a"), None, "stale view must not answer");
        assert_eq!(view.count(), None);
        view.observe(vec!["a".into()]);
        assert_eq!(
            view.shard_for("a"),
            Some((0, 1)),
            "single member owns the limit"
        );
    }

    #[test]
    fn tiny_limit_denies_rather_than_oversells() {
        // One slot cannot be split: each Gateway admits nothing, so the cluster
        // stays under the limit instead of admitting N×.
        let s = shard_quota(&quota(1, 1, 10, 1), 2);
        assert_eq!(s.max_concurrency, Some(0));
    }
}
