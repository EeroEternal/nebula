//! `pd_split` warm-signal lookup and role-pool helpers for `Router`.

use std::sync::atomic::Ordering;

use nebula_common::{max_matched_tokens_for_model, ReplicaRole};

use crate::{now_ms, Router};

/// pd_split controller lookups are cached briefly (prefix warmth changes on
/// the order of evictions, not requests) and time out fast so routing never
/// stalls on the controller.
pub(crate) const PD_WARM_CACHE_MS: u64 = 2_000;
/// Lookups after which an all-cold streak is worth a warning (and a counter).
pub(crate) const PD_COLD_STREAK_ALARM: u64 = 50;
pub(crate) const PD_WARM_CACHE_MAX_ENTRIES: usize = 4_096;
pub(crate) const PD_LOOKUP_TIMEOUT_MS: u64 = 200;

/// Extract matched tokens for `model_uid` from an LMCache controller `/lookup`
/// response, falling back to `fallback_instance_id` (such as
/// `"lmcache_default_instance"`) only when the response contains that exact
/// single-instance key and no `{model_uid}-*` instance matched.
///
/// shortcut: the fallback instance carries no model tag, so a controller run
/// with one shared default instance cannot isolate models; only `{model_uid}-*`
/// instances (`lmcache_instance_id`) are per-model. Upgrade when LMCache's
/// `/lookup` reports which model an instance holds.
pub(crate) fn lookup_matched_tokens_for_router(
    v: &serde_json::Value,
    model_uid: &str,
    fallback_instance_id: Option<&str>,
) -> u64 {
    let by_model = max_matched_tokens_for_model(v, model_uid);
    if by_model > 0 {
        return by_model;
    }
    if let Some(inst) = fallback_instance_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if inst != model_uid {
            return max_matched_tokens_for_model(v, inst);
        }
    }
    0
}

pub(crate) fn warm_cache_key(model_uid: &str, token_ids: &[u32]) -> (String, [u8; 32]) {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for &id in token_ids {
        hasher.update(id.to_le_bytes());
    }
    let digest: [u8; 32] = hasher.finalize().into();
    (model_uid.to_string(), digest)
}

impl Router {
    /// Ask the LMCache controller whether the shared store holds this
    /// prompt's prefix ("warm" signal for pd_split stage routing).
    ///
    /// `NEBULA_PD_SPLIT_LOOKUP_URL` points at `lmcache_controller`'s
    /// `/lookup` (e.g. http://127.0.0.1:9000/lookup). Short prompts below
    /// `NEBULA_PD_BYPASS_TOKENS` (default 256, aligned with LMCache's 256-token
    /// chunk size) short-circuit immediately without an HTTP round-trip.
    /// Results are cached per (model, sha256(token_ids)) for a short TTL and
    /// bounded by `PD_WARM_CACHE_MAX_ENTRIES`; any error degrades to "cold" so
    /// routing never blocks on the controller.
    pub async fn controller_warm(&self, model_uid: &str, token_ids: &[u32]) -> bool {
        self.controller_warm_tokens(model_uid, token_ids).await > 0
    }

