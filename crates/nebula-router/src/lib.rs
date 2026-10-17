use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use nebula_common::{
    EndpointInfo, EndpointStats, EndpointStatus, ExecutionContext, InferenceHint, ModelSpec,
    ReplicaRole,
};

pub mod kv_events;
pub mod kv_hash;
pub mod prompt_hash;
pub mod strategy;
pub mod sync;

use strategy::{Candidate, LeastPending, RoutingStrategy, SelectionCtx};

/// KV cache usage threshold (fraction 0.0–1.0). When ALL endpoints exceed this,
/// admission control kicks in and returns Overloaded.
const KV_CACHE_OVERLOAD_THRESHOLD: f64 = 0.95;

/// Extract the largest matched-prefix length from an LMCache controller
/// `/lookup` response. Shapes seen in the wild (both mean "n tokens cached"):
///   {"event_id": "...", "lmcache_default_instance": ["LocalCPUBackend", 512]}
///   {"event_id": "...", "layout_info": {"<instance>": ["LocalCPUBackend", 569]}}
fn max_matched_tokens(v: &serde_json::Value) -> Option<u64> {
    fn scan(m: &serde_json::Map<String, serde_json::Value>) -> Option<u64> {
        m.iter()
            .filter(|(k, _)| k.as_str() != "event_id")
            .filter_map(|(_, val)| match val {
                // instance -> ["LocalCPUBackend", n]
                serde_json::Value::Array(a) => a.get(1).and_then(|n| n.as_u64()),
                // layout_info -> {instance -> ["LocalCPUBackend", n]}
                serde_json::Value::Object(inner) => scan(inner),
                _ => None,
            })
            .max()
    }
    v.as_object().and_then(scan)
}

/// pd_split controller lookups are cached briefly (prefix warmth changes on
/// the order of evictions, not requests) and time out fast so routing never
/// stalls on the controller.
const PD_WARM_CACHE_MS: u64 = 2_000;
const PD_WARM_CACHE_MAX_ENTRIES: usize = 4_096;
const PD_LOOKUP_TIMEOUT_MS: u64 = 200;

/// Default prompt-token threshold below which `pd_split` skips controller
/// `/lookup` and routes directly to an unsaturated Decode replica (Batch 5.1,
/// aligned with LMCache's 256-token chunk boundary).
pub const DEFAULT_PD_BYPASS_TOKENS: usize = 256;

/// Default `NEBULA_PD_PREFILL_SPILLOVER_PENDING` (0 = disabled by default so
/// `pd_split` preserves strict Prefill/Decode stage isolation unless opted in).
pub const DEFAULT_PD_PREFILL_SPILLOVER_PENDING: u64 = 0;

/// Default TTL (ms) and capacity bound for Coding Agent `Tool-Pause` session
/// leases and session affinity (Batch 5.3).
pub const DEFAULT_TOOL_PAUSE_TTL_MS: u64 = 60_000;
pub const TOOL_PAUSE_MAX_ENTRIES: usize = 4_096;
pub const SESSION_AFFINITY_MAX_ENTRIES: usize = 4_096;

/// Tenant-scoped session key `(tenant_id, session_id)` so one tenant cannot
/// collide with, hijack, or borrow another tenant's continuation lease or
/// replica affinity.
type SessionKey = (Option<String>, String);

fn session_key(tenant_id: Option<&str>, session_id: &str) -> SessionKey {
    let tenant = tenant_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    (tenant, session_id.trim().to_string())
}

fn warm_cache_key(model_uid: &str, token_ids: &[u32]) -> (String, [u8; 32]) {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for &id in token_ids {
        hasher.update(id.to_le_bytes());
    }
    let digest: [u8; 32] = hasher.finalize().into();
    (model_uid.to_string(), digest)
}

/// Inspect an upstream response payload or SSE chunk to detect whether the
/// model paused for a tool call (`finish_reason == "tool_calls"` or non-empty
/// `tool_calls` array).
pub fn response_has_tool_calls(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    if !text.contains("tool_calls") {
        return false;
    };
    if text.contains("\"finish_reason\":\"tool_calls\"")
        || text.contains("\"finish_reason\": \"tool_calls\"")
    {
        return true;
    }
    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) {
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            return choices.iter().any(|c| {
                c.get("finish_reason").and_then(|r| r.as_str()) == Some("tool_calls")
                    || c.pointer("/message/tool_calls")
                        .and_then(|tc| tc.as_array())
                        .map(|a| !a.is_empty())
                        .unwrap_or(false)
                    || c.pointer("/delta/tool_calls")
                        .and_then(|tc| tc.as_array())
                        .map(|a| !a.is_empty())
                        .unwrap_or(false)
            });
        }
    }
    false
}

#[derive(Debug, Clone, Default)]
struct EndpointCircuitState {
    consecutive_failures: u32,
    open_until_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

/// Extract the prompt text for prefix hashing: `/v1/completions`-style
/// `prompt` (string, or first element of a string array).
pub fn extract_prompt_string(body: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    match v.get("prompt")? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(a) => a.first().and_then(|x| x.as_str()).map(str::to_string),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub enum RouteError {
    /// No ready endpoint found for the requested model.
    NoEndpoint,
    /// All endpoints are overloaded (kv_cache_usage > threshold).
    Overloaded,
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::NoEndpoint => write!(f, "no ready endpoint"),
            RouteError::Overloaded => write!(f, "all endpoints overloaded"),
        }
    }
}

pub struct Router {
    endpoints: DashMap<(String, u32), EndpointInfo>,
    stats: DashMap<(String, u32), EndpointStats>,
    session_affinity: DashMap<SessionKey, (String, u32)>,
    /// Batch 5.3: bounded single-use Tool-Pause session leases
    /// `(tenant_id, session_id) -> (model_uid, replica_id, expires_at_ms, queue_admitted)`.
    tool_pause_leases: DashMap<SessionKey, (String, u32, u64, bool)>,
    tool_pause_ttl_ms: u64,
    tool_pause_marked_total: AtomicU64,
    tool_continuation_hit_total: AtomicU64,
    strategy: Box<dyn RoutingStrategy>,
    /// model_name → model_uid (e.g. "Qwen/Qwen2.5-0.5B-Instruct" → "qwen2_5_0_5b")
    model_names: DashMap<String, String>,
    /// model_uid → model_name (reverse mapping)
    model_uids_to_names: DashMap<String, String>,
    /// model_uid → OpenAI model id sent to the engine (`--served-model-name` when set).
    engine_model_ids: DashMap<String, String>,
    /// Registered alias strings per model_uid (for cleanup on spec update/delete).
    model_alias_keys: DashMap<String, Vec<String>>,
    /// model_uid → latest PlacementPlan.version
    plan_versions: DashMap<String, u64>,
    route_stale_stats_dropped_total: AtomicU64,
    route_circuit_skipped_total: AtomicU64,
    circuit_open_total: AtomicU64,
    hint_received_total: AtomicU64,
    hint_adopted_total: AtomicU64,
    hint_conflict_rejected_total: AtomicU64,
    hint_expired_total: AtomicU64,
    hint_stale_degraded_total: AtomicU64,
    session_affinity_hit_total: AtomicU64,
    prefix_hint_hit_total: AtomicU64,
    /// Strategy fell back to least_pending: (strategy, reason) → count.
    /// Counted so silent degradation is observable (the engine_metrics bug
    /// hid exactly this way). Reasons: no_signal / below_threshold /
    /// owner_unavailable / owner_congested.
    strategy_fallback: DashMap<(&'static str, &'static str), AtomicU64>,
    /// P1c: prefix block-hash → replica index, fed by engine KV events.
    /// Rebuildable cache — dropping it only loses routing hints.
    prefix_index: Arc<kv_events::PrefixIndex>,
    prefix_hasher: prompt_hash::PromptHasher,
    /// P1c affinity observability: model → (lookups, hits).
    prefix_lookup: DashMap<String, (AtomicU64, AtomicU64)>,
    /// pd_split stage-pool fallbacks (empty role pool → all candidates).
    stage_prefill_fallback_total: AtomicU64,
    stage_decode_fallback_total: AtomicU64,
    /// pd_split single-mapper bottleneck protection: cold requests spilled over
    /// to idle/lighter Decode (kv_both) replicas when the Prefill pool is busy.
    stage_prefill_spillover_total: AtomicU64,
    /// pd_split Tier-1 locality: warm requests routed to a Decode replica that
    /// already holds the prefix in local GPU VRAM (skipping TCP KV shuffle).
    pd_decode_local_hit_total: AtomicU64,
    /// Minimum pending_requests across Prefill replicas before cold requests
    /// can spill over to idle Decode replicas (0 disables spillover).
    pd_prefill_spillover_pending: AtomicU64,
    /// Batch 5.1: token threshold below which `pd_split` skips controller `/lookup`
    /// and routes cold short requests directly to unsaturated Decode replicas.
    pd_bypass_tokens: usize,
    pd_short_bypass_total: AtomicU64,
    /// pd_split controller warm-lookup cache: (model, sha256(token_ids)) → (warm, ts).
    pd_warm_cache: DashMap<(String, [u8; 32]), (bool, u64)>,
    pd_lookup_total: AtomicU64,
    pd_lookup_warm_total: AtomicU64,
    http_client: reqwest::Client,
    stats_max_age_ms: u64,
    circuit_failure_threshold: u32,
    circuit_open_ms: u64,
    endpoint_circuit: DashMap<(String, u32), EndpointCircuitState>,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("strategy", &self.strategy.name())
            .finish()
    }
}

impl Router {
    pub fn new() -> Arc<Self> {
        Self::with_strategy(Box::new(LeastPending))
    }

