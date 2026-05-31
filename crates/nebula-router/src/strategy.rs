use nebula_common::{EndpointInfo, EndpointStats};

/// A candidate endpoint with its optional stats, presented to the routing strategy.
pub struct Candidate<'a> {
    pub endpoint: &'a EndpointInfo,
    pub stats: Option<&'a EndpointStats>,
}

/// Trait for pluggable routing strategies.
/// The Router filters candidates (model_uid match, Ready status, plan_version),
/// then delegates selection to the strategy.
pub trait RoutingStrategy: Send + Sync {
    /// Select one candidate from the list. Returns the index into `candidates`.
    fn select(&self, candidates: &[Candidate]) -> Option<usize>;

    /// Human-readable name for logging / metrics.
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// LeastPending — pick the endpoint with fewest pending requests (current default)
// ---------------------------------------------------------------------------

pub struct LeastPending;

impl RoutingStrategy for LeastPending {
    fn select(&self, candidates: &[Candidate]) -> Option<usize> {
        let mut best_idx: Option<usize> = None;
        let mut best_pending = u64::MAX;

        for (i, c) in candidates.iter().enumerate() {
            let pending = c.stats.map(|s| s.pending_requests).unwrap_or(0);
            if pending < best_pending {
                best_pending = pending;
                best_idx = Some(i);
            }
        }

        best_idx
    }

    fn name(&self) -> &'static str {
        "least_pending"
    }
}

// ---------------------------------------------------------------------------
// LeastKvCache — pick the endpoint with lowest KV cache usage (bytes)
// Falls back to LeastPending when no KV cache data is available.
// ---------------------------------------------------------------------------

pub struct LeastKvCache;

impl RoutingStrategy for LeastKvCache {
    fn select(&self, candidates: &[Candidate]) -> Option<usize> {
        let mut best_idx: Option<usize> = None;
        let mut best_kv = u64::MAX;
        let mut has_kv_data = false;

        for (i, c) in candidates.iter().enumerate() {
            if let Some(used) = c.stats.and_then(|s| s.kv_cache_used_bytes) {
                has_kv_data = true;
                if used < best_kv {
                    best_kv = used;
                    best_idx = Some(i);
                }
            }
        }

        if has_kv_data {
            return best_idx;
        }

        // Fallback: least pending
        LeastPending.select(candidates)
    }

    fn name(&self) -> &'static str {
        "least_kv_cache"
    }
}

// ---------------------------------------------------------------------------
// PrefixCacheAware — pick the endpoint with highest prefix cache hit rate.
// Falls back to LeastPending when hit rate is below threshold or unavailable.
// ---------------------------------------------------------------------------

const PREFIX_CACHE_THRESHOLD: f64 = 0.1;

pub struct PrefixCacheAware;

impl RoutingStrategy for PrefixCacheAware {
    fn select(&self, candidates: &[Candidate]) -> Option<usize> {
        let mut best_idx: Option<usize> = None;
        let mut best_hit_rate: f64 = -1.0;
        let mut has_cache_data = false;

        for (i, c) in candidates.iter().enumerate() {
            if let Some(hit_rate) = c.stats.and_then(|s| s.prefix_cache_hit_rate) {
                has_cache_data = true;
                if hit_rate > best_hit_rate {
                    best_hit_rate = hit_rate;
                    best_idx = Some(i);
                }
            }
        }

        if has_cache_data && best_hit_rate >= PREFIX_CACHE_THRESHOLD {
            return best_idx;
        }

        // Fallback: least pending
        LeastPending.select(candidates)
    }

    fn name(&self) -> &'static str {
        "prefix_cache_aware"
    }
}

/// Parse a strategy name string into a boxed strategy.
pub fn parse_strategy(name: &str) -> Result<Box<dyn RoutingStrategy>, String> {
    match name {
        "least_pending" => Ok(Box::new(LeastPending)),
        "least_kv_cache" => Ok(Box::new(LeastKvCache)),
        "prefix_cache_aware" => Ok(Box::new(PrefixCacheAware)),
        other => Err(format!(
            "unknown routing strategy '{}', available: least_pending, least_kv_cache, prefix_cache_aware",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_common::{EndpointInfo, EndpointKind, EndpointStats, EndpointStatus};

    fn make_ep(model: &str, replica: u32) -> EndpointInfo {
        EndpointInfo {
            model_uid: model.to_string(),
            replica_id: replica,
            plan_version: 1,
            node_id: "n1".to_string(),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".to_string(),
            status: EndpointStatus::Ready,
            last_heartbeat_ms: 0,
            grpc_target: None,
            base_url: Some("http://127.0.0.1:8000".to_string()),
        }
    }

    fn make_stats(
        model: &str,
        replica: u32,
        pending: u64,
        kv_used: Option<u64>,
        prefix_hit: Option<f64>,
    ) -> EndpointStats {
        EndpointStats {
            model_uid: model.to_string(),
            replica_id: replica,
            last_updated_ms: 0,
            pending_requests: pending,
            prefix_cache_hit_rate: prefix_hit,
            prompt_cache_hit_rate: None,
            kv_cache_used_bytes: kv_used,
            kv_cache_free_bytes: None,
        }
    }

    #[test]
    fn test_least_pending() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        let s0 = make_stats("m", 0, 10, None, None);
        let s1 = make_stats("m", 1, 3, None, None);

        let candidates = vec![
            Candidate {
                endpoint: &ep0,
                stats: Some(&s0),
            },
            Candidate {
                endpoint: &ep1,
                stats: Some(&s1),
            },
        ];

        assert_eq!(LeastPending.select(&candidates), Some(1));
    }

    #[test]
    fn test_least_kv_cache() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        let s0 = make_stats("m", 0, 1, Some(8000), None);
        let s1 = make_stats("m", 1, 10, Some(2000), None);

        let candidates = vec![
            Candidate {
                endpoint: &ep0,
                stats: Some(&s0),
            },
            Candidate {
                endpoint: &ep1,
                stats: Some(&s1),
            },
        ];

        assert_eq!(LeastKvCache.select(&candidates), Some(1));
    }

    #[test]
    fn test_least_kv_cache_fallback() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        let s0 = make_stats("m", 0, 10, None, None);
        let s1 = make_stats("m", 1, 3, None, None);

        let candidates = vec![
            Candidate {
                endpoint: &ep0,
                stats: Some(&s0),
            },
            Candidate {
                endpoint: &ep1,
                stats: Some(&s1),
            },
        ];

        // No KV data → falls back to least pending
        assert_eq!(LeastKvCache.select(&candidates), Some(1));
    }

    #[test]
    fn test_prefix_cache_aware() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        let s0 = make_stats("m", 0, 1, None, Some(0.8));
        let s1 = make_stats("m", 1, 1, None, Some(0.3));

        let candidates = vec![
            Candidate {
                endpoint: &ep0,
                stats: Some(&s0),
            },
            Candidate {
                endpoint: &ep1,
                stats: Some(&s1),
            },
        ];

        assert_eq!(PrefixCacheAware.select(&candidates), Some(0));
    }

    #[test]
    fn test_prefix_cache_aware_fallback_low_hit_rate() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        let s0 = make_stats("m", 0, 10, None, Some(0.01));
        let s1 = make_stats("m", 1, 3, None, Some(0.02));

        let candidates = vec![
            Candidate {
                endpoint: &ep0,
                stats: Some(&s0),
            },
            Candidate {
                endpoint: &ep1,
                stats: Some(&s1),
            },
        ];

        // All below threshold → falls back to least pending (index 1)
        assert_eq!(PrefixCacheAware.select(&candidates), Some(1));
    }
}