    /// Ask the LMCache controller how many prefix tokens of `token_ids` are
    /// cached for `model_uid`'s instances (`{model_uid}-*`).
    pub async fn controller_warm_tokens(&self, model_uid: &str, token_ids: &[u32]) -> u64 {
        let bypass_tokens = self.pd_bypass_tokens;
        if token_ids.is_empty() || (bypass_tokens > 0 && token_ids.len() < bypass_tokens) {
            return 0;
        }
        let Ok(url) = std::env::var("NEBULA_PD_SPLIT_LOOKUP_URL") else {
            return 0;
        };
        // TokenSpeed `scheduler.md` §1 (`prefix_replay_tokens >= 1`): at least the
        // final prompt token must always be recomputed to produce logits for the
        // first output token, so a prompt of length L can save at most L - 1 tokens.
        let max_replayable = token_ids.len().saturating_sub(1) as u64;
        let cache_key = warm_cache_key(model_uid, token_ids);
        {
            let now = now_ms();
            self.pd_warm_cache.remove_if(&cache_key, |_, (_, ts)| {
                now.saturating_sub(*ts) >= PD_WARM_CACHE_MS
            });
            if let Some(entry) = self.pd_warm_cache.get(&cache_key) {
                let (cached_matched, ts) = *entry.value();
                if now.saturating_sub(ts) < PD_WARM_CACHE_MS {
                    return cached_matched.min(max_replayable);
                }
            }
        }
        let fallback_inst = std::env::var("NEBULA_PD_SPLIT_INSTANCE_ID").ok();
        let instance_id = fallback_inst
            .clone()
            .unwrap_or_else(|| "lmcache_default_instance".to_string());
        let body = serde_json::json!({
            "tokens": token_ids,
            "instance_id": instance_id,
        });
        let matched = match self
            .http_client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_millis(PD_LOOKUP_TIMEOUT_MS))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(v) => {
                        lookup_matched_tokens_for_router(&v, model_uid, Some(instance_id.as_str()))
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "pd_split lookup response unparseable; treating as cold");
                        0
                    }
                }
            }
            Ok(resp) => {
                tracing::warn!(status = %resp.status(), "pd_split lookup rejected; treating as cold");
                0
            }
            Err(e) => {
                tracing::warn!(error = %e, "pd_split lookup unreachable; treating as cold");
                0
            }
        };
        let matched = matched.min(max_replayable);
        let warm = matched > 0;
        self.insert_warm_cache_at(cache_key, matched, now_ms());
        let lookups = self.pd_lookup_total.fetch_add(1, Ordering::Relaxed) + 1;
        if warm {
            self.pd_lookup_warm_total.fetch_add(1, Ordering::Relaxed);
        } else if lookups >= PD_COLD_STREAK_ALARM
            && self.pd_lookup_warm_total.load(Ordering::Relaxed) == 0
        {
            // A warm signal that has never fired means pd_split is silently degenerating
            // into a single-Prefill deployment. The usual cause is bring-up order: an
            // engine that started before lmcache_controller registers but never reports
            // its KV keys, so every lookup answers cold for that engine's lifetime
            // (docs/dev/benefit-verification.md §"暖信号启动顺序").
            //
            // Note the `warm_total == 0` guard: ordinary cold misses in a mixed stream
            // must not be counted here, or the counter would read "signal dead" while it
            // is working (measured: 176 warm hits alongside 123 false positives).
            if !self.pd_cold_streak_warned.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    model_uid = %model_uid,
                    lookups,
                    "every LMCache controller lookup has been cold; pd_split is running as a single-Prefill pool (check that lmcache_controller was ready before the engines started)"
                );
            }
            self.note_stage_stats_missing("warm_signal_cold");
        }
        matched
    }

    pub fn record_pd_warm_prefix_tokens(
        &self,
        model_uid: &str,
        first_block_hash: [u8; 32],
        matched_tokens: u64,
    ) {
        if matched_tokens == 0 {
            return;
        }
        let now = now_ms();
        let key = (model_uid.to_string(), first_block_hash);
        if self.pd_warm_tokens_by_prefix.len() >= PD_WARM_CACHE_MAX_ENTRIES
            && !self.pd_warm_tokens_by_prefix.contains_key(&key)
        {
            self.pd_warm_tokens_by_prefix
                .retain(|_, (_, ts)| now.saturating_sub(*ts) < PD_WARM_CACHE_MS);
            while self.pd_warm_tokens_by_prefix.len() >= PD_WARM_CACHE_MAX_ENTRIES {
                let Some(oldest_key) = self
                    .pd_warm_tokens_by_prefix
                    .iter()
                    .min_by_key(|e| e.value().1)
                    .map(|e| e.key().clone())
                else {
                    break;
                };
                self.pd_warm_tokens_by_prefix.remove(&oldest_key);
            }
        }
        self.pd_warm_tokens_by_prefix
            .insert(key, (matched_tokens, now));
    }

    pub(crate) fn insert_warm_cache_at(
        &self,
        cache_key: (String, [u8; 32]),
        matched_tokens: u64,
        now: u64,
    ) {
        if self.pd_warm_cache.len() >= PD_WARM_CACHE_MAX_ENTRIES
            && !self.pd_warm_cache.contains_key(&cache_key)
        {
            // 1. Sweep all TTL-expired entries first.
            self.pd_warm_cache
                .retain(|_, (_, ts)| now.saturating_sub(*ts) < PD_WARM_CACHE_MS);
            // 2. If still at capacity (e.g. burst of >4096 distinct prompts within 2s),
            // evict the oldest entry in O(n) via min_by_key so capacity never exceeds
            // PD_WARM_CACHE_MAX_ENTRIES.
            while self.pd_warm_cache.len() >= PD_WARM_CACHE_MAX_ENTRIES {
                let Some(oldest_key) = self
                    .pd_warm_cache
                    .iter()
                    .min_by_key(|e| e.value().1)
                    .map(|e| e.key().clone())
                else {
                    break;
                };
                self.pd_warm_cache.remove(&oldest_key);
            }
        }
        self.pd_warm_cache.insert(cache_key, (matched_tokens, now));
    }

    /// pd_split warm-signal observability (lookups, warm results).
    pub fn pd_lookup_stats(&self) -> (u64, u64) {
        (
            self.pd_lookup_total.load(Ordering::Relaxed),
            self.pd_lookup_warm_total.load(Ordering::Relaxed),
        )
    }

    /// True when any endpoint of this model declares a pd_split role.
    /// Cheap enough for the request path (a handful of replicas per model),
    /// and only consulted when the strategy is not prefix_affinity.
    pub fn model_has_role_pools(&self, model_uid: &str) -> bool {
        self.endpoints
            .iter()
            .any(|e| e.key().0 == model_uid && e.value().role != ReplicaRole::Mixed)
    }
}