    pub fn with_strategy(strategy: Box<dyn RoutingStrategy>) -> Arc<Self> {
        tracing::info!(strategy = strategy.name(), "router initialized");
        let stats_max_age_ms = std::env::var("NEBULA_ROUTE_STATS_MAX_AGE_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| {
                std::env::var("NEBULA_XTRACE_METRIC_MAX_AGE_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or(60_000);
        let circuit_failure_threshold = std::env::var("NEBULA_ROUTE_CIRCUIT_FAILURE_THRESHOLD")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(3);
        let circuit_open_ms = std::env::var("NEBULA_ROUTE_CIRCUIT_OPEN_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30_000);
        let pd_prefill_spillover_pending = std::env::var("NEBULA_PD_PREFILL_SPILLOVER_PENDING")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_PD_PREFILL_SPILLOVER_PENDING);
        let pd_bypass_tokens = std::env::var("NEBULA_PD_BYPASS_TOKENS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_PD_BYPASS_TOKENS);
        let tool_pause_ttl_ms = std::env::var("NEBULA_TOOL_PAUSE_TTL_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TOOL_PAUSE_TTL_MS);
        Arc::new(Self {
            endpoints: DashMap::new(),
            stats: DashMap::new(),
            session_affinity: DashMap::new(),
            tool_pause_leases: DashMap::new(),
            tool_pause_ttl_ms,
            tool_pause_marked_total: AtomicU64::new(0),
            tool_continuation_hit_total: AtomicU64::new(0),
            strategy,
            model_names: DashMap::new(),
            model_uids_to_names: DashMap::new(),
            engine_model_ids: DashMap::new(),
            model_alias_keys: DashMap::new(),
            plan_versions: DashMap::new(),
            route_stale_stats_dropped_total: AtomicU64::new(0),
            route_circuit_skipped_total: AtomicU64::new(0),
            circuit_open_total: AtomicU64::new(0),
            hint_received_total: AtomicU64::new(0),
            hint_adopted_total: AtomicU64::new(0),
            hint_conflict_rejected_total: AtomicU64::new(0),
            hint_expired_total: AtomicU64::new(0),
            hint_stale_degraded_total: AtomicU64::new(0),
            session_affinity_hit_total: AtomicU64::new(0),
            prefix_hint_hit_total: AtomicU64::new(0),
            strategy_fallback: DashMap::new(),
            prefix_index: kv_events::PrefixIndex::new(),
            prefix_hasher: prompt_hash::PromptHasher::from_env(),
            prefix_lookup: DashMap::new(),
            stage_prefill_fallback_total: AtomicU64::new(0),
            stage_decode_fallback_total: AtomicU64::new(0),
            stage_prefill_spillover_total: AtomicU64::new(0),
            pd_decode_local_hit_total: AtomicU64::new(0),
            pd_prefill_spillover_pending: AtomicU64::new(pd_prefill_spillover_pending),
            pd_bypass_tokens,
            pd_short_bypass_total: AtomicU64::new(0),
            pd_warm_cache: DashMap::new(),
            pd_lookup_total: AtomicU64::new(0),
            pd_lookup_warm_total: AtomicU64::new(0),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(PD_LOOKUP_TIMEOUT_MS))
                .build()
                .unwrap_or_default(),
            stats_max_age_ms,
            circuit_failure_threshold,
            circuit_open_ms,
            endpoint_circuit: DashMap::new(),
        })
    }

    pub fn strategy_name(&self) -> &'static str {
        self.strategy.name()
    }

    pub fn replace_all_endpoints(&self, infos: Vec<EndpointInfo>) {
        self.endpoints.clear();
        for info in infos {
            self.endpoints
                .insert((info.model_uid.clone(), info.replica_id), info);
        }
        self.session_affinity.clear();
        self.tool_pause_leases.clear();
    }

    pub fn upsert_endpoint(&self, info: EndpointInfo) {
        let key = (info.model_uid.clone(), info.replica_id);
        if let Some(old) = self.endpoints.get(&key) {
            if old.base_url != info.base_url {
                self.endpoint_circuit.remove(&key);
            }
        }
        self.endpoints.insert(key, info);
    }

    pub fn remove_endpoint(&self, model_uid: &str, replica_id: u32) {
        self.endpoints.remove(&(model_uid.to_string(), replica_id));
    }

    pub fn upsert_stats(&self, stats: EndpointStats) {
        self.stats
            .insert((stats.model_uid.clone(), stats.replica_id), stats);
    }

    pub fn remove_stats(&self, model_uid: &str, replica_id: u32) {
        self.stats.remove(&(model_uid.to_string(), replica_id));
    }

    pub fn replace_all_stats(&self, items: Vec<EndpointStats>) {
        self.stats.clear();
        for stats in items {
            self.upsert_stats(stats);
        }
    }

    pub fn route_stale_stats_dropped_total(&self) -> u64 {
        self.route_stale_stats_dropped_total.load(Ordering::Relaxed)
    }

    pub fn route_circuit_skipped_total(&self) -> u64 {
        self.route_circuit_skipped_total.load(Ordering::Relaxed)
    }

    pub fn circuit_open_total(&self) -> u64 {
        self.circuit_open_total.load(Ordering::Relaxed)
    }

    pub fn hint_received_total(&self) -> u64 {
        self.hint_received_total.load(Ordering::Relaxed)
    }

    pub fn hint_adopted_total(&self) -> u64 {
        self.hint_adopted_total.load(Ordering::Relaxed)
    }

    pub fn hint_conflict_rejected_total(&self) -> u64 {
        self.hint_conflict_rejected_total.load(Ordering::Relaxed)
    }

    pub fn hint_expired_total(&self) -> u64 {
        self.hint_expired_total.load(Ordering::Relaxed)
    }

    pub fn hint_stale_degraded_total(&self) -> u64 {
        self.hint_stale_degraded_total.load(Ordering::Relaxed)
    }

    pub fn session_affinity_hit_total(&self) -> u64 {
        self.session_affinity_hit_total.load(Ordering::Relaxed)
    }

    pub fn prefix_hint_hit_total(&self) -> u64 {
        self.prefix_hint_hit_total.load(Ordering::Relaxed)
    }

    pub fn strategy_fallback_total(&self, strategy: &'static str, reason: &'static str) -> u64 {
        self.strategy_fallback
            .get(&(strategy, reason))
            .map(|c| c.value().load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    pub fn prefix_index(&self) -> &Arc<kv_events::PrefixIndex> {
        &self.prefix_index
    }

    /// Live prefix-index entry count (routing hints learned from KV events).
    pub fn prefix_index_entries(&self) -> usize {
        self.prefix_index.entries()
    }

    /// Stale snapshot/delta mutations dropped by prefix-index watermark protection.
    pub fn prefix_index_stale_dropped_total(&self) -> u64 {
        self.prefix_index.stale_dropped_total()
    }

    /// Affinity lookup/hit counters per model, for /metrics and BFF.
    pub fn prefix_lookup_stats(&self) -> Vec<(String, u64, u64)> {
        self.prefix_lookup
            .iter()
            .map(|e| {
                let (l, h) = e.value();
                (
                    e.key().clone(),
                    l.load(Ordering::Relaxed),
                    h.load(Ordering::Relaxed),
                )
            })
            .collect()
    }

    pub fn stage_fallback_total(&self, stage: &'static str) -> u64 {
        match stage {
            "decode" => self.stage_decode_fallback_total.load(Ordering::Relaxed),
            _ => self.stage_prefill_fallback_total.load(Ordering::Relaxed),
        }
    }

    pub fn stage_prefill_spillover_total(&self) -> u64 {
        self.stage_prefill_spillover_total.load(Ordering::Relaxed)
    }

    pub fn pd_decode_local_hit_total(&self) -> u64 {
        self.pd_decode_local_hit_total.load(Ordering::Relaxed)
    }

    pub fn pd_short_bypass_total(&self) -> u64 {
        self.pd_short_bypass_total.load(Ordering::Relaxed)
    }

    pub fn tool_pause_marked_total(&self) -> u64 {
        self.tool_pause_marked_total.load(Ordering::Relaxed)
    }

    pub fn tool_continuation_hit_total(&self) -> u64 {
        self.tool_continuation_hit_total.load(Ordering::Relaxed)
    }

    pub fn set_pd_prefill_spillover_pending(&self, threshold: u64) {
        self.pd_prefill_spillover_pending
            .store(threshold, Ordering::Relaxed);
    }

    fn insert_session_affinity(&self, key: SessionKey, model_uid: String, replica_id: u32) {
        if self.session_affinity.len() >= SESSION_AFFINITY_MAX_ENTRIES
            && !self.session_affinity.contains_key(&key)
        {
            // Bind `evict_key` in a standalone `let` statement so the temporary
            // `DashMap::iter()` guard is dropped BEFORE `remove(&evict_key)`
            // acquires a write lock on the same shard (Rust 2021 temporary
            // lifetime rule inside `if let` scrutinees).
            let evict_key = self.session_affinity.iter().next().map(|e| e.key().clone());
            if let Some(evict_key) = evict_key {
                self.session_affinity.remove(&evict_key);
            }
        }
        self.session_affinity.insert(key, (model_uid, replica_id));
    }

    /// Record a single-use Coding Agent `Tool-Pause` session lease
    /// (`finish_reason == "tool_calls"`, Batch 5.3) scoped to `(tenant_id, session_id)`
    /// so when the same tenant's session returns within the lease window
    /// (`NEBULA_TOOL_PAUSE_TTL_MS`, default 60s), it gets one priority queue
    /// admission and strong replica anchoring before the lease is consumed.
    pub fn mark_tool_pause(&self, session_id: &str, model_uid: &str, replica_id: u32) {
        self.mark_tool_pause_for_tenant_at(None, session_id, model_uid, replica_id, now_ms());
    }

    pub fn mark_tool_pause_for_tenant(
        &self,
        tenant_id: Option<&str>,
        session_id: &str,
        model_uid: &str,
        replica_id: u32,
    ) {
        self.mark_tool_pause_for_tenant_at(tenant_id, session_id, model_uid, replica_id, now_ms());
    }

    pub fn mark_tool_pause_at(&self, session_id: &str, model_uid: &str, replica_id: u32, now: u64) {
        self.mark_tool_pause_for_tenant_at(None, session_id, model_uid, replica_id, now);
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

    /// True when `(None, session_id)` currently holds a non-expired Tool-Pause lease.
    pub fn has_active_tool_pause(&self, session_id: &str) -> bool {
        self.has_active_tool_pause_for_tenant_at(None, session_id, now_ms())
    }

    pub fn has_active_tool_pause_for_tenant(
        &self,
        tenant_id: Option<&str>,
        session_id: &str,
    ) -> bool {
        self.has_active_tool_pause_for_tenant_at(tenant_id, session_id, now_ms())
    }

    pub fn has_active_tool_pause_at(&self, session_id: &str, now: u64) -> bool {
        self.has_active_tool_pause_for_tenant_at(None, session_id, now)
    }

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
        let bypass_tokens = self.pd_bypass_tokens;
        if token_ids.is_empty() || (bypass_tokens > 0 && token_ids.len() < bypass_tokens) {
            return false;
        }
        let Ok(url) = std::env::var("NEBULA_PD_SPLIT_LOOKUP_URL") else {
            return false;
        };
        let cache_key = warm_cache_key(model_uid, token_ids);
        {
            let now = now_ms();
            self.pd_warm_cache.remove_if(&cache_key, |_, (_, ts)| {
                now.saturating_sub(*ts) >= PD_WARM_CACHE_MS
            });
            if let Some(entry) = self.pd_warm_cache.get(&cache_key) {
                let (warm, ts) = *entry.value();
                if now.saturating_sub(ts) < PD_WARM_CACHE_MS {
                    return warm;
                }
            }
        }
        let body = serde_json::json!({
            "tokens": token_ids,
            "instance_id": std::env::var("NEBULA_PD_SPLIT_INSTANCE_ID")
                .unwrap_or_else(|_| "lmcache_default_instance".to_string()),
        });
        let warm = match self
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
                        let matched = max_matched_tokens(&v);
                        matched.map(|n| n > 0).unwrap_or(false)
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "pd_split lookup response unparseable; treating as cold");
                        false
                    }
                }
            }
            Ok(resp) => {
                tracing::warn!(status = %resp.status(), "pd_split lookup rejected; treating as cold");
                false
            }
            Err(e) => {
                tracing::warn!(error = %e, "pd_split lookup unreachable; treating as cold");
                false
            }
        };
        self.insert_warm_cache_at(cache_key, warm, now_ms());
        self.pd_lookup_total.fetch_add(1, Ordering::Relaxed);
        if warm {
            self.pd_lookup_warm_total.fetch_add(1, Ordering::Relaxed);
        }
        warm
    }

    fn insert_warm_cache_at(&self, cache_key: (String, [u8; 32]), warm: bool, now: u64) {
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
        self.pd_warm_cache.insert(cache_key, (warm, now));
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

    /// Live index entries per model (metrics scrapes only, O(n)).
    pub fn prefix_index_entries_by_model(&self) -> std::collections::HashMap<String, usize> {
        self.prefix_index.entries_by_model()
    }

    /// Tokenize `prompt` (via the model's tokenizer, when configured) and
    /// chain its vLLM-compatible block hashes.
    pub fn prompt_hashes(&self, model_uid: &str, prompt: &str) -> Option<Vec<[u8; 32]>> {
        let name = self
            .model_uids_to_names
            .get(model_uid)
            .map(|e| e.value().clone());
        self.prefix_hasher
            .prompt_hashes(model_uid, name.as_deref(), prompt)
    }

    /// Route hints for a request body: prefix hashes (prefix_affinity index /
    /// pd_split warm decisions) plus the pd_split controller warm signal.
    /// Tokenizes the body at most once so chat template rendering and BPE
    /// encoding are never duplicated on the request path.
    pub async fn body_route_hints(
        &self,
        model_uid: &str,
        body: &[u8],
    ) -> (Option<Vec<[u8; 32]>>, bool) {
        let has_roles = self.model_has_role_pools(model_uid);
        if self.strategy_name() != "prefix_affinity" && !has_roles {
            return (None, false);
        }
        let Some(ids) = self.body_token_ids(model_uid, body) else {
            return (None, false);
        };
        let hashes = Some(self.prefix_hasher.hashes_from_ids(&ids));
        let warm = if has_roles {
            self.controller_warm(model_uid, &ids).await
        } else {
            false
        };
        (hashes, warm)
    }

    /// Extract token IDs for an OpenAI-style or SGLang `/v1/score` body
    /// (`/v1/completions` `prompt`, `/v1/chat/completions` `messages` rendered
    /// with the model's chat template, or `/v1/score` `query` / `text` shared
    /// context when accompanied by `items` or `label_token_ids`). `/v1/rerank`
    /// bodies (`{"query", "documents"}`) are intentionally excluded.
    pub fn body_token_ids(&self, model_uid: &str, body: &[u8]) -> Option<Vec<u32>> {
        let v: serde_json::Value = serde_json::from_slice(body).ok()?;
        let name = self
            .model_uids_to_names
            .get(model_uid)
            .map(|e| e.value().clone());
        let is_score_payload = (v.get("items").is_some() || v.get("label_token_ids").is_some())
            && v.get("documents").is_none();
        match v.get("prompt") {
            Some(serde_json::Value::String(s)) => {
                self.prefix_hasher.prompt_ids(model_uid, name.as_deref(), s)
            }
            Some(serde_json::Value::Array(a)) => a
                .first()
                .and_then(|x| x.as_str())
                .and_then(|s| self.prefix_hasher.prompt_ids(model_uid, name.as_deref(), s)),
            _ if v.get("messages").is_some() => {
                self.prefix_hasher.chat_ids(model_uid, name.as_deref(), &v)
            }
            _ if is_score_payload && v.get("query").is_some() => match v.get("query")? {
                serde_json::Value::String(s) => {
                    self.prefix_hasher.prompt_ids(model_uid, name.as_deref(), s)
                }
                serde_json::Value::Array(a) => {
                    let ids: Vec<u32> = a
                        .iter()
                        .filter_map(|x| x.as_u64().map(|n| n as u32))
                        .collect();
                    if ids.is_empty() {
                        None
                    } else {
                        Some(ids)
                    }
                }
                _ => None,
            },
            _ if is_score_payload && v.get("text").and_then(|t| t.as_str()).is_some() => {
                let s = v.get("text")?.as_str()?;
                self.prefix_hasher.prompt_ids(model_uid, name.as_deref(), s)
            }
            _ => None,
        }
    }

    pub fn body_prompt_hashes(&self, model_uid: &str, body: &[u8]) -> Option<Vec<[u8; 32]>> {
        if self.strategy_name() != "prefix_affinity" && !self.model_has_role_pools(model_uid) {
            return None;
        }
        let ids = self.body_token_ids(model_uid, body)?;
        Some(self.prefix_hasher.hashes_from_ids(&ids))
    }

    pub fn record_endpoint_success(&self, model_uid: &str, replica_id: u32) {
        self.endpoint_circuit
            .remove(&(model_uid.to_string(), replica_id));
    }

    pub fn record_endpoint_failure(&self, model_uid: &str, replica_id: u32) {
        let key = (model_uid.to_string(), replica_id);
        let now = now_ms();
        let mut entry = self.endpoint_circuit.entry(key).or_default();

        if entry.open_until_ms > now {
            return;
        }

        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        if entry.consecutive_failures >= self.circuit_failure_threshold {
            entry.consecutive_failures = 0;
            entry.open_until_ms = now.saturating_add(self.circuit_open_ms);
            self.circuit_open_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn is_endpoint_circuit_open(&self, model_uid: &str, replica_id: u32) -> bool {
        let now = now_ms();
        if let Some(entry) = self
            .endpoint_circuit
            .get(&(model_uid.to_string(), replica_id))
        {
            let open = entry.open_until_ms > now;
            if !open {
                drop(entry);
                self.endpoint_circuit
                    .remove(&(model_uid.to_string(), replica_id));
            }
            return open;
        }
        false
    }

    pub fn clear_session_affinity(&self, session_id: &str) {
        self.clear_session_affinity_for_tenant(None, session_id);
    }

    pub fn clear_session_affinity_for_tenant(&self, tenant_id: Option<&str>, session_id: &str) {
        let key = session_key(tenant_id, session_id);
        self.session_affinity.remove(&key);
        self.tool_pause_leases.remove(&key);
    }

    /// Register a bidirectional model_uid ↔ model_name mapping.
    pub fn set_model_mapping(&self, model_uid: &str, model_name: &str) {
        self.model_names
            .insert(model_name.to_string(), model_uid.to_string());
        self.model_uids_to_names
            .insert(model_uid.to_string(), model_name.to_string());
        if !self.engine_model_ids.contains_key(model_uid) {
            self.engine_model_ids
                .insert(model_uid.to_string(), model_name.to_string());
        }
    }

    fn register_model_alias(&self, model_uid: &str, alias: &str) {
        if alias.is_empty() || alias == model_uid {
            return;
        }
        self.model_names
            .insert(alias.to_string(), model_uid.to_string());
        self.model_alias_keys
            .entry(model_uid.to_string())
            .or_default()
            .push(alias.to_string());
    }

    /// Sync aliases from `/models/{uid}/spec` (model_name, served_model_name, model_path).
    pub fn register_model_spec(&self, spec: &ModelSpec) {
        self.clear_model_mappings(&spec.model_uid);
        self.set_model_mapping(&spec.model_uid, &spec.model_name);

        let engine_id = spec
            .config
            .as_ref()
            .and_then(|c| c.served_model_name.as_deref())
            .unwrap_or(spec.model_name.as_str());
        self.engine_model_ids
            .insert(spec.model_uid.clone(), engine_id.to_string());
        if engine_id != spec.model_name {
            self.register_model_alias(&spec.model_uid, engine_id);
        }

        if let Some(path) = spec.model_path.as_deref() {
            self.register_model_alias(&spec.model_uid, path);
            if let Some(base) = path.rsplit('/').next() {
                if !base.is_empty() && base != path {
                    self.register_model_alias(&spec.model_uid, base);
                }
            }
        }
    }

    /// Remove all name aliases registered for a model (spec deleted).
    pub fn clear_model_mappings(&self, model_uid: &str) {
        if let Some((_, keys)) = self.model_alias_keys.remove(model_uid) {
            for key in keys {
                // Compare via a cloned value so the read guard is dropped
                // before remove — get+remove on the same key deadlocks the
                // shard (this exact bug froze the router on spec update).
                let owned = self.model_names.get(&key).map(|e| e.value().clone());
                if owned.as_deref() == Some(model_uid) {
                    self.model_names.remove(&key);
                }
            }
        }
        self.model_uids_to_names.remove(model_uid);
        self.engine_model_ids.remove(model_uid);
    }

    /// OpenAI `model` field to send to the engine after routing.
    pub fn get_engine_model_name(&self, model_uid: &str) -> Option<String> {
        self.engine_model_ids
            .get(model_uid)
            .map(|v| v.clone())
            .or_else(|| self.get_model_name(model_uid))
    }

    /// Resolve an input model string to a model_uid.
    /// If `input` is already a known model_uid, return it as-is.
    /// Otherwise, check if it's a known model_name and return the mapped model_uid.
    /// Falls back to returning the input unchanged.
    pub fn resolve_model(&self, input: &str) -> String {
        // Already a known model_uid?
        if self.model_uids_to_names.contains_key(input) {
            return input.to_string();
        }
        // Known model_name → model_uid?
        if let Some(uid) = self.model_names.get(input) {
            return uid.value().clone();
        }
        // Fallback: return as-is
        input.to_string()
    }

    /// Get the user-facing model_name for a given model_uid.
    pub fn get_model_name(&self, model_uid: &str) -> Option<String> {
        self.model_uids_to_names
            .get(model_uid)
            .map(|v| v.value().clone())
    }

    /// Record the latest placement plan version for a model.
    pub fn set_plan_version(&self, model_uid: &str, version: u64) {
        self.plan_versions.insert(model_uid.to_string(), version);
    }

    /// Clear plan version when placement is deleted.
    pub fn clear_plan_version(&self, model_uid: &str) {
        self.plan_versions.remove(model_uid);
    }

    /// Latest known plan version for a model (None if unknown / deleted).
    pub fn plan_version_for(&self, model_uid: &str) -> Option<u64> {
        self.plan_versions.get(model_uid).map(|v| *v)
    }

    /// Replace all known plan versions from a full placement snapshot.
    pub fn replace_all_plan_versions(&self, versions: impl IntoIterator<Item = (String, u64)>) {
        self.plan_versions.clear();
        for (uid, ver) in versions {
            self.plan_versions.insert(uid, ver);
        }
    }

    /// Collect all stats as a slice snapshot (for admission control, etc.).
    pub fn all_stats_for_model(&self, model_uid: &str) -> Vec<EndpointStats> {
        self.stats
            .iter()
            .filter(|e| e.key().0 == model_uid)
            .map(|e| e.value().clone())
            .collect()
    }

    fn get_fresh_stats(&self, model_uid: &str, replica_id: u32) -> Option<EndpointStats> {
        let stats = self
            .stats
            .get(&(model_uid.to_string(), replica_id))
            .map(|s| s.value().clone())?;

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
        let age_ms = now_ms.saturating_sub(stats.last_updated_ms);
        if age_ms > self.stats_max_age_ms {
            self.route_stale_stats_dropped_total
                .fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                model_uid=%model_uid,
                replica_id,
                age_ms,
                max_age_ms=self.stats_max_age_ms,
                "dropping stale routing stats"
            );
            return None;
        }

        Some(stats)
    }

    fn route_internal(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        plan_version: Option<u64>,
        exclude: Option<(&str, u32)>,
        prefix_hashes: Option<&[[u8; 32]]>,
        pd_warm: bool,
    ) -> Result<EndpointInfo, RouteError> {
        if ctx.inference_hint.is_some() {
            self.hint_received_total.fetch_add(1, Ordering::Relaxed);
        }
        let now = now_ms();
        let active_hint = if let Some(h) = ctx.inference_hint.clone() {
            if h.expires_at_ms.map(|ts| ts <= now).unwrap_or(false) {
                self.hint_expired_total.fetch_add(1, Ordering::Relaxed);
                None
            } else if h.prefer_session_affinity == Some(true) && ctx.session_id.is_none() {
                self.hint_conflict_rejected_total
                    .fetch_add(1, Ordering::Relaxed);
                None
            } else {
                Some(h)
            }
        } else {
            None
        };

        // Explicit replica pin (opt-in via x-nebula-replica-id).
        if let Some(replica_id) = ctx.pinned_replica_id {
            if active_hint.is_some() {
                self.hint_conflict_rejected_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            if let Some(ep) = self
                .endpoints
                .get(&(model_uid.to_string(), replica_id))
                .map(|v| v.value().clone())
            {
                let excluded = exclude
                    .map(|(m, r)| m == model_uid && r == replica_id)
                    .unwrap_or(false);
                let plan_ok = plan_version.map(|v| ep.plan_version == v).unwrap_or(true);
                if !excluded
                    && ep.status == EndpointStatus::Ready
                    && plan_ok
                    && !self.is_endpoint_circuit_open(&ep.model_uid, ep.replica_id)
                {
                    return Ok(ep);
                }
            }
            return Err(RouteError::NoEndpoint);
        }

        // Session affinity check (never lock onto a Prefill role replica in pd_split:
        // a cold first turn routes to Prefill to populate shared KV, while subsequent
        // warm turns must evaluate stage routing to land on a Decode replica).
        if let Some(session_id) = ctx.session_id.as_deref() {
            let key = session_key(ctx.tenant_id.as_deref(), session_id);
            let _ = self
                .tool_pause_leases
                .remove_if(&key, |_, (_, _, exp, _)| *exp <= now);
            let tool_pause_consumed = self
                .tool_pause_leases
                .remove_if(&key, |_, (_, _, exp, _)| *exp > now)
                .is_some();
            if let Some(aff) = self.session_affinity.get(&key) {
                let (aff_model_uid, aff_replica_id) = aff.value();
                if aff_model_uid == model_uid {
                    let excluded = exclude
                        .map(|(m, r)| m == aff_model_uid.as_str() && r == *aff_replica_id)
                        .unwrap_or(false);
                    if excluded {
                        // Retry path explicitly excludes this sticky endpoint.
                    } else if let Some(ep) = self
                        .endpoints
                        .get(&(aff_model_uid.clone(), *aff_replica_id))
                        .map(|v| v.value().clone())
                    {
                        let plan_ok = plan_version.map(|v| ep.plan_version == v).unwrap_or(true);
                        if ep.status == EndpointStatus::Ready
                            && plan_ok
                            && !self.is_endpoint_circuit_open(&ep.model_uid, ep.replica_id)
                            && ep.role != ReplicaRole::Prefill
                        {
                            if tool_pause_consumed {
                                self.tool_continuation_hit_total
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            self.session_affinity_hit_total
                                .fetch_add(1, Ordering::Relaxed);
                            return Ok(ep);
                        }
                    }
                }
            }
        }

        // Build candidate list: filter by model_uid, Ready, optional plan_version and optional exclude
        let filtered: Vec<EndpointInfo> = self
            .endpoints
            .iter()
            .filter(|e| {
                let ep = e.value();
                if ep.model_uid != model_uid {
                    tracing::debug!(model_uid=%ep.model_uid, "endpoint filtered: uid mismatch");
                    return false;
                }
                if ep.status != EndpointStatus::Ready {
                    tracing::debug!(model_uid=%ep.model_uid, replica_id=%ep.replica_id, status=?ep.status, "endpoint filtered: not ready");
                    return false;
                }
                if self.is_endpoint_circuit_open(&ep.model_uid, ep.replica_id) {
                    tracing::debug!(model_uid=%ep.model_uid, replica_id=%ep.replica_id, "endpoint filtered: circuit open");
                    self.route_circuit_skipped_total
                        .fetch_add(1, Ordering::Relaxed);
                    return false;
                }
                if let Some(required_plan_version) = plan_version {
                    if ep.plan_version != required_plan_version {
                        tracing::debug!(model_uid=%ep.model_uid, replica_id=%ep.replica_id, plan_version=%ep.plan_version, required=%required_plan_version, "endpoint filtered: plan version mismatch");
                        return false;
                    }
                }
                if let Some((exclude_model_uid, exclude_replica_id)) = exclude {
                    if ep.model_uid == exclude_model_uid && ep.replica_id == exclude_replica_id {
                        tracing::debug!(model_uid=%ep.model_uid, replica_id=%ep.replica_id, "endpoint filtered: excluded");
                        return false;
                    }
                }
                tracing::debug!(model_uid=%ep.model_uid, replica_id=%ep.replica_id, "endpoint accepted as candidate");
                true
            })
            .map(|e| e.value().clone())
            .collect();

        if filtered.is_empty() {
            tracing::warn!(model_uid=%model_uid, total_endpoints=self.endpoints.len(), "no endpoints passed filters");
            return Err(RouteError::NoEndpoint);
        }

        let stats_snapshot: Vec<Option<EndpointStats>> = filtered
            .iter()
            .map(|ep| self.get_fresh_stats(&ep.model_uid, ep.replica_id))
            .collect();

        let mut candidates_data: Vec<(EndpointInfo, Option<EndpointStats>)> =
            filtered.into_iter().zip(stats_snapshot).collect();

        // Stats-missing degradation: when any fresh stats exist, deprioritize stale/missing stats
        // by dropping missing-stats candidates from this routing decision.
        if candidates_data.iter().any(|(_, s)| s.is_some()) {
            let before_len = candidates_data.len();
            let with_stats: Vec<(EndpointInfo, Option<EndpointStats>)> = candidates_data
                .iter()
                .filter(|(_, s)| s.is_some())
                .map(|(ep, s)| (ep.clone(), s.clone()))
                .collect();
            if !with_stats.is_empty() {
                if active_hint.is_some() && with_stats.len() < before_len {
                    self.hint_stale_degraded_total
                        .fetch_add(1, Ordering::Relaxed);
                }
                candidates_data = with_stats;
            }
        }

        // Candidate reduction: filter out confirmed overloaded endpoints first.
        let has_kv_data = candidates_data.iter().any(|(_, s)| {
            s.as_ref()
                .map(|st| st.kv_cache_usage.is_some())
                .unwrap_or(false)
        });

        if has_kv_data {
            let non_overloaded: Vec<(EndpointInfo, Option<EndpointStats>)> = candidates_data
                .into_iter()
                .filter(|(_, s)| {
                    if let Some(st) = s {
                        if let Some(usage) = st.kv_cache_usage {
                            return usage < KV_CACHE_OVERLOAD_THRESHOLD;
                        }
                    }
                    true
                })
                .collect();

            if non_overloaded.is_empty() {
                return Err(RouteError::Overloaded);
            }
            candidates_data = non_overloaded;
        }

        let any_prefix_owner = prefix_hashes.and_then(|hs| {
            if hs.is_empty() {
                return None;
            }
            let entry = self
                .prefix_lookup
                .entry(model_uid.to_string())
                .or_insert_with(|| (AtomicU64::new(0), AtomicU64::new(0)));
            let (l, h) = entry.value();
            l.fetch_add(1, Ordering::Relaxed);
            let owner = self.prefix_index.lookup(model_uid, hs);
            if owner.is_some() {
                h.fetch_add(1, Ordering::Relaxed);
            }
            owner
        });

        let is_pd_split = candidates_data
            .iter()
            .any(|(ep, _)| ep.role != ReplicaRole::Mixed);
        let mut pd_warm_stage = false;

        // pd_split stage routing (pool.md §pd_split v1.1 + Batch 5.1 short-prompt bypass):
        // when any candidate declares a role, cold traffic goes to the prefill pool
        // (which writes KV into the shared store) and warm / session-continued traffic
        // to the decode pool (which retrieves or hits local VRAM). Short cold prompts
        // (< NEBULA_PD_BYPASS_TOKENS, default 256) bypass directly to an unsaturated
        // Decode pool for in-place Chunked Prefill + Decode. An empty pool falls back
        // to all candidates behind a visible counter. Replicated deployments never
        // enter this branch (all roles Mixed).
        if is_pd_split {
            let affinity_on_decode = ctx
                .session_id
                .as_deref()
                .and_then(|sid| {
                    let key = session_key(ctx.tenant_id.as_deref(), sid);
                    self.session_affinity.get(&key).map(|e| e.value().clone())
                })
                .map(|(m, r)| {
                    candidates_data.iter().any(|(ep, _)| {
                        ep.model_uid == m && ep.replica_id == r && ep.role == ReplicaRole::Decode
                    })
                })
                .unwrap_or(false);
            let warm = pd_warm || any_prefix_owner.is_some() || affinity_on_decode;
            pd_warm_stage = warm;
            let want = if warm {
                ReplicaRole::Decode
            } else {
                ReplicaRole::Prefill
            };
            let pool: Vec<(EndpointInfo, Option<EndpointStats>)> = candidates_data
                .iter()
                .filter(|(ep, _)| ep.role == want)
                .cloned()
                .collect();
            let bypass_tokens = self.pd_bypass_tokens;
            let is_short_prompt = bypass_tokens > 0
                && prefix_hashes
                    .map(|hs| hs.len() * 16 < bypass_tokens)
                    .unwrap_or(false);
            let spillover_threshold = self.pd_prefill_spillover_pending.load(Ordering::Relaxed);
            if pool.is_empty() {
                let counter = if want == ReplicaRole::Decode {
                    &self.stage_decode_fallback_total
                } else {
                    &self.stage_prefill_fallback_total
                };
                counter.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(
                    model_uid = %model_uid,
                    stage = ?want,
                    "pd_split role pool empty; falling back to all candidates"
                );
            } else if want == ReplicaRole::Prefill && is_short_prompt {
                // Only bypass to Decode replicas with fresh stats (`s.is_some()`):
                // never treat missing stats as `pending_requests = 0`.
                let decode_with_stats: Vec<(EndpointInfo, Option<EndpointStats>)> = candidates_data
                    .iter()
                    .filter(|(ep, s)| ep.role == ReplicaRole::Decode && s.is_some())
                    .cloned()
                    .collect();
                let min_prefill_pending = pool
                    .iter()
                    .filter_map(|(_, s)| s.as_ref().map(|st| st.pending_requests))
                    .min()
                    .unwrap_or(0);
                let min_decode_pending = decode_with_stats
                    .iter()
                    .filter_map(|(_, s)| s.as_ref().map(|st| st.pending_requests))
                    .min();
                if let Some(min_decode_pending) = min_decode_pending {
                    if min_decode_pending == 0 || min_decode_pending <= min_prefill_pending {
                        self.pd_short_bypass_total.fetch_add(1, Ordering::Relaxed);
                        tracing::debug!(
                            model_uid = %model_uid,
                            bypass_tokens,
                            min_decode_pending,
                            "pd_split short prompt bypassing prefill pool to unsaturated decode pool"
                        );
                        candidates_data = decode_with_stats;
                    } else {
                        candidates_data = pool;
                    }
                } else {
                    candidates_data = pool;
                }
            } else if want == ReplicaRole::Prefill && spillover_threshold > 0 {
                // Opt-in single-mapper bottleneck protection: when every Prefill replica
                // has fresh stats with pending_requests >= spillover_threshold and at least
                // one Decode (kv_both) replica has fresh stats with strictly lower pending
                // load, spill cold traffic over to those lighter Decode replicas.
                let min_prefill_pending = pool
                    .iter()
                    .filter_map(|(_, s)| s.as_ref().map(|st| st.pending_requests))
                    .min()
                    .unwrap_or(0);
                if min_prefill_pending >= spillover_threshold {
                    let spillover_decode: Vec<(EndpointInfo, Option<EndpointStats>)> =
                        candidates_data
                            .iter()
                            .filter(|(ep, s)| {
                                ep.role == ReplicaRole::Decode
                                    && s.as_ref()
                                        .is_some_and(|st| st.pending_requests < min_prefill_pending)
                            })
                            .cloned()
                            .collect();
                    if !spillover_decode.is_empty() {
                        self.stage_prefill_spillover_total
                            .fetch_add(1, Ordering::Relaxed);
                        tracing::debug!(
                            model_uid = %model_uid,
                            min_prefill_pending,
                            spillover_candidates = spillover_decode.len(),
                            "pd_split prefill pool saturated; spilling cold request to idle decode pool"
                        );
                        candidates_data = spillover_decode;
                    } else {
                        candidates_data = pool;
                    }
                } else {
                    candidates_data = pool;
                }
            } else {
                candidates_data = pool;
            }
        }

        // Two-tier locality (v1.1 multi-owner index + Batch 5.2 joint cost):
        // resolve prefix_owner and per-candidate matched block depth scoped to
        // the post-stage-filter candidate pool first so a Decode replica holding
        // the prefix in local GPU VRAM is preferred over remote TCP KV shuffle.
        let allowed_ids: Vec<u32> = candidates_data
            .iter()
            .map(|(ep, _)| ep.replica_id)
            .collect();
        let scoped_prefix_owner = prefix_hashes.and_then(|hs| {
            if hs.is_empty() {
                None
            } else {
                self.prefix_index
                    .lookup_in(model_uid, hs, Some(&allowed_ids))
            }
        });
        let scoped_prefix_matches = prefix_hashes.and_then(|hs| {
            if hs.is_empty() {
                None
            } else {
                Some(
                    self.prefix_index
                        .lookup_matches_in(model_uid, hs, Some(&allowed_ids)),
                )
            }
        });
        let prefix_owner = scoped_prefix_owner.or(any_prefix_owner);

        let candidates: Vec<Candidate> = candidates_data
            .iter()
            .map(|(ep, s)| Candidate {
                endpoint: ep,
                stats: s.as_ref(),
            })
            .collect();

        let sel_ctx = SelectionCtx {
            prefix_owner,
            prefix_matches: scoped_prefix_matches.as_ref(),
        };

        let selected = if let Some(hint) = active_hint.as_ref() {
            if let Some(idx) = Self::select_hint_biased_endpoint(&candidates_data, hint) {
                self.hint_adopted_total.fetch_add(1, Ordering::Relaxed);
                if hint.prefer_prefix_key.is_some() {
                    self.prefix_hint_hit_total.fetch_add(1, Ordering::Relaxed);
                }
                candidates_data[idx].0.clone()
            } else {
                self.select_by_strategy(&sel_ctx, &candidates, &candidates_data)?
            }
        } else if is_pd_split && pd_warm_stage {
            // Tier-1 vs Tier-2 locality inside the Decode pool: if a Decode
            // replica in the pool already owns the prefix in local GPU VRAM
            // (`scoped_prefix_owner`) and its pending load is no higher than the
            // least-pending candidate in the pool, route to that local owner to
            // avoid TCP KV shuffle even when the global strategy is LeastPending.
            let min_pool_pending = candidates_data
                .iter()
                .map(|(_, s)| s.as_ref().map(|st| st.pending_requests).unwrap_or(0))
                .min()
                .unwrap_or(0);
            if let Some(local_idx) = scoped_prefix_owner.and_then(|owner| {
                candidates_data.iter().position(|(ep, s)| {
                    ep.replica_id == owner
                        && s.as_ref().map(|st| st.pending_requests).unwrap_or(0) <= min_pool_pending
                })
            }) {
                candidates_data[local_idx].0.clone()
            } else {
                self.select_by_strategy(&sel_ctx, &candidates, &candidates_data)?
            }
        } else {
            self.select_by_strategy(&sel_ctx, &candidates, &candidates_data)?
        };

        if is_pd_split
            && pd_warm_stage
            && selected.role == ReplicaRole::Decode
            && scoped_prefix_owner == Some(selected.replica_id)
        {
            self.pd_decode_local_hit_total
                .fetch_add(1, Ordering::Relaxed);
        }

        if selected.role != ReplicaRole::Prefill {
            if let Some(session_id) = ctx.session_id.as_deref() {
                let key = session_key(ctx.tenant_id.as_deref(), session_id);
                self.insert_session_affinity(key, selected.model_uid.clone(), selected.replica_id);
            }
        }

        Ok(selected)
    }

    pub fn route_with_plan_version(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        plan_version: u64,
        prefix_hashes: Option<&[[u8; 32]]>,
        pd_warm: bool,
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(
            ctx,
            model_uid,
            Some(plan_version),
            None,
            prefix_hashes,
            pd_warm,
        )
    }

    /// Run the configured strategy and count a fallback when the primary
    /// signal was unusable, so degradation is observable instead of silent.
    fn select_by_strategy(
        &self,
        sel_ctx: &SelectionCtx<'_>,
        candidates: &[Candidate<'_>],
        candidates_data: &[(EndpointInfo, Option<EndpointStats>)],
    ) -> Result<EndpointInfo, RouteError> {
        let sel = self
            .strategy
            .select(sel_ctx, candidates)
            .ok_or(RouteError::NoEndpoint)?;
        if let Some(reason) = sel.fallback_reason {
            let name = self.strategy.name();
            self.strategy_fallback
                .entry((name, reason))
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                strategy = name,
                reason,
                "routing strategy fell back to least_pending"
            );
        }
        Ok(candidates_data[sel.index].0.clone())
    }

    fn select_hint_biased_endpoint(
        candidates_data: &[(EndpointInfo, Option<EndpointStats>)],
        hint: &InferenceHint,
    ) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (idx, (_, stats)) in candidates_data.iter().enumerate() {
            let mut score = 0.0_f64;
            if let Some(st) = stats {
                if hint.prefer_low_latency.unwrap_or(false) {
                    score += 1.0 / (1.0 + st.pending_requests as f64);
                }
                if hint.prefer_low_cost.unwrap_or(false) {
                    score += 1.0 - st.kv_cache_usage.unwrap_or(0.5).clamp(0.0, 1.0);
                }
                if hint.prefer_prefix_key.is_some() {
                    score += st.prefix_cache_hit_rate.unwrap_or(0.0).clamp(0.0, 1.0) * 2.0;
                }
            }
            if score > 0.0 {
                if let Some((_, current)) = best {
                    if score > current {
                        best = Some((idx, score));
                    }
                } else {
                    best = Some((idx, score));
                }
            }
        }
        best.map(|(idx, _)| idx)
    }

    pub fn route_with_plan_version_excluding(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        plan_version: u64,
        exclude: (&str, u32),
        prefix_hashes: Option<&[[u8; 32]]>,
        pd_warm: bool,
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(
            ctx,
            model_uid,
            Some(plan_version),
            Some(exclude),
            prefix_hashes,
            pd_warm,
        )
    }

    pub fn route(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        prefix_hashes: Option<&[[u8; 32]]>,
        pd_warm: bool,
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(ctx, model_uid, None, None, prefix_hashes, pd_warm)
    }

    pub fn route_excluding(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        exclude: (&str, u32),
        prefix_hashes: Option<&[[u8; 32]]>,
        pd_warm: bool,
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(ctx, model_uid, None, Some(exclude), prefix_hashes, pd_warm)
    }
}

#[cfg(test)]
mod plan_version_tests {
    use super::*;
    use nebula_common::{EndpointKind, EndpointStatus};

    fn ready_ep(model_uid: &str, replica_id: u32, plan_version: u64) -> EndpointInfo {
        EndpointInfo {
            model_uid: model_uid.into(),
            replica_id,
            plan_version,
            node_id: "n1".into(),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".into(),
            status: EndpointStatus::Ready,
            last_heartbeat_ms: 0,
            status_detail: None,
            grpc_target: None,
            base_url: Some(format!("http://127.0.0.1:{}", 8000 + replica_id)),
            engine_type: None,
            role: Default::default(),
        }
    }

    #[test]
    fn pinned_replica_id_selects_target() {
        let router = Router::new();
        router.upsert_endpoint(ready_ep("m1", 0, 1));
        router.upsert_endpoint(ready_ep("m1", 1, 1));
        let ctx = ExecutionContext {
            request_id: "r".into(),
            session_id: None,
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: Some(1),
            inference_hint: None,
            hint_trusted: false,
        };
        let ep = router.route(&ctx, "m1", None, false).unwrap();
        assert_eq!(ep.replica_id, 1);
    }

    #[test]
    fn strategy_fallback_counted_when_signal_missing() {
        // least_kv_cache with no KV stats anywhere → falls back to
        // least_pending; the fallback must be counted, not silent.
        let router = Router::with_strategy(Box::new(strategy::LeastKvCache));
        router.upsert_endpoint(ready_ep("m1", 0, 1));
        router.upsert_endpoint(ready_ep("m1", 1, 1));
        let ctx = ExecutionContext {
            request_id: "r".into(),
            session_id: None,
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: None,
            inference_hint: None,
            hint_trusted: false,
        };
        let ep = router.route(&ctx, "m1", None, false).unwrap();
        assert!(ep.replica_id <= 1);
        assert_eq!(
            router.strategy_fallback_total("least_kv_cache", "no_signal"),
            1
        );
        assert_eq!(
            router.strategy_fallback_total("least_kv_cache", "below_threshold"),
            0
        );
    }

    fn ep_with_role(model: &str, replica: u32, role: ReplicaRole) -> EndpointInfo {
        let mut ep = ready_ep(model, replica, 1);
        ep.role = role;
        ep
    }

    fn ctx_with_session(session: Option<&str>) -> ExecutionContext {
        ExecutionContext {
            request_id: "r".into(),
            session_id: session.map(|s| s.to_string()),
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: None,
            inference_hint: None,
            hint_trusted: false,
        }
    }

    /// pd_split stage routing: cold traffic → prefill pool; session-continued
    /// traffic (anchored on a decode replica) -> decode pool.
    #[test]
    fn pd_split_stage_routing() {
        let router = Router::new();
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
        router.upsert_endpoint(ep_with_role("m1", 1, ReplicaRole::Decode));

        // Cold → prefill pool (writes KV into the shared store).
        let ep = router
            .route(&ctx_with_session(None), "m1", None, false)
            .unwrap();
        assert_eq!(ep.replica_id, 0);

        // Prefix already in the shared store → decode pool (retrieves).
        let h: [u8; 32] = [9; 32];
        router.prefix_index().apply(
            "m1",
            0,
            &[kv_events::KvEvent::Stored {
                block_hashes: vec![h.to_vec()],
                medium: None,
            }],
            1,
        );
        let ep = router
            .route(
                &ctx_with_session(None),
                "m1",
                Some(std::slice::from_ref(&h)),
                false,
            )
            .unwrap();
        assert_eq!(ep.replica_id, 1, "warm prefix should go to the decode pool");

        // Session anchored on the decode replica → decode.
        router
            .session_affinity
            .insert(session_key(None, "s1"), ("m1".into(), 1));
        let ep = router
            .route(&ctx_with_session(Some("s1")), "m1", None, false)
            .unwrap();
        assert_eq!(ep.replica_id, 1);
    }

    #[test]
    fn pd_split_session_affinity_does_not_lock_onto_prefill() {
        let router = Router::new();
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
        router.upsert_endpoint(ep_with_role("m1", 1, ReplicaRole::Decode));

        // First cold request with session_id selects Prefill (replica 0),
        // but must NOT pin the session to Prefill.
        let ep1 = router
            .route(&ctx_with_session(Some("sess-1")), "m1", None, false)
            .unwrap();
        assert_eq!(ep1.replica_id, 0);
        assert!(
            router
                .session_affinity
                .get(&session_key(None, "sess-1"))
                .is_none(),
            "cold prefill turn must not save session_affinity"
        );

        // Even if a prefill replica were somehow present in session_affinity,
        // a subsequent warm turn (controller warm = true) must bypass it and
        // select Decode (replica 1), then anchor the session on Decode.
        router
            .session_affinity
            .insert(session_key(None, "sess-1"), ("m1".into(), 0));
        let ep2 = router
            .route(&ctx_with_session(Some("sess-1")), "m1", None, true)
            .unwrap();
        assert_eq!(ep2.replica_id, 1, "warm turn must route to decode replica");
        assert_eq!(
            router
                .session_affinity
                .get(&session_key(None, "sess-1"))
                .map(|e| e.value().clone()),
            Some(("m1".to_string(), 1))
        );
    }

    #[test]
    fn warm_cache_key_distinguishes_same_first_token_different_suffix() {
        let k1 = warm_cache_key("m1", &[151644, 100, 200]);
        let k2 = warm_cache_key("m1", &[151644, 300, 400]);
        let k3 = warm_cache_key("m1", &[151644, 100, 200]);
        assert_ne!(
            k1, k2,
            "requests sharing BOS / first token must not collide in pd_warm_cache"
        );
        assert_eq!(k1, k3);
    }

    #[test]
    fn warm_cache_evicts_expired_and_caps_capacity() {
        let router = Router::new();
        // Fill to capacity with entries at t = 1_000.
        for i in 0..(PD_WARM_CACHE_MAX_ENTRIES as u32) {
            let key = warm_cache_key("m1", &[151644, i]);
            router.insert_warm_cache_at(key, true, 1_000);
        }
        assert_eq!(router.pd_warm_cache.len(), PD_WARM_CACHE_MAX_ENTRIES);

        // Inserting one more within TTL (t = 1_500) must evict the oldest entry
        // so the cache never exceeds PD_WARM_CACHE_MAX_ENTRIES.
        let fresh_key = warm_cache_key("m1", &[999_999]);
        router.insert_warm_cache_at(fresh_key.clone(), true, 1_500);
        assert_eq!(router.pd_warm_cache.len(), PD_WARM_CACHE_MAX_ENTRIES);
        assert!(router.pd_warm_cache.contains_key(&fresh_key));

        // Once the t=1_000 batch expires (t = 1_000 + PD_WARM_CACHE_MS + 100),
        // inserting at capacity sweeps all expired entries first.
        for i in 0..10u32 {
            let key = warm_cache_key("m1", &[888_000, i]);
            router.insert_warm_cache_at(key, false, 1_500);
        }
        assert_eq!(router.pd_warm_cache.len(), PD_WARM_CACHE_MAX_ENTRIES);
        let after_ttl = 1_000 + PD_WARM_CACHE_MS + 100;
        let post_ttl_key = warm_cache_key("m1", &[777_777]);
        router.insert_warm_cache_at(post_ttl_key.clone(), true, after_ttl);
        assert!(
            router.pd_warm_cache.len() < 32,
            "expired entries must be swept when inserting at capacity, got len={}",
            router.pd_warm_cache.len()
        );
        assert!(router.pd_warm_cache.contains_key(&post_ttl_key));
    }

    #[test]
    fn body_token_ids_supports_completions_and_chat_messages() {
        let tk_json = br#"{
            "version": "1.0",
            "truncation": null,
            "padding": null,
            "added_tokens": [],
            "normalizer": null,
            "pre_tokenizer": {"type": "Whitespace"},
            "post_processor": null,
            "decoder": null,
            "model": {
                "type": "WordLevel",
                "vocab": {"<unk>": 0, "user": 1, "hello": 2, "world": 3, "assistant": 4},
                "unk_token": "<unk>"
            }
        }"#;
        let tk = tokenizers::Tokenizer::from_bytes(tk_json).unwrap();

        let router = Router::new();
        router.prefix_hasher.seed_for_test(
            "m1",
            tk,
            "{% for m in messages %}{{ m.role }} {{ m.content }} {% endfor %}assistant",
        );

        let comp_body = br#"{"model":"m1","prompt":"hello world"}"#;
        assert_eq!(router.body_token_ids("m1", comp_body), Some(vec![2, 3]));

        let chat_body = br#"{"model":"m1","messages":[{"role":"user","content":"hello world"}]}"#;
        assert_eq!(
            router.body_token_ids("m1", chat_body),
            Some(vec![1, 2, 3, 4])
        );
    }

    /// Rig finding: pd_split stage routing needs prefix hashes even when the
    /// strategy is not prefix_affinity — otherwise the warm signal is always
    /// absent and the decode pool is never used.
    #[test]
    fn pd_split_model_computes_hashes_under_any_strategy() {
        let router = Router::with_strategy(Box::new(strategy::LeastPending));
        assert!(!router.model_has_role_pools("m1"));
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
        assert!(router.model_has_role_pools("m1"));
        // Replicated model → hashing stays off for non-affinity strategies.
        router.upsert_endpoint(ready_ep("m2", 0, 1));
        assert!(!router.model_has_role_pools("m2"));
    }

    /// Both controller response shapes must yield the matched-token count
    /// (rig: the live controller nests it under `layout_info`; the docs show
    /// the flat form).
    #[test]
    fn controller_lookup_response_shapes() {
        let nested: serde_json::Value = serde_json::json!({
            "event_id": "Lookupff0e",
            "layout_info": { "qwen25-7b-0": ["LocalCPUBackend", 569] }
        });
        assert_eq!(max_matched_tokens(&nested), Some(569));
        let flat: serde_json::Value = serde_json::json!({
            "event_id": "x",
            "lmcache_default_instance": ["LocalCPUBackend", 512]
        });
        assert_eq!(max_matched_tokens(&flat), Some(512));
        // No match → 0 / absent → None
        let empty: serde_json::Value = serde_json::json!({
            "event_id": "x", "layout_info": {}
        });
        assert_eq!(max_matched_tokens(&empty), None);
        // Multiple instances → the deepest match wins
        let multi: serde_json::Value = serde_json::json!({
            "event_id": "x",
            "layout_info": { "a": ["LocalCPUBackend", 256], "b": ["LocalCPUBackend", 569] }
        });
        assert_eq!(max_matched_tokens(&multi), Some(569));
    }

    #[test]
    fn pd_split_empty_pool_falls_back_with_counter() {
        let router = Router::new();
        // Only a decode replica exists → cold traffic (wants prefill) falls back.
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Decode));
        let ep = router
            .route(&ctx_with_session(None), "m1", None, false)
            .unwrap();
        assert_eq!(ep.replica_id, 0);
        assert_eq!(router.stage_fallback_total("prefill"), 1);
        assert_eq!(router.stage_fallback_total("decode"), 0);
    }

    #[test]
    fn pd_split_two_tier_decode_locality_prefers_local_vram_owner() {
        // 1 Prefill (r0) + 2 Decode (r1, r2).
        // Both Prefill r0 and Decode r2 have stored prefix H (e.g. r2 served an
        // earlier turn and r0 later stored H again). Under both LeastPending and
        // PrefixAffinity, a warm request for H must prefer Decode r2 (Tier-1
        // Node-local GPU hit, skipping TCP KV shuffle) over Decode r1 (which
        // would have to pull KV over TCP from LMCacheServer), without counting
        // an owner_unavailable fallback.
        for strategy in [
            Box::new(strategy::LeastPending) as Box<dyn RoutingStrategy>,
            Box::new(strategy::PrefixAffinity) as Box<dyn RoutingStrategy>,
        ] {
            let router = Router::with_strategy(strategy);
            router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
            router.upsert_endpoint(ep_with_role("m1", 1, ReplicaRole::Decode));
            router.upsert_endpoint(ep_with_role("m1", 2, ReplicaRole::Decode));

            let h: [u8; 32] = [0x42; 32];
            router.prefix_index().apply(
                "m1",
                2,
                &[kv_events::KvEvent::Stored {
                    block_hashes: vec![h.to_vec()],
                    medium: None,
                }],
                100,
            );
            router.prefix_index().apply(
                "m1",
                0,
                &[kv_events::KvEvent::Stored {
                    block_hashes: vec![h.to_vec()],
                    medium: None,
                }],
                200,
            );

            let ep = router
                .route(
                    &ctx_with_session(None),
                    "m1",
                    Some(std::slice::from_ref(&h)),
                    false,
                )
                .unwrap();
            assert_eq!(
                ep.replica_id, 2,
                "Decode replica 2 holds H in local VRAM and must be preferred over Decode replica 1"
            );
            assert_eq!(router.pd_decode_local_hit_total(), 1);
            assert_eq!(
                router.strategy_fallback_total("prefix_affinity", "owner_unavailable"),
                0,
                "multi-owner candidate-scoped lookup must not report owner_unavailable when a Decode replica holds the prefix"
            );
        }
    }

    #[test]
    fn pd_split_saturated_prefill_spills_cold_traffic_to_idle_decode() {
        let router = Router::new();
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
        router.upsert_endpoint(ep_with_role("m1", 1, ReplicaRole::Decode));

        let now = now_ms();
        // Prefill replica 0 is busy (pending_requests = 2), while Decode replica 1
        // initially has NO stats (None): must NOT be treated as pending=0 even when
        // spillover is enabled.
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 0,
            last_updated_ms: now,
            pending_requests: 2,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.2),
        });

        // By default (DEFAULT_PD_PREFILL_SPILLOVER_PENDING = 0), spillover is off.
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: now,
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.1),
        });
        let ep_default = router
            .route(&ctx_with_session(None), "m1", None, false)
            .unwrap();
        assert_eq!(
            ep_default.replica_id, 0,
            "by default spillover is disabled (0) so cold traffic stays on Prefill"
        );
        assert_eq!(router.stage_prefill_spillover_total(), 0);

        // Opt in to spillover (threshold = 1), but if Decode replica 1 lacks fresh
        // stats, it must NOT be treated as pending=0.
        router.set_pd_prefill_spillover_pending(1);
        router.remove_stats("m1", 1);
        let ep_no_decode_stats = router
            .route(&ctx_with_session(None), "m1", None, false)
            .unwrap();
        assert_eq!(
            ep_no_decode_stats.replica_id, 0,
            "Decode replica without stats must not be treated as pending=0"
        );
        assert_eq!(router.stage_prefill_spillover_total(), 0);

        // Once Decode replica 1 reports fresh stats with pending_requests = 0,
        // opt-in spillover routes cold request to Decode replica 1.
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: now,
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.1),
        });

        let ep = router
            .route(&ctx_with_session(Some("cold-sess")), "m1", None, false)
            .unwrap();
        assert_eq!(
            ep.replica_id, 1,
            "cold request must spill over to idle Decode (kv_both) replica when opted in and Prefill replica is saturated"
        );
        assert_eq!(router.stage_prefill_spillover_total(), 1);
        assert_eq!(
            router
                .session_affinity
                .get(&session_key(None, "cold-sess"))
                .map(|e| e.value().clone()),
            Some(("m1".to_string(), 1)),
            "spilled request handled by Decode replica anchors session directly on that Decode replica"
        );
    }

    #[test]
    fn replicated_ignores_stage_filtering() {
        let router = Router::new();
        router.upsert_endpoint(ready_ep("m1", 0, 1));
        router.upsert_endpoint(ready_ep("m1", 1, 1));
        let ep = router
            .route(&ctx_with_session(None), "m1", None, false)
            .unwrap();
        assert!(ep.replica_id <= 1);
        assert_eq!(router.stage_fallback_total("prefill"), 0);
    }

    #[test]
    fn prefix_affinity_routes_to_owner_and_counts_fallback() {
        let router = Router::with_strategy(Box::new(strategy::PrefixAffinity));
        router.upsert_endpoint(ready_ep("m1", 0, 1));
        router.upsert_endpoint(ready_ep("m1", 1, 1));
        // Learn from a (synthetic) KV event: replica 1 holds hash H.
        let h: [u8; 32] = [7; 32];
        router.prefix_index().apply(
            "m1",
            1,
            &[kv_events::KvEvent::Stored {
                block_hashes: vec![h.to_vec()],
                medium: None,
            }],
            1,
        );
        let ctx = ExecutionContext {
            request_id: "r".into(),
            session_id: None,
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: None,
            inference_hint: None,
            hint_trusted: false,
        };
        let ep = router
            .route(&ctx, "m1", Some(std::slice::from_ref(&h)), false)
            .unwrap();
        assert_eq!(ep.replica_id, 1);
        // Unknown prefix → least_pending fallback, counted as no_signal.
        let ep2 = router.route(&ctx, "m1", None, false).unwrap();
        assert!(ep2.replica_id <= 1);
        assert_eq!(
            router.strategy_fallback_total("prefix_affinity", "no_signal"),
            1
        );
        // Owner known but drained (not a candidate) → owner_unavailable,
        // never a hard failure.
        let mut drained = ready_ep("m1", 1, 1);
        drained.status = EndpointStatus::Draining;
        router.upsert_endpoint(drained);
        let ep3 = router
            .route(&ctx, "m1", Some(std::slice::from_ref(&h)), false)
            .unwrap();
        assert_eq!(ep3.replica_id, 0);
        assert_eq!(
            router.strategy_fallback_total("prefix_affinity", "owner_unavailable"),
            1
        );
    }

    #[test]
    fn plan_version_filters_stale_endpoints() {
        let router = Router::new();
        router.upsert_endpoint(ready_ep("m1", 0, 1));
        router.upsert_endpoint(ready_ep("m1", 1, 2));
        let ctx = ExecutionContext {
            request_id: "r1".into(),
            session_id: None,
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: None,
            inference_hint: None,
            hint_trusted: false,
        };

        let ep = router
            .route_with_plan_version(&ctx, "m1", 2, None, false)
            .unwrap();
        assert_eq!(ep.replica_id, 1);
        assert_eq!(ep.plan_version, 2);

        let err = router
            .route_with_plan_version(&ctx, "m1", 99, None, false)
            .unwrap_err();
        assert!(matches!(err, RouteError::NoEndpoint));
    }

    #[test]
    fn per_model_plan_version_independent() {
        let router = Router::new();
        router.set_plan_version("m1", 3);
        router.set_plan_version("m2", 7);
        assert_eq!(router.plan_version_for("m1"), Some(3));
        assert_eq!(router.plan_version_for("m2"), Some(7));
        router.clear_plan_version("m1");
        assert_eq!(router.plan_version_for("m1"), None);
        assert_eq!(router.plan_version_for("m2"), Some(7));

        router.upsert_endpoint(ready_ep("m2", 0, 7));
        router.upsert_endpoint(ready_ep("m2", 1, 6)); // stale
        let ctx = ExecutionContext {
            request_id: "r".into(),
            session_id: None,
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: None,
            inference_hint: None,
            hint_trusted: false,
        };
        let ep = router
            .route_with_plan_version(&ctx, "m2", 7, None, false)
            .unwrap();
        assert_eq!(ep.replica_id, 0);
    }

    #[test]
    fn draining_endpoints_are_not_selected() {
        let router = Router::new();
        let mut ep = ready_ep("m1", 0, 1);
        ep.status = EndpointStatus::Draining;
        router.upsert_endpoint(ep);
        let ctx = ExecutionContext {
            request_id: "r1".into(),
            session_id: None,
            tenant_id: None,
            priority: None,
            deadline_ms: None,
            budget_tokens: None,
            pinned_replica_id: None,
            inference_hint: None,
            hint_trusted: false,
        };
        assert!(matches!(
            router
                .route_with_plan_version(&ctx, "m1", 1, None, false)
                .unwrap_err(),
            RouteError::NoEndpoint
        ));
    }

    #[test]
    fn pd_split_short_prompt_bypasses_prefill_to_unsaturated_decode() {
        let router = Router::new();
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
        router.upsert_endpoint(ep_with_role("m1", 1, ReplicaRole::Decode));

        let short_hashes = [[0x11u8; 32], [0x22u8; 32]];
        // When Decode replica 1 has NO stats (`None`), it must NOT be treated as
        // `pending_requests = 0` — short prompt stays on Prefill.
        let ep_no_stats = router
            .route(&ctx_with_session(None), "m1", Some(&short_hashes), false)
            .unwrap();
        assert_eq!(ep_no_stats.replica_id, 0);
        assert_eq!(router.pd_short_bypass_total(), 0);

        // Once Decode replica 1 reports fresh unsaturated stats (pending=0),
        // 2 blocks = 32 tokens < DEFAULT_PD_BYPASS_TOKENS (256) bypasses Prefill
        // directly to Decode.
        let now = now_ms();
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 0,
            last_updated_ms: now,
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.1),
        });
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: now,
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.1),
        });
        let ep = router
            .route(&ctx_with_session(None), "m1", Some(&short_hashes), false)
            .unwrap();
        assert_eq!(ep.replica_id, 1);
        assert_eq!(router.pd_short_bypass_total(), 1);

        // Long cold prompt (16 blocks = 256 tokens >= 256) goes to Prefill.
        let long_hashes = [[0x33u8; 32]; 16];
        let ep_long = router
            .route(&ctx_with_session(None), "m1", Some(&long_hashes), false)
            .unwrap();
        assert_eq!(ep_long.replica_id, 0);
        assert_eq!(router.pd_short_bypass_total(), 1);

        // When Decode is busy (pending=3) and Prefill is idle (pending=0),
        // short prompt does NOT bypass to saturated Decode.
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: now,
            pending_requests: 3,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.4),
        });
        let ep_busy = router
            .route(&ctx_with_session(None), "m1", Some(&short_hashes), false)
            .unwrap();
        assert_eq!(ep_busy.replica_id, 0);
        assert_eq!(router.pd_short_bypass_total(), 1);
    }

    #[test]
    fn prefix_affinity_multi_owner_joint_cost_and_congested_spillover() {
        let router = Router::with_strategy(Box::new(strategy::PrefixAffinity));
        router.upsert_endpoint(ready_ep("m1", 0, 1));
        router.upsert_endpoint(ready_ep("m1", 1, 1));

        let h: [u8; 32] = [0x55; 32];
        router.prefix_index().apply(
            "m1",
            0,
            &[kv_events::KvEvent::Stored {
                block_hashes: vec![h.to_vec()],
                medium: None,
            }],
            100,
        );

        let now = now_ms();
        // Owner replica 0 is congested (pending = 4 => cost 256 > 16 saved tokens),
        // while replica 1 is idle (pending = 0).
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 0,
            last_updated_ms: now,
            pending_requests: 4,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.5),
        });
        router.upsert_stats(EndpointStats {
            model_uid: "m1".into(),
            replica_id: 1,
            last_updated_ms: now,
            pending_requests: 0,
            prefix_cache_hit_rate: None,
            prefix_cache_queries_total: None,
            prompt_cache_hit_rate: None,
            kv_cache_usage: Some(0.1),
        });

        let ep = router
            .route(
                &ctx_with_session(None),
                "m1",
                Some(std::slice::from_ref(&h)),
                false,
            )
            .unwrap();
        assert_eq!(ep.replica_id, 1);
        assert_eq!(
            router.strategy_fallback_total("prefix_affinity", "owner_congested"),
            1
        );
    }

    #[test]
    fn session_affinity_eviction_does_not_deadlock_over_max_entries() {
        let router = Router::new();
        router.upsert_endpoint(ready_ep("m1", 0, 1));
        for i in 0..=SESSION_AFFINITY_MAX_ENTRIES {
            let sid = format!("sess-{i}");
            let ep = router
                .route(&ctx_with_session(Some(&sid)), "m1", None, false)
                .unwrap();
            assert_eq!(ep.replica_id, 0);
        }
        assert_eq!(router.session_affinity.len(), SESSION_AFFINITY_MAX_ENTRIES);
    }

    #[test]
    fn pd_split_empty_decode_pool_fallback_does_not_count_decode_local_hit() {
        let router = Router::new();
        // Only a Prefill replica is Ready; Decode pool is empty.
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
        let h: [u8; 32] = [0x99; 32];
        router.prefix_index().apply(
            "m1",
            0,
            &[kv_events::KvEvent::Stored {
                block_hashes: vec![h.to_vec()],
                medium: None,
            }],
            1,
        );

        let ep = router
            .route(
                &ctx_with_session(None),
                "m1",
                Some(std::slice::from_ref(&h)),
                true,
            )
            .unwrap();
        assert_eq!(ep.replica_id, 0);
        assert_eq!(ep.role, ReplicaRole::Prefill);
        assert_eq!(router.stage_fallback_total("decode"), 1);
        assert_eq!(
            router.pd_decode_local_hit_total(),
            0,
            "falling back to a Prefill replica when Decode pool is empty must not count as a Decode local hit"
        );
    }

    #[test]
    fn tool_pause_lease_marks_and_isolates_by_tenant_without_body_spoofing() {
        let router = Router::new();
        router.upsert_endpoint(ep_with_role("m1", 0, ReplicaRole::Prefill));
        router.upsert_endpoint(ep_with_role("m1", 1, ReplicaRole::Decode));
        router.upsert_endpoint(ep_with_role("m1", 2, ReplicaRole::Decode));

        let resp_json = br#"{"choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","tool_calls":[{"id":"call_1","type":"function","function":{"name":"bash","arguments":"{}"}}]}}]}"#;
        assert!(response_has_tool_calls(resp_json));

        // Without a server-recorded lease, no session is granted continuation
        // privilege (client-supplied `role:"tool"` cannot spoof privilege).
        assert!(!router.is_tool_continuation_for_tenant(Some("tenant-a"), Some("agent-sess-1")));
        assert!(!router.is_tool_continuation(Some("agent-sess-1")));

        // Server records a tool-pause lease for (tenant-a, agent-sess-1) on Decode replica 2.
        router.mark_tool_pause_for_tenant_at(Some("tenant-a"), "agent-sess-1", "m1", 2, now_ms());
        assert_eq!(router.tool_pause_marked_total(), 1);
        // Tenant B reusing the same x-session-id does NOT inherit Tenant A's
        // continuation privilege or replica affinity.
        assert!(!router.is_tool_continuation_for_tenant(Some("tenant-b"), Some("agent-sess-1")));
        assert!(!router.is_tool_continuation_for_tenant(None, Some("agent-sess-1")));

        // Tenant A claims queue continuation priority once; a second claim
        // without a new `tool_calls` response is rejected (single-use lease).
        assert!(router.is_tool_continuation_for_tenant(Some("tenant-a"), Some("agent-sess-1")));
        assert!(!router.is_tool_continuation_for_tenant(Some("tenant-a"), Some("agent-sess-1")));

        let mut ctx_a = ctx_with_session(Some("agent-sess-1"));
        ctx_a.tenant_id = Some("tenant-a".into());
        let ep_a = router.route(&ctx_a, "m1", None, false).unwrap();
        assert_eq!(ep_a.replica_id, 2);
        assert_eq!(router.tool_continuation_hit_total(), 1);
        assert!(
            !router.has_active_tool_pause_for_tenant(Some("tenant-a"), "agent-sess-1"),
            "lease is invalidated after being used once"
        );

        // Subsequent request on the same session still enjoys normal session
        // affinity on Decode replica 2, but does NOT count as a second tool
        // continuation hit unless a new tool_calls response marks a fresh lease.
        let ep_a2 = router.route(&ctx_a, "m1", None, false).unwrap();
        assert_eq!(ep_a2.replica_id, 2);
        assert_eq!(router.tool_continuation_hit_total(), 1);

        let mut ctx_b = ctx_with_session(Some("agent-sess-1"));
        ctx_b.tenant_id = Some("tenant-b".into());
        let ep_b = router.route(&ctx_b, "m1", None, false).unwrap();
        assert_eq!(
            ep_b.replica_id, 0,
            "tenant-b cold request with same session_id must route to Prefill, not tenant-a's Decode anchor"
        );
        assert_eq!(router.tool_continuation_hit_total(), 1);
    }

    #[test]
    fn body_token_ids_supports_score_query_and_excludes_rerank() {
        let tk_json = br#"{
            "version": "1.0",
            "truncation": null,
            "padding": null,
            "added_tokens": [],
            "normalizer": null,
            "pre_tokenizer": {"type": "Whitespace"},
            "post_processor": null,
            "decoder": null,
            "model": {
                "type": "WordLevel",
                "vocab": {"<unk>": 0, "user": 1, "hello": 2, "world": 3, "assistant": 4},
                "unk_token": "<unk>"
            }
        }"#;
        let tk = tokenizers::Tokenizer::from_bytes(tk_json).unwrap();
        let router = Router::new();
        router.prefix_hasher.seed_for_test("m1", tk, "");

        let score_query = br#"{"model":"m1","query":"hello world","items":["yes","no"]}"#;
        assert_eq!(router.body_token_ids("m1", score_query), Some(vec![2, 3]));

        let score_ids = br#"{"model":"m1","query":[2,3],"label_token_ids":[1,4]}"#;
        assert_eq!(router.body_token_ids("m1", score_ids), Some(vec![2, 3]));

        // /v1/rerank body (`{"query", "documents"}`) must NOT be tokenized as /v1/score.
        let rerank_body = br#"{"model":"m1","query":"hello world","documents":["doc1","doc2"]}"#;
        assert_eq!(router.body_token_ids("m1", rerank_body), None);
    }
}

