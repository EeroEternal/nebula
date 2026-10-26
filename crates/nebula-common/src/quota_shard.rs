//! Cluster quota sharding (issue #68): N Gateway replicas must not multiply a
//! tenant quota N times.
//!
//! Every Gateway enforces `limit / members`, so the shards summed over the
//! declared member count can never exceed the configured limit. The integer
//! remainder is deliberately unused — under-serving is visible, over-serving is
//! a correctness bug.
//!
//! shortcut: the member count is a deployment invariant (env), not live
//! membership; auto re-shard on Gateway join/leave needs the `/gateways/`
//! lease registry (HA-P2 follow-up).

use crate::tenant::TenantQuota;

/// Gateway replica count the cluster tenant quota is split across.
pub const ENV_QUOTA_MEMBERS: &str = "NEBULA_GATEWAY_QUOTA_MEMBERS";

/// Declared member count. Unset / `0` / unparseable → 1 (single-Gateway
/// behaviour, exactly as before sharding).
pub fn members_from_env() -> u32 {
    std::env::var(ENV_QUOTA_MEMBERS)
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(1)
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
    fn tiny_limit_denies_rather_than_oversells() {
        // One slot cannot be split: each Gateway admits nothing, so the cluster
        // stays under the limit instead of admitting N×.
        let s = shard_quota(&quota(1, 1, 10, 1), 2);
        assert_eq!(s.max_concurrency, Some(0));
    }
}
