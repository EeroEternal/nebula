use nebula_common::{EndpointInfo, EndpointStats};

/// A candidate endpoint with its optional stats, presented to the routing strategy.
pub struct Candidate<'a> {
    pub endpoint: &'a EndpointInfo,
    pub stats: Option<&'a EndpointStats>,
}

/// Tunable weights and aggregation bounds for [`PrefixAffinity`].
///
/// Defaults match the baseline constants (`waiting=96`, `running=64`, `kv=16`,
/// `preemption=32`, `max_owners=2`, `owner_slack=4`) so behaviour is backward
/// compatible when `NEBULA_PREFIX_*` environment variables are unset.
#[derive(Debug, Clone, Copy)]
pub struct PrefixAffinityTuning {
    pub wait_cost_waiting: f64,
    pub wait_cost_running: f64,
    pub kv_usage_penalty: f64,
    pub preemption_penalty: f64,
    /// Target number of primary Rendezvous replicas to consolidate a shared prefix onto.
    /// `0` disables Rendezvous consolidation / cold-prefix packing.
    pub max_owners: usize,
    /// Maximum extra `pending_requests` above cluster `min_pending` tolerated on the
    /// `max_owners` Rendezvous subset before spilling cold traffic to other replicas.
    pub owner_load_slack: u64,
}

impl Default for PrefixAffinityTuning {
    fn default() -> Self {
        Self {
            wait_cost_waiting: QUEUE_WAIT_COST_PER_WAITING,
            wait_cost_running: QUEUE_WAIT_COST_PER_PENDING,
            kv_usage_penalty: KV_USAGE_PENALTY_WEIGHT,
            preemption_penalty: PREEMPTION_PENALTY_WEIGHT,
            max_owners: DEFAULT_PREFIX_MAX_OWNERS,
            owner_load_slack: DEFAULT_PREFIX_OWNER_SLACK,
        }
    }
}