#[cfg(test)]
mod model_alias_tests {
    use super::*;
    use nebula_common::{ModelConfig, ModelSource, ModelSpec};

    fn sample_spec(uid: &str, name: &str, served: Option<&str>, path: Option<&str>) -> ModelSpec {
        ModelSpec {
            model_uid: uid.into(),
            model_name: name.into(),
            model_source: ModelSource::Local,
            model_path: path.map(|s| s.into()),
            engine_type: Some("vllm".into()),
            docker_image: None,
            config: served.map(|s| ModelConfig {
                served_model_name: Some(s.into()),
                ..Default::default()
            }),
            labels: Default::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: None,
        }
    }

    #[test]
    fn resolve_served_model_name_and_engine_rewrite() {
        let router = Router::new();
        router.register_model_spec(&sample_spec(
            "qwen15_moe_vllm",
            "Qwen/Qwen1.5-MoE-A2.7B-Chat",
            Some("qwen15-moe-vllm"),
            Some("/home/bodesi/models/Qwen1.5-MoE-A2.7B-Chat"),
        ));

        assert_eq!(router.resolve_model("qwen15-moe-vllm"), "qwen15_moe_vllm");
        assert_eq!(
            router.resolve_model("Qwen/Qwen1.5-MoE-A2.7B-Chat"),
            "qwen15_moe_vllm"
        );
        assert_eq!(
            router.get_engine_model_name("qwen15_moe_vllm").as_deref(),
            Some("qwen15-moe-vllm")
        );
    }

