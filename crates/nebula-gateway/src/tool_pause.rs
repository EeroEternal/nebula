//! Local Tool-Pause continuation tracker for `NEBULA_ROUTER_MODE=remote`.
//!
//! In `embedded` mode, `nebula-router` owns the `(tenant_id, session_id)` Tool-Pause
//! lease table and Gateway queries it directly in `acquire_queue_permit`.
//! In `remote` mode (`st.router == None`), Gateway still inspects every upstream
//! response/SSE stream, so tracking observed `tool_calls` completions locally keeps
//! bounded-queue continuation priority consistent across both Router modes without
//! adding an extra hop or RPC round-trip.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_TOOL_PAUSE_TTL_MS: u64 = 60_000;
const TOOL_PAUSE_MAX_ENTRIES: usize = 4_096;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn tool_pause_ttl_ms_from_env() -> u64 {
    std::env::var("NEBULA_TOOL_PAUSE_TTL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_TOOL_PAUSE_TTL_MS)
}

fn session_key(tenant_id: Option<&str>, session_id: &str) -> String {
    match tenant_id.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => format!("{t}\0{session_id}"),
        None => format!("\0{session_id}"),
    }
}

#[derive(Debug, Clone)]
pub struct ToolPauseTracker {
    /// Keyed by `(tenant_id, session_id)` -> `(expires_at_ms, consumed)`.
    leases: Arc<Mutex<HashMap<String, (u64, bool)>>>,
    ttl_ms: u64,
}

impl Default for ToolPauseTracker {
    fn default() -> Self {
        Self::new(tool_pause_ttl_ms_from_env())
    }
}

impl ToolPauseTracker {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            leases: Arc::new(Mutex::new(HashMap::new())),
            ttl_ms: ttl_ms.max(1),
        }
    }

    pub fn mark(&self, tenant_id: Option<&str>, session_id: &str) {
        self.mark_at(tenant_id, session_id, now_ms());
    }

    pub fn mark_at(&self, tenant_id: Option<&str>, session_id: &str, now: u64) {
        let sid = session_id.trim();
        if sid.is_empty() {
            return;
        }
        let key = session_key(tenant_id, sid);
        let mut guard = self.leases.lock().unwrap_or_else(|e| e.into_inner());
        if guard.len() >= TOOL_PAUSE_MAX_ENTRIES && !guard.contains_key(&key) {
            guard.retain(|_, (exp, _)| *exp > now);
            while guard.len() >= TOOL_PAUSE_MAX_ENTRIES {
                let oldest = guard
                    .iter()
                    .min_by_key(|(_, (exp, _))| *exp)
                    .map(|(k, _)| k.clone());
                match oldest {
                    Some(k) => {
                        guard.remove(&k);
                    }
                    None => break,
                }
            }
        }
        guard.insert(key, (now.saturating_add(self.ttl_ms), false));
    }

    /// Atomically claim the single-use queue-admission continuation privilege
    /// for `(tenant_id, session_id)`.
    pub fn claim_continuation(&self, tenant_id: Option<&str>, session_id: Option<&str>) -> bool {
        self.claim_continuation_at(tenant_id, session_id, now_ms())
    }

    pub fn claim_continuation_at(
        &self,
        tenant_id: Option<&str>,
        session_id: Option<&str>,
        now: u64,
    ) -> bool {
        let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
            return false;
        };
        let key = session_key(tenant_id, sid);
        let mut guard = self.leases.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((exp, used)) = guard.get_mut(&key) {
            if *exp <= now {
                guard.remove(&key);
                return false;
            }
            if !*used {
                *used = true;
                return true;
            }
        }
        false
    }
}

/// Record a `tool_calls` completion across either embedded Router or remote tracker.
pub fn record_tool_pause(
    router: Option<&Arc<nebula_router::Router>>,
    tracker: &ToolPauseTracker,
    tenant_id: Option<&str>,
    session_id: Option<&str>,
    routed_endpoint: Option<&(String, u32)>,
) {
    let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    if let (Some(r), Some((m, rep))) = (router, routed_endpoint) {
        r.mark_tool_pause_for_tenant(tenant_id, sid, m, *rep);
    } else if router.is_none() {
        tracker.mark(tenant_id, sid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_is_single_use_tenant_isolated_and_expires() {
        let tracker = ToolPauseTracker::new(1000);
        assert!(!tracker.claim_continuation_at(Some("t1"), Some("s1"), 100));

        tracker.mark_at(Some("t1"), "s1", 100);
        // Cross-tenant or tenantless requests cannot hijack the lease.
        assert!(!tracker.claim_continuation_at(Some("t2"), Some("s1"), 200));
        assert!(!tracker.claim_continuation_at(None, Some("s1"), 200));

        // First claim succeeds; second claim is rejected (single-use).
        assert!(tracker.claim_continuation_at(Some("t1"), Some("s1"), 200));
        assert!(!tracker.claim_continuation_at(Some("t1"), Some("s1"), 201));

        // Expired lease is rejected.
        tracker.mark_at(Some("t1"), "s2", 500);
        assert!(!tracker.claim_continuation_at(Some("t1"), Some("s2"), 1500));
    }
}
