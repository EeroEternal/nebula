//! Tool-Pause continuation leases and L1 KV spine pinning for `Router`.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use nebula_common::ReplicaRole;

use crate::{now_ms, session_key, Router, TOOL_PAUSE_MAX_ENTRIES, TOOL_PAUSE_PIN_MAX_ENTRIES};

/// `lmcache_controller` request timeout for a pin. Pins are best-effort background work.
const TOOL_PAUSE_PIN_TIMEOUT_MS: u64 = 5_000;

/// Chunk-aligned pin prefix length.
///
/// LMCache keys, pins and reports KV in `chunk_size` (256) token objects, so a shorter
/// prefix has nothing to address: `pin` of a 200-token list pins zero chunks. Measured on
/// the rig: a 1493-token list pins 1280 (`docs/dev/kv-governance.md` §4.3).
pub(crate) fn pin_prefix_len(available: usize, cap: usize) -> usize {
    let n = available.min(cap);
    n / nebula_common::LMCACHE_CHUNK_TOKENS * nebula_common::LMCACHE_CHUNK_TOKENS
}

/// `lmcache_controller` base URL for `POST /pin`, or `None` to disable Tool-Pause pinning.
///
/// Resolution order keeps one knob per deployment shape: an explicit
/// `NEBULA_LMCACHE_CONTROLLER_BASE_URL` (the same variable the BFF uses), else the pd_split
/// `/lookup` URL the router already has with its path stripped. `NEBULA_TOOL_PAUSE_PIN=0`
/// turns it off without unsetting either.
pub(crate) fn tool_pause_pin_base_from_env() -> Option<String> {
    let disabled = std::env::var("NEBULA_TOOL_PAUSE_PIN")
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
        .unwrap_or(false);
    if disabled {
        return None;
    }
    let normalize = |raw: &str| {
        let base = raw.trim().trim_end_matches('/');
        let base = base.strip_suffix("/lookup").unwrap_or(base);
        let base = base.trim_end_matches('/');
        (!base.is_empty()).then(|| base.to_string())
    };
    std::env::var("NEBULA_LMCACHE_CONTROLLER_BASE_URL")
        .ok()
        .and_then(|v| normalize(&v))
        .or_else(|| {
            std::env::var("NEBULA_PD_SPLIT_LOOKUP_URL")
                .ok()
                .and_then(|v| normalize(&v))
        })
}

impl Router {
    /// Record a single-use Coding Agent `Tool-Pause` session lease
    /// (`finish_reason == "tool_calls"`, Batch 5.3) scoped to `(tenant_id, session_id)`
    /// so when the same tenant's session returns within the lease window
    /// (`NEBULA_TOOL_PAUSE_TTL_MS`, default 60s), it gets one priority queue
    /// admission and strong replica anchoring before the lease is consumed.
    pub fn mark_tool_pause_for_tenant(
        self: &Arc<Self>,
        tenant_id: Option<&str>,
        session_id: &str,
        model_uid: &str,
        replica_id: u32,
    ) {
        self.mark_tool_pause_for_tenant_at(tenant_id, session_id, model_uid, replica_id, now_ms());
        self.spawn_tool_pause_pin(tenant_id, session_id, model_uid, replica_id);
    }