    #[test]
    fn routes_third_party_endpoint_without_plan_version() {
        let router = Router::new();
        // Endpoint registered by PowerLLM without scheduler plan_version (defaults to 0)
        let json = r#"{
            "model_uid": "qwen2.5-7b",
            "replica_id": 0,
            "status": "ready",
            "base_url": "http://10.99.255.102:9997",
            "engine_type": "powerllm"
        }"#;
        let ep: EndpointInfo = serde_json::from_str(json).unwrap();
        router.upsert_endpoint(ep);

        let ctx = ExecutionContext::default();
        let selected = router.route(&ctx, "qwen2.5-7b", None, false).unwrap();
        assert_eq!(selected.model_uid, "qwen2.5-7b");
        assert_eq!(selected.replica_id, 0);
        assert_eq!(
            selected.base_url.as_deref(),
            Some("http://10.99.255.102:9997")
        );
        assert_eq!(selected.engine_type.as_deref(), Some("powerllm"));
    }
}

#[cfg(test)]
mod model_spec_update_tests {
    use super::*;

    fn spec(uid: &str, name: &str, served: Option<&str>) -> ModelSpec {
        ModelSpec {
            model_uid: uid.into(),
            model_name: name.into(),
            model_source: nebula_common::ModelSource::Local,
            model_path: Some("/models/some-path".into()),
            engine_type: Some("vllm".into()),
            docker_image: None,
            config: served.map(|s| {
                serde_json::from_str(&format!(r#"{{"served_model_name":"{s}"}}"#)).unwrap()
            }),
            labels: Default::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: None,
        }
    }

    /// Regression: updating a spec re-registers mappings via
    /// clear_model_mappings, which used to get+remove the same
    /// `model_names` key under one read guard and deadlock the shard
    /// (froze the whole router on a real-machine spec update).
    #[test]
    fn spec_update_does_not_deadlock() {
        let router = Router::new();
        router.register_model_spec(&spec("m1", "Org/Model-A", Some("m1")));
        router.register_model_spec(&spec("m1", "Org/Model-B", Some("m1")));
        let (tx, rx) = std::sync::mpsc::channel();
        let r = router.clone();
        std::thread::spawn(move || {
            for _ in 0..50 {
                r.register_model_spec(&spec("m1", "Org/Model-A", Some("m1")));
                r.register_model_spec(&spec("m1", "Org/Model-B", Some("m1")));
            }
            tx.send(()).unwrap();
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("spec re-registration deadlocked (DashMap get+remove on same key)");
        assert_eq!(router.resolve_model("Org/Model-B"), "m1");
        assert_eq!(router.resolve_model("m1"), "m1");
    }
}