impl PrefixAffinityTuning {
    pub fn from_env() -> Self {
        let f = |key: &str, def: f64| -> f64 {
            std::env::var(key)
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(def)
        };
        let mut out = Self::default();
        out.wait_cost_waiting = f("NEBULA_PREFIX_WAIT_COST_WAITING", out.wait_cost_waiting);
        out.wait_cost_running = f("NEBULA_PREFIX_WAIT_COST_RUNNING", out.wait_cost_running);
        out.kv_usage_penalty = f("NEBULA_PREFIX_KV_PENALTY", out.kv_usage_penalty);
        out.preemption_penalty = f("NEBULA_PREFIX_PREEMPTION_PENALTY", out.preemption_penalty);
        if let Some(k) = std::env::var("NEBULA_PREFIX_MAX_OWNERS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
        {
            out.max_owners = k;
        }
        if let Some(slack) = std::env::var("NEBULA_PREFIX_OWNER_SLACK")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
        {
            out.owner_load_slack = slack;
        }
        out
    }
}

/// Deterministic SplitMix64 Rendezvous (Highest Random Weight) score for `(seed, replica_id)`.
#[inline]
pub fn rendezvous_score(seed: u64, replica_id: u32) -> u64 {
    let mut z = seed ^ ((replica_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Hash an arbitrary string key (e.g. `prefer_prefix_key` or `(tenant_id, session_id)`)
/// into a deterministic 64-bit Rendezvous seed (FNV-1a 64-bit).
pub fn seed_from_bytes(parts: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for &b in *part {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Select a candidate from the top `max_owners` Rendezvous-ranked replicas for `seed`
/// whose `pending_requests <= min_cluster_pending + load_slack`, preferring lower queue
/// load within that bounded subset and breaking ties by Rendezvous rank.
/// Returns `None` when `candidates` is empty or `max_owners == 0`, or spills to overall
/// `LeastPending` when every top-K owner exceeds the load bound.
pub fn select_bounded_rendezvous(
    candidates: &[Candidate<'_>],
    seed: u64,
    max_owners: usize,
    load_slack: u64,
) -> Option<usize> {
    if candidates.is_empty() || max_owners == 0 {
        return None;
    }
    let k = max_owners.min(candidates.len());
    let mut ranked: Vec<(usize, u64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (i, rendezvous_score(seed, c.endpoint.replica_id)))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let min_pending = candidates
        .iter()
        .map(|c| c.stats.map(|s| s.pending_requests).unwrap_or(0))
        .min()
        .unwrap_or(0);
    let ceiling = min_pending.saturating_add(load_slack);

    let mut best_in_subset: Option<(usize, u64, u64, f64, usize)> = None;
    for (rank, &(idx, _hrw)) in ranked.iter().take(k).enumerate() {
        let c = &candidates[idx];
        let pending = c.stats.map(|s| s.pending_requests).unwrap_or(0);
        if pending > ceiling {
            continue;
        }
        let waiting = c.stats.and_then(|s| s.waiting_requests).unwrap_or(pending);
        let kv = c.stats.and_then(|s| s.kv_cache_usage).unwrap_or(0.0);
        let better = match best_in_subset {
            None => true,
            Some((_, bp, bw, bkv, brank)) => {
                if pending != bp {
                    pending < bp
                } else if waiting != bw {
                    waiting < bw
                } else if (kv - bkv).abs() > 1e-6 {
                    kv < bkv
                } else {
                    rank < brank
                }
            }
        };
        if better {
            best_in_subset = Some((idx, pending, waiting, kv, rank));
        }
    }

    if let Some((idx, _, _, _, _)) = best_in_subset {
        Some(idx)
    } else {
        LeastPending
            .select(&SelectionCtx::none(), candidates)
            .map(|s| s.index)
    }
}

/// Request-scoped routing context (P1c / Batch 5.2): the primary replica holding
/// the request's KV prefix chain, per-replica matched block counts across
/// all candidate owners, the model's prefix block granularity (`block_size`,
/// defaulting to 16 tokens when 0), optional Rendezvous seed for node aggregation,
/// optional sliding-window active preemption set, and tunable cost weights.
pub struct SelectionCtx<'a> {
    pub prefix_owner: Option<u32>,
    pub prefix_matches: Option<&'a std::collections::HashMap<u32, usize>>,
    pub block_size: usize,
    /// Optional 64-bit seed derived from the shared prefix root block (or session/hint)
    /// to pack cold bursts and reconverge fragmented multi-owners onto `max_owners` nodes.
    pub prefix_key_seed: Option<u64>,
    /// When `Some`, identifies replicas with sliding-window preemption increases (`delta > 0`),
    /// avoiding permanent penalties from monotonic `num_preemptions_total` counters.
    pub active_preemptions: Option<&'a std::collections::HashSet<u32>>,
    pub tuning: PrefixAffinityTuning,
}

impl SelectionCtx<'_> {
    pub fn none() -> Self {
        Self {
            prefix_owner: None,
            prefix_matches: None,
            block_size: 16,
            prefix_key_seed: None,
            active_preemptions: None,
            tuning: PrefixAffinityTuning::default(),
        }
    }

    pub fn tokens_per_block(&self) -> f64 {
        if self.block_size == 0 {
            TOKENS_PER_KV_BLOCK
        } else {
            self.block_size as f64
        }
    }
}

/// Result of a strategy selection: the chosen candidate index plus the
/// reason when the primary signal was unusable and the strategy fell back to
/// `LeastPending` (`"no_signal"` / `"below_threshold"` /
/// `"owner_unavailable"` / `"owner_congested"`), so fallbacks are observable
/// instead of silent.
pub struct Selection {
    pub index: usize,
    pub fallback_reason: Option<&'static str>,
}

/// Trait for pluggable routing strategies.
/// The Router filters candidates (model_uid match, Ready status, plan_version),
/// then delegates selection to the strategy.
pub trait RoutingStrategy: Send + Sync {
    /// Select one candidate from the list.
    fn select(&self, ctx: &SelectionCtx<'_>, candidates: &[Candidate]) -> Option<Selection>;

    /// Human-readable name for logging / metrics.
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// LeastPending — pick the endpoint with fewest pending requests (current default)
// ---------------------------------------------------------------------------

pub struct LeastPending;

impl RoutingStrategy for LeastPending {
    fn select(&self, _ctx: &SelectionCtx<'_>, candidates: &[Candidate]) -> Option<Selection> {
        let mut best_idx: Option<usize> = None;
        let mut best_pending = u64::MAX;
        let mut best_waiting = u64::MAX;
        let mut best_kv = f64::MAX;

        for (i, c) in candidates.iter().enumerate() {
            let pending = c.stats.map(|s| s.pending_requests).unwrap_or(0);
            let waiting = c.stats.and_then(|s| s.waiting_requests).unwrap_or(pending);
            let kv = c.stats.and_then(|s| s.kv_cache_usage).unwrap_or(f64::MAX);
            let is_better = if pending != best_pending {
                pending < best_pending
            } else if waiting != best_waiting {
                waiting < best_waiting
            } else {
                kv < best_kv
            };
            if is_better {
                best_pending = pending;
                best_waiting = waiting;
                best_kv = kv;
                best_idx = Some(i);
            }
        }

        best_idx.map(|index| Selection {
            index,
            fallback_reason: None,
        })
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
    fn select(&self, ctx: &SelectionCtx<'_>, candidates: &[Candidate]) -> Option<Selection> {
        let mut best_idx: Option<usize> = None;
        let mut best_kv = f64::MAX;
        let mut has_kv_data = false;

        for (i, c) in candidates.iter().enumerate() {
            if let Some(usage) = c.stats.and_then(|s| s.kv_cache_usage) {
                has_kv_data = true;
                if usage < best_kv {
                    best_kv = usage;
                    best_idx = Some(i);
                }
            }
        }

        if has_kv_data {
            return best_idx.map(|index| Selection {
                index,
                fallback_reason: None,
            });
        }

        // Fallback: least pending
        LeastPending.select(ctx, candidates).map(|mut s| {
            s.fallback_reason = Some("no_signal");
            s
        })
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
    fn select(&self, ctx: &SelectionCtx<'_>, candidates: &[Candidate]) -> Option<Selection> {
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
            return best_idx.map(|index| Selection {
                index,
                fallback_reason: None,
            });
        }

        // Fallback: least pending
        let reason: &'static str = if has_cache_data {
            "below_threshold"
        } else {
            "no_signal"
        };
        LeastPending.select(ctx, candidates).map(|mut s| {
            s.fallback_reason = Some(reason);
            s
        })
    }

    fn name(&self) -> &'static str {
        "prefix_cache_aware"
    }
}

// ---------------------------------------------------------------------------
// PrefixAffinity (P1c + Batch 5.2) — route by joint cost function:
//   Score(w) = SavedPrefillTokens(w) - QueueWaitCost(w) - KvUsagePenalty(w)
// across multi-owner prefix candidates, spilling over to an idle replica with
// `fallback_reason = Some("owner_congested")` when all prefix owners are
// queued deeper than the prefill savings justify.
// ---------------------------------------------------------------------------

const TOKENS_PER_KV_BLOCK: f64 = 16.0;
const QUEUE_WAIT_COST_PER_PENDING: f64 = 64.0;
/// Head-of-Line waiting requests (`num_requests_waiting`, blocked behind an
/// incomplete chunked prefill or KV admission) incur a steeper latency cost
/// than requests already running in the GPU batch (TokenSpeed `scheduler.md` §1.1).
const QUEUE_WAIT_COST_PER_WAITING: f64 = 96.0;
const KV_USAGE_PENALTY_WEIGHT: f64 = 16.0;
const PREEMPTION_PENALTY_WEIGHT: f64 = 32.0;
pub const DEFAULT_PREFIX_MAX_OWNERS: usize = 2;
pub const DEFAULT_PREFIX_OWNER_SLACK: u64 = 4;
const MAX_CONSOLIDATION_BONUS_TOKENS: f64 = 24.0;

pub struct PrefixAffinity;

impl RoutingStrategy for PrefixAffinity {
    fn select(&self, ctx: &SelectionCtx<'_>, candidates: &[Candidate]) -> Option<Selection> {
        if candidates.is_empty() {
            return None;
        }
        let has_any_signal = ctx.prefix_owner.is_some()
            || ctx.prefix_matches.map(|m| !m.is_empty()).unwrap_or(false);
        if !has_any_signal {
            // Cold prefix with a known root block hash: pack onto the deterministic
            // `max_owners` Rendezvous subset (within bounded load slack) instead of
            // spraying concurrent cold requests across every node in the cluster.
            if let Some(seed) = ctx.prefix_key_seed {
                if let Some(index) = select_bounded_rendezvous(
                    candidates,
                    seed,
                    ctx.tuning.max_owners,
                    ctx.tuning.owner_load_slack,
                ) {
                    return Some(Selection {
                        index,
                        fallback_reason: Some("no_signal"),
                    });
                }
            }
            return LeastPending.select(ctx, candidates).map(|mut s| {
                s.fallback_reason = Some("no_signal");
                s
            });
        }

        let matched_blocks_for = |replica_id: u32| -> usize {
            if let Some(m) = ctx.prefix_matches {
                if let Some(&blocks) = m.get(&replica_id) {
                    return blocks;
                }
            }
            if ctx.prefix_owner == Some(replica_id) {
                1
            } else {
                0
            }
        };

        let any_owner_available = candidates
            .iter()
            .any(|c| matched_blocks_for(c.endpoint.replica_id) > 0);
        if !any_owner_available {
            // Owner was filtered out (draining / circuit / stale plan).
            if let Some(seed) = ctx.prefix_key_seed {
                if let Some(index) = select_bounded_rendezvous(
                    candidates,
                    seed,
                    ctx.tuning.max_owners,
                    ctx.tuning.owner_load_slack,
                ) {
                    return Some(Selection {
                        index,
                        fallback_reason: Some("owner_unavailable"),
                    });
                }
            }
            return LeastPending.select(ctx, candidates).map(|mut s| {
                s.fallback_reason = Some("owner_unavailable");
                s
            });
        }

        // Compute Rendezvous rank per replica when `prefix_key_seed` and `max_owners`
        // are active so fragmented multi-owners reconverge onto the primary K nodes
        // once transient queue pressure subsides.
        let rendezvous_ranks: Option<std::collections::HashMap<u32, usize>> =
            ctx.prefix_key_seed.and_then(|seed| {
                if ctx.tuning.max_owners == 0 {
                    return None;
                }
                let mut order: Vec<(u32, u64)> = candidates
                    .iter()
                    .map(|c| {
                        (
                            c.endpoint.replica_id,
                            rendezvous_score(seed, c.endpoint.replica_id),
                        )
                    })
                    .collect();
                order.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                Some(
                    order
                        .into_iter()
                        .enumerate()
                        .map(|(rank, (rid, _))| (rid, rank))
                        .collect(),
                )
            });

        let mut best_idx = 0usize;
        let mut best_score = f64::NEG_INFINITY;
        let mut best_blocks = 0usize;

        let tokens_per_block = ctx.tokens_per_block();
        let tuning = &ctx.tuning;
        for (i, c) in candidates.iter().enumerate() {
            let rid = c.endpoint.replica_id;
            let blocks = matched_blocks_for(rid);
            let saved_tokens = (blocks as f64) * tokens_per_block;
            let consolidation_bonus = if blocks > 0 {
                rendezvous_ranks
                    .as_ref()
                    .and_then(|m| m.get(&rid).copied())
                    .filter(|&rank| rank < tuning.max_owners)
                    .map(|rank| {
                        let cap = (saved_tokens * 0.25).min(MAX_CONSOLIDATION_BONUS_TOKENS);
                        cap / ((rank + 1) as f64)
                    })
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            let queue_cost = match c
                .stats
                .and_then(|s| Some((s.waiting_requests?, s.running_requests?)))
            {
                Some((waiting, running)) => {
                    tuning.wait_cost_waiting * (waiting as f64)
                        + tuning.wait_cost_running * (running as f64)
                }
                None => {
                    let pending = c.stats.map(|s| s.pending_requests).unwrap_or(0) as f64;
                    tuning.wait_cost_running * pending
                }
            };
            let kv_usage = c
                .stats
                .and_then(|s| s.kv_cache_usage)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let is_preempting = match ctx.active_preemptions {
                Some(active) => active.contains(&rid),
                None => c.stats.and_then(|s| s.num_preemptions_total).unwrap_or(0) > 0,
            };
            let preemption_penalty = if is_preempting {
                tuning.preemption_penalty
            } else {
                0.0
            };
            let score = saved_tokens + consolidation_bonus
                - queue_cost
                - tuning.kv_usage_penalty * kv_usage
                - preemption_penalty;
            // Tie-break: higher score > deeper matched blocks > primary prefix_owner > lower index
            let is_better = if (score - best_score).abs() > 1e-9 {
                score > best_score
            } else if blocks != best_blocks {
                blocks > best_blocks
            } else {
                ctx.prefix_owner == Some(rid)
            };
            if is_better {
                best_score = score;
                best_blocks = blocks;
                best_idx = i;
            }
        }

        let fallback_reason = if best_blocks == 0 {
            Some("owner_congested")
        } else {
            None
        };
        Some(Selection {
            index: best_idx,
            fallback_reason,
        })
    }

    fn name(&self) -> &'static str {
        "prefix_affinity"
    }
}

/// Parse a strategy name string into a boxed strategy.
pub fn parse_strategy(name: &str) -> Result<Box<dyn RoutingStrategy>, String> {
    match name {
        "least_pending" => Ok(Box::new(LeastPending)),
        "least_kv_cache" => Ok(Box::new(LeastKvCache)),
        "prefix_cache_aware" => Ok(Box::new(PrefixCacheAware)),
        "prefix_affinity" => Ok(Box::new(PrefixAffinity)),
        other => Err(format!(
            "unknown routing strategy '{}', available: least_pending, least_kv_cache, prefix_cache_aware, prefix_affinity",
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
            status_detail: None,
            grpc_target: None,
            base_url: Some("http://127.0.0.1:8000".to_string()),
            engine_type: None,
            role: Default::default(),
        }
    }

    fn make_stats(
        model: &str,
        replica: u32,
        pending: u64,
        kv_usage: Option<f64>,
        prefix_hit: Option<f64>,
    ) -> EndpointStats {
        EndpointStats {
            model_uid: model.to_string(),
            replica_id: replica,
            last_updated_ms: 0,
            pending_requests: pending,
            prefix_cache_hit_rate: prefix_hit,
            kv_cache_usage: kv_usage,
            ..Default::default()
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

        assert_eq!(
            LeastPending
                .select(&SelectionCtx::none(), &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((1, None))
        );
    }

    #[test]
    fn test_least_kv_cache() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        let s0 = make_stats("m", 0, 1, Some(0.8), None);
        let s1 = make_stats("m", 1, 10, Some(0.2), None);

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

        assert_eq!(
            LeastKvCache
                .select(&SelectionCtx::none(), &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((1, None))
        );
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
        assert_eq!(
            LeastKvCache
                .select(&SelectionCtx::none(), &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((1, Some("no_signal")))
        );
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

        assert_eq!(
            PrefixCacheAware
                .select(&SelectionCtx::none(), &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((0, None))
        );
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
        assert_eq!(
            PrefixCacheAware
                .select(&SelectionCtx::none(), &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((1, Some("below_threshold")))
        );
    }

    #[test]
    fn test_prefix_cache_aware_fallback_no_signal() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        // Stats present but prefix_cache_hit_rate None (e.g. broken metric
        // parsing) → no signal, must fall back with a visible reason.
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

        assert_eq!(
            PrefixCacheAware
                .select(&SelectionCtx::none(), &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((1, Some("no_signal")))
        );
    }

    #[test]
    fn test_prefix_affinity_joint_cost_spills_congested_short_owner_and_retains_deep_owner() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);
        // Replica 0 owns 1 block (16 tokens) but has 3 pending requests (cost = 192);
        // Replica 1 is completely idle (0 pending).
        let s0 = make_stats("m", 0, 3, Some(0.4), None);
        let s1 = make_stats("m", 1, 0, Some(0.1), None);
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

        let mut short_matches = std::collections::HashMap::new();
        short_matches.insert(0u32, 1usize);
        let ctx_short = SelectionCtx {
            prefix_owner: Some(0),
            prefix_matches: Some(&short_matches),
            block_size: 16,
            ..SelectionCtx::none()
        };
        assert_eq!(
            PrefixAffinity
                .select(&ctx_short, &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((1, Some("owner_congested")))
        );

        // When Replica 0 holds a deep prefix (32 blocks = 512 tokens), saved prefill
        // (512) outweighs 3 pending requests (192), so it stays on Replica 0.
        let mut deep_matches = std::collections::HashMap::new();
        deep_matches.insert(0u32, 32usize);
        let ctx_deep = SelectionCtx {
            prefix_owner: Some(0),
            prefix_matches: Some(&deep_matches),
            block_size: 16,
            ..SelectionCtx::none()
        };
        assert_eq!(
            PrefixAffinity
                .select(&ctx_deep, &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((0, None))
        );

        // With 64-token blocks (e.g. MLA / TokenSpeed prefix_granularity = 64),
        // 4 matched blocks = 256 saved tokens > 3 pending requests (cost = 192),
        // whereas at 16-token blocks 4 blocks = 64 tokens < 192 would have spilled.
        let mut mla_matches = std::collections::HashMap::new();
        mla_matches.insert(0u32, 4usize);
        let ctx_mla = SelectionCtx {
            prefix_owner: Some(0),
            prefix_matches: Some(&mla_matches),
            block_size: 64,
            ..SelectionCtx::none()
        };
        assert_eq!(
            PrefixAffinity
                .select(&ctx_mla, &candidates)
                .map(|s| (s.index, s.fallback_reason)),
            Some((0, None))
        );
    }

    #[test]
    fn test_waiting_vs_running_and_preemption_penalties() {
        let ep0 = make_ep("m", 0);
        let ep1 = make_ep("m", 1);

        // Both replicas have pending_requests = 2, but Replica 0 has 2 waiting
        // (HoL blocked behind chunked prefill), while Replica 1 has 0 waiting
        // and 2 running in batch. LeastPending must prefer Replica 1.
        let mut s0 = make_stats("m", 0, 2, Some(0.3), None);
        s0.waiting_requests = Some(2);
        s0.running_requests = Some(0);

        let mut s1 = make_stats("m", 1, 2, Some(0.3), None);
        s1.waiting_requests = Some(0);
        s1.running_requests = Some(2);

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
        assert_eq!(
            LeastPending
                .select(&SelectionCtx::none(), &candidates)
                .map(|s| s.index),
            Some(1)
        );

        // Under PrefixAffinity, if both hold 4 blocks (64 tokens), Replica 1
        // (running batch, lower queue cost and 0 preemptions) beats Replica 0
        // (HoL waiting queue + active preemptions).
        s0.num_preemptions_total = Some(2);
        let candidates_aff = vec![
            Candidate {
                endpoint: &ep0,
                stats: Some(&s0),
            },
            Candidate {
                endpoint: &ep1,
                stats: Some(&s1),
            },
        ];
        let mut both_matches = std::collections::HashMap::new();
        both_matches.insert(0u32, 4usize);
        both_matches.insert(1u32, 4usize);
        let ctx_both = SelectionCtx {
            prefix_owner: Some(0),
            prefix_matches: Some(&both_matches),
            block_size: 16,
            ..SelectionCtx::none()
        };
        assert_eq!(
            PrefixAffinity
                .select(&ctx_both, &candidates_aff)
                .map(|s| (s.index, s.fallback_reason)),
            Some((1, None))
        );
    }

    #[test]
    fn test_bounded_rendezvous_packs_prefix_onto_k_nodes_and_reconverges() {
        let eps: Vec<EndpointInfo> = (0..6).map(|r| make_ep("m", r)).collect();
        let idle_stats: Vec<EndpointStats> = (0..6)
            .map(|r| make_stats("m", r, 0, Some(0.2), None))
            .collect();
        let idle_candidates: Vec<Candidate> = (0..6)
            .map(|i| Candidate {
                endpoint: &eps[i],
                stats: Some(&idle_stats[i]),
            })
            .collect();

        let seed_a = seed_from_bytes(&[b"shared-prefix-alpha"]);
        let seed_b = seed_from_bytes(&[b"shared-prefix-beta"]);

        // Cold burst for seed_a always picks its deterministic primary Rendezvous node.
        let primary_a = PrefixAffinity
            .select(
                &SelectionCtx {
                    prefix_key_seed: Some(seed_a),
                    ..SelectionCtx::none()
                },
                &idle_candidates,
            )
            .unwrap();
        assert_eq!(primary_a.fallback_reason, Some("no_signal"));

        // Even when all 6 replicas hold 32 blocks after an earlier spike, once queues
        // drain back to 0, PrefixAffinity reconverges onto the same primary Rendezvous owner.
        let all_six: std::collections::HashMap<u32, usize> =
            (0..6u32).map(|r| (r, 32usize)).collect();
        let reconverged = PrefixAffinity
            .select(
                &SelectionCtx {
                    prefix_owner: Some(5), // even if replica 5 emitted the latest Stored event
                    prefix_matches: Some(&all_six),
                    block_size: 16,
                    prefix_key_seed: Some(seed_a),
                    ..SelectionCtx::none()
                },
                &idle_candidates,
            )
            .unwrap();
        assert_eq!(reconverged.index, primary_a.index);
        assert_eq!(reconverged.fallback_reason, None);

        // When the top-2 Rendezvous owners for seed_b exceed `min_pending + slack` (0 + 4),
        // bounded Rendezvous spills to an uncongested replica rather than piling up.
        let mut busy_stats = idle_stats.clone();
        let mut ranked_b: Vec<(usize, u64)> = (0..6)
            .map(|i| (i, rendezvous_score(seed_b, i as u32)))
            .collect();
        ranked_b.sort_by(|a, b| b.1.cmp(&a.1));
        let top0 = ranked_b[0].0;
        let top1 = ranked_b[1].0;
        busy_stats[top0].pending_requests = 10;
        busy_stats[top1].pending_requests = 10;
        let busy_candidates: Vec<Candidate> = (0..6)
            .map(|i| Candidate {
                endpoint: &eps[i],
                stats: Some(&busy_stats[i]),
            })
            .collect();
        let spilled = select_bounded_rendezvous(&busy_candidates, seed_b, 2, 4).unwrap();
        assert!(spilled != top0 && spilled != top1);
    }
}
