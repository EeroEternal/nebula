use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use nebula_common::{EndpointInfo, EndpointStats, EndpointStatus, ExecutionContext};

pub mod strategy;

use strategy::{Candidate, LeastPending, RoutingStrategy};

/// KV cache usage threshold (fraction 0.0–1.0). When ALL endpoints exceed this,
/// admission control kicks in and returns Overloaded.
const KV_CACHE_OVERLOAD_THRESHOLD: f64 = 0.95;

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
    session_affinity: DashMap<String, (String, u32)>,
    strategy: Box<dyn RoutingStrategy>,
    /// model_name → model_uid (e.g. "Qwen/Qwen2.5-0.5B-Instruct" → "qwen2_5_0_5b")
    model_names: DashMap<String, String>,
    /// model_uid → model_name (reverse mapping)
    model_uids_to_names: DashMap<String, String>,
    xtrace_query_errors_total: AtomicU64,
    xtrace_rate_limited_total: AtomicU64,
    xtrace_stale_total: AtomicU64,
    xtrace_truncated_total: AtomicU64,
    route_stale_stats_dropped_total: AtomicU64,
    route_circuit_skipped_total: AtomicU64,
    circuit_open_total: AtomicU64,
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
        Arc::new(Self {
            endpoints: DashMap::new(),
            stats: DashMap::new(),
            session_affinity: DashMap::new(),
            strategy,
            model_names: DashMap::new(),
            model_uids_to_names: DashMap::new(),
            xtrace_query_errors_total: AtomicU64::new(0),
            xtrace_rate_limited_total: AtomicU64::new(0),
            xtrace_stale_total: AtomicU64::new(0),
            xtrace_truncated_total: AtomicU64::new(0),
            route_stale_stats_dropped_total: AtomicU64::new(0),
            route_circuit_skipped_total: AtomicU64::new(0),
            circuit_open_total: AtomicU64::new(0),
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
    }

    pub fn upsert_endpoint(&self, info: EndpointInfo) {
        self.endpoints
            .insert((info.model_uid.clone(), info.replica_id), info);
    }

    pub fn remove_endpoint(&self, model_uid: &str, replica_id: u32) {
        self.endpoints.remove(&(model_uid.to_string(), replica_id));
    }

    pub fn upsert_stats(&self, stats: EndpointStats) {
        self.stats
            .insert((stats.model_uid.clone(), stats.replica_id), stats);
    }

    pub fn inc_xtrace_query_errors(&self) {
        self.xtrace_query_errors_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_xtrace_rate_limited(&self) {
        self.xtrace_rate_limited_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_xtrace_stale(&self) {
        self.xtrace_stale_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_xtrace_truncated(&self) {
        self.xtrace_truncated_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn xtrace_query_errors_total(&self) -> u64 {
        self.xtrace_query_errors_total.load(Ordering::Relaxed)
    }

    pub fn xtrace_rate_limited_total(&self) -> u64 {
        self.xtrace_rate_limited_total.load(Ordering::Relaxed)
    }

    pub fn xtrace_stale_total(&self) -> u64 {
        self.xtrace_stale_total.load(Ordering::Relaxed)
    }

    pub fn xtrace_truncated_total(&self) -> u64 {
        self.xtrace_truncated_total.load(Ordering::Relaxed)
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

    pub fn record_endpoint_success(&self, model_uid: &str, replica_id: u32) {
        self.endpoint_circuit
            .remove(&(model_uid.to_string(), replica_id));
    }

    pub fn record_endpoint_failure(&self, model_uid: &str, replica_id: u32) {
        let key = (model_uid.to_string(), replica_id);
        let now = now_ms();
        let mut entry = self
            .endpoint_circuit
            .entry(key)
            .or_insert_with(EndpointCircuitState::default);

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
        self.session_affinity.remove(session_id);
    }

    /// Register a bidirectional model_uid ↔ model_name mapping.
    pub fn set_model_mapping(&self, model_uid: &str, model_name: &str) {
        self.model_names
            .insert(model_name.to_string(), model_uid.to_string());
        self.model_uids_to_names
            .insert(model_uid.to_string(), model_name.to_string());
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
    ) -> Result<EndpointInfo, RouteError> {
        // Session affinity check
        if let Some(session_id) = ctx.session_id.as_deref() {
            if let Some(aff) = self.session_affinity.get(session_id) {
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
                        if ep.status == EndpointStatus::Ready && plan_ok {
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
            let with_stats: Vec<(EndpointInfo, Option<EndpointStats>)> = candidates_data
                .iter()
                .filter(|(_, s)| s.is_some())
                .map(|(ep, s)| (ep.clone(), s.clone()))
                .collect();
            if !with_stats.is_empty() {
                candidates_data = with_stats;
            }
        }

        // Candidate reduction: filter out confirmed overloaded endpoints first.
        let has_kv_data = candidates_data.iter().any(|(_, s)| {
            s.as_ref()
                .map(|st| st.kv_cache_used_bytes.is_some() && st.kv_cache_free_bytes.is_some())
                .unwrap_or(false)
        });

        if has_kv_data {
            let non_overloaded: Vec<(EndpointInfo, Option<EndpointStats>)> = candidates_data
                .into_iter()
                .filter(|(_, s)| {
                    if let Some(st) = s {
                        if let (Some(used), Some(free)) =
                            (st.kv_cache_used_bytes, st.kv_cache_free_bytes)
                        {
                            let total = used.saturating_add(free);
                            if total == 0 {
                                return false;
                            }
                            let usage = used as f64 / total as f64;
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

        let candidates: Vec<Candidate> = candidates_data
            .iter()
            .map(|(ep, s)| Candidate {
                endpoint: ep,
                stats: s.as_ref(),
            })
            .collect();

        let selected = self
            .strategy
            .select(&candidates)
            .map(|i| candidates_data[i].0.clone())
            .ok_or(RouteError::NoEndpoint)?;

        if let Some(session_id) = ctx.session_id.clone() {
            self.session_affinity.insert(
                session_id,
                (selected.model_uid.clone(), selected.replica_id),
            );
        }

        Ok(selected)
    }

    pub fn route_with_plan_version(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        plan_version: u64,
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(ctx, model_uid, Some(plan_version), None)
    }

    pub fn route_with_plan_version_excluding(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        plan_version: u64,
        exclude: (&str, u32),
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(ctx, model_uid, Some(plan_version), Some(exclude))
    }

    pub fn route(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(ctx, model_uid, None, None)
    }

    pub fn route_excluding(
        &self,
        ctx: &ExecutionContext,
        model_uid: &str,
        exclude: (&str, u32),
    ) -> Result<EndpointInfo, RouteError> {
        self.route_internal(ctx, model_uid, None, Some(exclude))
    }
}