    /// Remember the first `NEBULA_TOOL_PAUSE_PIN_TOKENS` prompt ids for this session.
    ///
    /// Called from [`Self::body_route_hints`] — the request path is the only place the
    /// prompt is tokenized and the body still exists; by the time `tool_calls` is observed
    /// the body is gone, and `/pin` takes token ids, not hashes.
    ///
    /// Chunk-aligned and bounded: a prefix shorter than one 256-token chunk has no
    /// addressable KV object, and the map is capped because it holds raw ids.
    pub fn remember_session_prefix(
        &self,
        tenant_id: Option<&str>,
        session_id: Option<&str>,
        ids: &[u32],
    ) {
        let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
            return;
        };
        if self.tool_pause_pin_base.is_none() || self.tool_pause_pin_tokens_cap == 0 {
            return;
        }
        let n = pin_prefix_len(ids.len(), self.tool_pause_pin_tokens_cap);
        if n < nebula_common::LMCACHE_CHUNK_TOKENS {
            return;
        }
        let now = now_ms();
        let key = session_key(tenant_id, sid);
        if self.session_prefix_ids.len() >= TOOL_PAUSE_PIN_MAX_ENTRIES
            && !self.session_prefix_ids.contains_key(&key)
        {
            self.session_prefix_ids.retain(|_, (_, exp)| *exp > now);
            while self.session_prefix_ids.len() >= TOOL_PAUSE_PIN_MAX_ENTRIES {
                let oldest = self
                    .session_prefix_ids
                    .iter()
                    .min_by_key(|e| e.value().1)
                    .map(|e| e.key().clone());
                let Some(oldest) = oldest else { break };
                self.session_prefix_ids.remove(&oldest);
            }
        }
        self.session_prefix_ids.insert(
            key,
            (
                ids[..n].to_vec(),
                now.saturating_add(self.tool_pause_ttl_ms),
            ),
        );
    }

    /// Claim this session's remembered spine prefix for a pin (consumes the entry).
    pub(crate) fn take_pin_tokens(
        &self,
        tenant_id: Option<&str>,
        session_id: &str,
        model_uid: &str,
        replica_id: u32,
    ) -> Option<(String, Vec<u32>)> {
        let key = session_key(tenant_id, session_id.trim());
        let (_, (ids, expires_at_ms)) = self.session_prefix_ids.remove(&key)?;
        // A resumed turn after a long idle must not pin a prefix captured hours ago.
        if expires_at_ms <= now_ms() {
            return None;
        }
        let n = pin_prefix_len(ids.len(), self.tool_pause_pin_tokens_cap);
        if n < nebula_common::LMCACHE_CHUNK_TOKENS {
            return None;
        }
        // `/pin` is addressed per replica: the instance id is the string the engine
        // registered with, never the cluster-wide `NEBULA_PD_SPLIT_INSTANCE_ID` fallback.
        Some((
            nebula_common::lmcache_instance_id(model_uid, replica_id),
            ids[..n].to_vec(),
        ))
    }

    /// Pin the resumed session's spine prefix in L1 on the replica that answered.
    ///
    /// Triggered only by the *server-observed* `tool_calls` signal (never a client hint),
    /// and only meaningful for LMCache-backed deployments. LMCache has no unpin endpoint:
    /// `POST /pin` is a lease released by `pin_timeout_sec` (or engine restart), which is
    /// exactly the pause window. There is deliberately **no** renewal/unpin path —
    /// re-pinning after a pause cannot recover KV that already expired during it, and the
    /// lease would leak L1 otherwise (measured: a pin survives L1 LRU pressure until the
    /// lease expires, an unpinned prefix is evicted; `docs/dev/kv-governance.md` §4.3).
    ///
    /// Spawned off the response path: a controller round-trip must never add latency, and
    /// every outcome (ok / failed / skipped) is counted so silent degradation is visible.
    fn spawn_tool_pause_pin(
        self: &Arc<Self>,
        tenant_id: Option<&str>,
        session_id: &str,
        model_uid: &str,
        replica_id: u32,
    ) {
        let Some(base) = self.tool_pause_pin_base.clone() else {
            self.tool_pause_pin_skipped_total
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let Some((instance, tokens)) =
            self.take_pin_tokens(tenant_id, session_id, model_uid, replica_id)
        else {
            // No tokenized prefix for this session (tokenizer dir missing, or < 1 chunk).
            self.tool_pause_pin_skipped_total
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            // Sync caller (CLI/tests): nothing to spawn on. Counted, not swallowed.
            self.tool_pause_pin_skipped_total
                .fetch_add(1, Ordering::Relaxed);
            return;
        };
        let router = Arc::clone(self);
        let url = format!("{base}/pin");
        handle.spawn(async move {
            let body = serde_json::json!({
                "instance_id": instance,
                "location": nebula_common::LMCACHE_DEFAULT_KV_LOCATION,
                "tokens": tokens,
            });
            let response = router
                .http_client
                .post(&url)
                .json(&body)
                .timeout(std::time::Duration::from_millis(TOOL_PAUSE_PIN_TIMEOUT_MS))
                .send()
                .await;
            match response {
                Ok(resp) if resp.status().is_success() => {
                    let pinned = resp
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|v| v.get("num_tokens").and_then(|n| n.as_u64()))
                        .unwrap_or(0);
                    router
                        .tool_pause_pin_ok_total
                        .fetch_add(1, Ordering::Relaxed);
                    router
                        .tool_pause_pinned_tokens_total
                        .fetch_add(pinned, Ordering::Relaxed);
                }
                Ok(resp) => {
                    let status = resp.status();
                    let detail = resp.text().await.unwrap_or_default();
                    router
                        .tool_pause_pin_failed_total
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        status = %status,
                        detail = %detail.chars().take(200).collect::<String>(),
                        "tool-pause KV pin rejected by lmcache_controller"
                    );
                }
                Err(e) => {
                    router
                        .tool_pause_pin_failed_total
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(error = %e, "tool-pause KV pin failed");
                }
            }
        });
    }

    /// `(ok, failed, skipped, pinned_tokens)` for Tool-Pause spine pinning.
    pub fn tool_pause_pin_stats(&self) -> (u64, u64, u64, u64) {
        (
            self.tool_pause_pin_ok_total.load(Ordering::Relaxed),
            self.tool_pause_pin_failed_total.load(Ordering::Relaxed),
            self.tool_pause_pin_skipped_total.load(Ordering::Relaxed),
            self.tool_pause_pinned_tokens_total.load(Ordering::Relaxed),
        )
    }

    pub fn mark_tool_pause_for_tenant_at(
        &self,
        tenant_id: Option<&str>,
        session_id: &str,
        model_uid: &str,
        replica_id: u32,
        now: u64,
    ) {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return;
        }
        // Never anchor a session lease on a Prefill-only replica in pd_split.
        if let Some(ep) = self.endpoints.get(&(model_uid.to_string(), replica_id)) {
            if ep.role == ReplicaRole::Prefill {
                return;
            }
        }
        let key = session_key(tenant_id, session_id);
        if self.tool_pause_leases.len() >= TOOL_PAUSE_MAX_ENTRIES
            && !self.tool_pause_leases.contains_key(&key)
        {
            self.tool_pause_leases
                .retain(|_, (_, _, exp, _)| *exp > now);
            while self.tool_pause_leases.len() >= TOOL_PAUSE_MAX_ENTRIES {
                let oldest_key = self
                    .tool_pause_leases
                    .iter()
                    .min_by_key(|e| e.value().2)
                    .map(|e| e.key().clone());
                let Some(oldest_key) = oldest_key else {
                    break;
                };
                self.tool_pause_leases.remove(&oldest_key);
            }
        }
        let expires_at = now.saturating_add(self.tool_pause_ttl_ms);
        self.tool_pause_leases.insert(
            key.clone(),
            (model_uid.to_string(), replica_id, expires_at, false),
        );
        self.insert_session_affinity(key, model_uid.to_string(), replica_id);
        self.tool_pause_marked_total.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn has_active_tool_pause_for_tenant(
        &self,
        tenant_id: Option<&str>,
        session_id: &str,
    ) -> bool {
        self.has_active_tool_pause_for_tenant_at(tenant_id, session_id, now_ms())
    }

    #[cfg(test)]
    pub fn has_active_tool_pause_for_tenant_at(
        &self,
        tenant_id: Option<&str>,
        session_id: &str,
        now: u64,
    ) -> bool {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return false;
        }
        let key = session_key(tenant_id, session_id);
        self.tool_pause_leases
            .remove_if(&key, |_, (_, _, exp, _)| *exp <= now);
        self.tool_pause_leases
            .get(&key)
            .map(|entry| entry.value().2 > now)
            .unwrap_or(false)
    }

    /// Atomically claim the single-use queue-admission continuation privilege
    /// for `(tenant_id, session_id)`. Returns `true` at most once per
    /// server-recorded `Tool-Pause` lease (`finish_reason == "tool_calls"`).
    /// Client-supplied request bodies (`role == "tool"`) are never trusted on
    /// their own to prevent privilege spoofing and cross-tenant session hijacking.
    #[cfg(test)]
    pub fn is_tool_continuation(&self, session_id: Option<&str>) -> bool {
        self.is_tool_continuation_for_tenant(None, session_id)
    }

    pub fn is_tool_continuation_for_tenant(
        &self,
        tenant_id: Option<&str>,
        session_id: Option<&str>,
    ) -> bool {
        let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
            return false;
        };
        let now = now_ms();
        let key = session_key(tenant_id, sid);
        self.tool_pause_leases
            .remove_if(&key, |_, (_, _, exp, _)| *exp <= now);
        if let Some(mut entry) = self.tool_pause_leases.get_mut(&key) {
            if entry.value().2 > now && !entry.value().3 {
                entry.value_mut().3 = true;
                return true;
            }
        }
        false
    }
}
