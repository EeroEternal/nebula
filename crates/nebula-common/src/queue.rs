//! Engine-level bounded fair-queue admission (gateway egress).
//!
//! Bounds in-flight concurrency per model/engine to the latency sweet spot and
//! absorbs bursts in a **bounded, per-tenant-fair queue** instead of overloading
//! the engine.
//!
//! Deliberately **not adaptive** (no AIMD): queue-induced TTFT must not be read
//! as upstream congestion — that self-locks under burst load. See
//! `docs/dev/queue-admission.md`.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};

/// Queue execution mode: wait for a permit (fair) vs reject with 429 (fast fail).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueMode {
    Fair,
    FastFail,
}

impl QueueMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fair" | "queue" | "wait" => Some(Self::Fair),
            "fast_fail" | "fastfail" | "fail_fast" | "failfast" | "reject" => Some(Self::FastFail),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fair => "fair",
            Self::FastFail => "fast_fail",
        }
    }
}

/// Reason a request was not admitted. Maps to a low-cardinality metric label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueDeny {
    /// Global queue depth reached `max_size` (or fast-fail with no permit free).
    QueueFull,
    /// This tenant's share of the queue is full.
    TenantQueueFull,
    /// Waited `max_wait` without a permit.
    Timeout,
    /// Queue closed (engine gone).
    Closed,
}

impl QueueDeny {
    pub fn code(&self) -> &'static str {
        match self {
            Self::QueueFull => "queue_full",
            Self::TenantQueueFull => "tenant_queue_full",
            Self::Timeout => "queue_timeout",
            Self::Closed => "queue_closed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub mode: QueueMode,
    pub max_concurrency: usize,
    pub max_size: usize,
    pub max_wait: Duration,
    /// Per-tenant cap as a fraction of `max_size` (e.g. 0.5).
    pub tenant_share: f64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            mode: QueueMode::Fair,
            max_concurrency: 32,
            max_size: 500,
            max_wait: Duration::from_millis(2000),
            tenant_share: 0.5,
        }
    }
}

#[derive(Debug, Default)]
struct WaitState {
    total: usize,
    per_tenant: HashMap<String, usize>,
}

const DEFAULT_TENANT: &str = "__default__";

/// Maximum consecutive priority continuation handoffs before yielding a slot
/// back to `self.sem` so a queued cold FIFO waiter makes progress (prevents
/// cold-request starvation under sustained continuation traffic).
const MAX_CONSECUTIVE_PRIORITY_HANDOFFS: usize = 4;

/// Default aging weight (turns per second waited) for Turn-Priority fairness
/// (`score = turn_number + time_weight * wait_secs`), matching llm-d's
/// `turn-priority` formula so shallow sessions age up and never starve.
const DEFAULT_TURN_PRIORITY_TIME_WEIGHT: f64 = 0.5;
const DEFAULT_TURN_PRIORITY_INACTIVITY_SECS: u64 = 120;
const MAX_TURN_TRACKER_ENTRIES: usize = 10_000;

struct PriorityWaiter {
    id: u64,
    turn_number: u64,
    enqueued_at: Instant,
    tx: oneshot::Sender<OwnedSemaphorePermit>,
}

struct PriorityWaiterState {
    waiters: VecDeque<PriorityWaiter>,
    next_waiter_id: u64,
    consecutive_handoffs: usize,
    cold_waiters: usize,
    time_weight: f64,
}

impl Default for PriorityWaiterState {
    fn default() -> Self {
        let time_weight = std::env::var("NEBULA_TURN_PRIORITY_TIME_WEIGHT")
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|w| w.is_finite() && *w >= 0.0)
            .unwrap_or(DEFAULT_TURN_PRIORITY_TIME_WEIGHT);
        Self {
            waiters: VecDeque::new(),
            next_waiter_id: 0,
            consecutive_handoffs: 0,
            cold_waiters: 0,
            time_weight,
        }
    }
}

#[inline]
fn turn_priority_score(turn_number: u64, wait: Duration, time_weight: f64) -> f64 {
    (turn_number as f64) + time_weight * wait.as_secs_f64()
}

type PriorityWaiterQueue = Arc<Mutex<PriorityWaiterState>>;

/// One engine's (model's) bounded fair queue.
pub struct EngineQueue {
    cfg: QueueConfig,
    sem: Arc<Semaphore>,
    /// Cluster member count the engine cap is divided by (issue #68). The
    /// semaphore holds the single-replica ceiling; this divisor enforces this
    /// replica's share of it and follows the `/gateways/` registry live.
    divisor: Arc<AtomicU32>,
    priority_waiters: PriorityWaiterQueue,
    waiting: Mutex<WaitState>,
    inflight: Arc<AtomicUsize>,
    admitted: AtomicUsize,
    admitted_continuation: AtomicUsize,
    queued: AtomicUsize,
    denied_full: AtomicUsize,
    denied_tenant: AtomicUsize,
    denied_timeout: AtomicUsize,
}

/// Held for the request lifetime (including the streamed response). Dropping it
/// releases the concurrency slot, handing off directly to any queued
/// continuation waiter (up to `MAX_CONSECUTIVE_PRIORITY_HANDOFFS` consecutive
/// times when cold waiters are present) before falling back to the cold FIFO semaphore.
pub struct EnginePermit {
    permit: Option<OwnedSemaphorePermit>,
    priority_waiters: PriorityWaiterQueue,
    inflight: Arc<AtomicUsize>,
}

impl Drop for EnginePermit {
    fn drop(&mut self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
        if let Some(permit) = self.permit.take() {
            let mut st = self
                .priority_waiters
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if st.consecutive_handoffs >= MAX_CONSECUTIVE_PRIORITY_HANDOFFS && st.cold_waiters > 0 {
                // Yield one slot back to `self.sem` while holding `st` so the head
                // cold FIFO waiter wakes up; reset the consecutive priority counter.
                st.consecutive_handoffs = 0;
                drop(permit);
                return;
            }
            let mut handoff = Some(permit);
            let now = Instant::now();
            while !st.waiters.is_empty() {
                let best_idx = st
                    .waiters
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| {
                        let sa = turn_priority_score(
                            a.turn_number,
                            now.saturating_duration_since(a.enqueued_at),
                            st.time_weight,
                        );
                        let sb = turn_priority_score(
                            b.turn_number,
                            now.saturating_duration_since(b.enqueued_at),
                            st.time_weight,
                        );
                        sa.partial_cmp(&sb)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            // On tie, prefer earlier waiter (lower id)
                            .then_with(|| b.id.cmp(&a.id))
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let waiter = st.waiters.remove(best_idx).expect("valid index");
                match waiter.tx.send(handoff.take().unwrap()) {
                    Ok(()) => {
                        st.consecutive_handoffs += 1;
                        break;
                    }
                    Err(returned_permit) => {
                        handoff = Some(returned_permit);
                    }
                }
            }
            if let Some(unclaimed) = handoff {
                st.consecutive_handoffs = 0;
                drop(unclaimed);
            }
        }
    }
}

impl std::fmt::Debug for EnginePermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnginePermit").finish_non_exhaustive()
    }
}

/// Decrements the waiting counters on drop, so a cancelled waiter does not leak
/// queue depth.
struct WaitingGuard<'a> {
    queue: &'a EngineQueue,
    key: String,
}

impl Drop for WaitingGuard<'_> {
    fn drop(&mut self) {
        let mut st = self.queue.waiting.lock().unwrap_or_else(|e| e.into_inner());
        st.total = st.total.saturating_sub(1);
        if let Some(n) = st.per_tenant.get_mut(&self.key) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                st.per_tenant.remove(&self.key);
            }
        }
    }
}

impl EngineQueue {
    pub fn new(cfg: QueueConfig) -> Arc<Self> {
        Self::new_with_divisor(cfg, Arc::new(AtomicU32::new(1)))
    }

    fn new_with_divisor(cfg: QueueConfig, divisor: Arc<AtomicU32>) -> Arc<Self> {
        let max = cfg.max_concurrency.max(1);
        Arc::new(Self {
            cfg,
            sem: Arc::new(Semaphore::new(max)),
            divisor,
            priority_waiters: Arc::new(Mutex::new(PriorityWaiterState::default())),
            waiting: Mutex::new(WaitState::default()),
            inflight: Arc::new(AtomicUsize::new(0)),
            admitted: AtomicUsize::new(0),
            admitted_continuation: AtomicUsize::new(0),
            queued: AtomicUsize::new(0),
            denied_full: AtomicUsize::new(0),
            denied_tenant: AtomicUsize::new(0),
            denied_timeout: AtomicUsize::new(0),
        })
    }

    pub fn config(&self) -> &QueueConfig {
        &self.cfg
    }

    pub fn inflight(&self) -> usize {
        self.inflight.load(Ordering::Relaxed)
    }

    pub fn depth(&self) -> usize {
        self.waiting.lock().map(|s| s.total).unwrap_or(0)
    }

    pub fn admitted_total(&self) -> usize {
        self.admitted.load(Ordering::Relaxed)
    }

    pub fn admitted_continuation_total(&self) -> usize {
        self.admitted_continuation.load(Ordering::Relaxed)
    }

    pub fn queued_total(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }

    pub fn denied_total(&self) -> (usize, usize, usize) {
        (
            self.denied_full.load(Ordering::Relaxed),
            self.denied_tenant.load(Ordering::Relaxed),
            self.denied_timeout.load(Ordering::Relaxed),
        )
    }

    /// In-flight ceiling for this replica: the configured engine sweetness
    /// divided by the live Gateway count, never below 1.
    pub fn cluster_cap(&self) -> usize {
        let members = self.divisor.load(Ordering::Relaxed).max(1) as usize;
        (self.cfg.max_concurrency.max(1) / members).max(1)
    }

    fn over_cluster_cap(&self) -> bool {
        self.inflight.load(Ordering::Relaxed) >= self.cluster_cap()
    }

    fn make_permit(
        &self,
        permit: OwnedSemaphorePermit,
        priority_continuation: bool,
    ) -> EnginePermit {
        self.admitted.fetch_add(1, Ordering::Relaxed);
        if priority_continuation {
            self.admitted_continuation.fetch_add(1, Ordering::Relaxed);
        }
        self.inflight.fetch_add(1, Ordering::Relaxed);
        EnginePermit {
            permit: Some(permit),
            priority_waiters: self.priority_waiters.clone(),
            inflight: self.inflight.clone(),
        }
    }

    /// Acquire a concurrency slot, waiting in the bounded queue when `fair`.
    pub async fn acquire(&self, tenant: Option<&str>) -> Result<EnginePermit, QueueDeny> {
        self.acquire_with_priority(tenant, false).await
    }

    /// Acquire a concurrency slot with optional continuation priority (Batch 5.3).
    ///
    /// When `priority_continuation` is `true` (an active Coding Agent returning
    /// from a server-recorded `tool_calls` pause lease), `QueueMode::Fair`
    /// wakes the continuation waiter ahead of queued cold requests, grants 25%
    /// burst headroom on both global queue depth and per-tenant waiter share
    /// (at least +1 slot), and doubles `max_wait`, preventing multi-turn agent
    /// workflows with high attained GPU service (PLAS) from being starved or
    /// dropped at the cold-request watermark.
    pub async fn acquire_with_priority(
        &self,
        tenant: Option<&str>,
        priority_continuation: bool,
    ) -> Result<EnginePermit, QueueDeny> {
        self.acquire_with_turn(tenant, priority_continuation, 1)
            .await
    }

    /// Acquire a concurrency slot with Turn-Priority fairness scoring
    /// (`score = turn_number + time_weight * wait_secs`).
    pub async fn acquire_with_turn(
        &self,
        tenant: Option<&str>,
        priority_continuation: bool,
        turn_number: u64,
    ) -> Result<EnginePermit, QueueDeny> {
        // Fast path: a slot is free *and* this replica is under its share of the
        // cluster-wide engine cap (N Gateways must not multiply it).
        if !self.over_cluster_cap() {
            if let Ok(permit) = self.sem.clone().try_acquire_owned() {
                return Ok(self.make_permit(permit, priority_continuation));
            }
        }

        // Busy: fast-fail, or `max_wait=0`, reject immediately.
        if self.cfg.mode == QueueMode::FastFail || self.cfg.max_wait.is_zero() {
            self.denied_full.fetch_add(1, Ordering::Relaxed);
            return Err(QueueDeny::QueueFull);
        }

        let key = tenant.unwrap_or(DEFAULT_TENANT).to_string();

        // Bounded enqueue with per-tenant cap; continuation requests get 25%
        // burst headroom (+1 min) so returning tool calls are not starved by
        // cold arrivals.
        let _waiting_guard = {
            let mut st = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
            let effective_max_size = if priority_continuation {
                self.cfg
                    .max_size
                    .saturating_add((self.cfg.max_size / 4).max(1))
            } else {
                self.cfg.max_size
            };
            if st.total >= effective_max_size {
                self.denied_full.fetch_add(1, Ordering::Relaxed);
                return Err(QueueDeny::QueueFull);
            }
            let base_cap = ((self.cfg.max_size as f64) * self.cfg.tenant_share).ceil() as usize;
            let base_cap = base_cap.max(1);
            let effective_cap = if priority_continuation {
                base_cap.saturating_add((base_cap / 4).max(1))
            } else {
                base_cap
            };
            let cur = st.per_tenant.get(&key).copied().unwrap_or(0);
            if cur >= effective_cap {
                self.denied_tenant.fetch_add(1, Ordering::Relaxed);
                return Err(QueueDeny::TenantQueueFull);
            }
            st.total += 1;
            *st.per_tenant.entry(key.clone()).or_insert(0) += 1;
            WaitingGuard {
                queue: self,
                key: key.clone(),
            }
        };
        self.queued.fetch_add(1, Ordering::Relaxed);

        let effective_wait = if priority_continuation {
            self.cfg.max_wait.saturating_mul(2)
        } else {
            self.cfg.max_wait
        };

        if priority_continuation {
            let (tx, rx) = oneshot::channel();
            let waiter_id = {
                let mut q = self
                    .priority_waiters
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if let Ok(permit) = self.sem.clone().try_acquire_owned() {
                    return Ok(self.make_permit(permit, true));
                }
                let id = q.next_waiter_id;
                q.next_waiter_id = q.next_waiter_id.wrapping_add(1);
                q.waiters.push_back(PriorityWaiter {
                    id,
                    turn_number: turn_number.max(1),
                    enqueued_at: Instant::now(),
                    tx,
                });
                id
            };

            struct PriorityCancelGuard<'a> {
                queue: &'a EngineQueue,
                waiter_id: u64,
                rx: Option<oneshot::Receiver<OwnedSemaphorePermit>>,
            }
            impl Drop for PriorityCancelGuard<'_> {
                fn drop(&mut self) {
                    if let Some(mut rx) = self.rx.take() {
                        {
                            let mut q = self
                                .queue
                                .priority_waiters
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            q.waiters.retain(|w| w.id != self.waiter_id);
                        }
                        if let Ok(permit) = rx.try_recv() {
                            // Re-wrap and drop without incrementing `admitted*` so
                            // EnginePermit::drop hands the slot to the next priority
                            // waiter (or back to `self.sem`).
                            self.queue.inflight.fetch_add(1, Ordering::Relaxed);
                            drop(EnginePermit {
                                permit: Some(permit),
                                priority_waiters: self.queue.priority_waiters.clone(),
                                inflight: self.queue.inflight.clone(),
                            });
                        }
                    }
                }
            }

            let mut cancel_guard = PriorityCancelGuard {
                queue: self,
                waiter_id,
                rx: Some(rx),
            };
            let recv_res = tokio::time::timeout(
                effective_wait,
                cancel_guard.rx.as_mut().expect("rx present"),
            )
            .await;

            match recv_res {
                Ok(Ok(permit)) => {
                    cancel_guard.rx = None;
                    if self.over_cluster_cap() {
                        // Returning the permit hands the slot to the next waiter.
                        self.denied_full.fetch_add(1, Ordering::Relaxed);
                        return Err(QueueDeny::QueueFull);
                    }
                    Ok(self.make_permit(permit, true))
                }
                Ok(Err(_)) => {
                    cancel_guard.rx = None;
                    Err(QueueDeny::Closed)
                }
                Err(_) => {
                    let mut rx = cancel_guard.rx.take().expect("rx present");
                    {
                        let mut q = self
                            .priority_waiters
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        q.waiters.retain(|w| w.id != waiter_id);
                    }
                    if let Ok(permit) = rx.try_recv() {
                        if self.over_cluster_cap() {
                            self.denied_full.fetch_add(1, Ordering::Relaxed);
                            return Err(QueueDeny::QueueFull);
                        }
                        return Ok(self.make_permit(permit, true));
                    }
                    self.denied_timeout.fetch_add(1, Ordering::Relaxed);
                    Err(QueueDeny::Timeout)
                }
            }
        } else {
            struct ColdWaiterGuard<'a>(&'a PriorityWaiterQueue);
            impl Drop for ColdWaiterGuard<'_> {
                fn drop(&mut self) {
                    let mut q = self.0.lock().unwrap_or_else(|e| e.into_inner());
                    q.cold_waiters = q.cold_waiters.saturating_sub(1);
                }
            }
            {
                let mut q = self
                    .priority_waiters
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                q.cold_waiters += 1;
            }
            let _cold_guard = ColdWaiterGuard(&self.priority_waiters);
            let waited =
                tokio::time::timeout(effective_wait, self.sem.clone().acquire_owned()).await;
            match waited {
                Ok(Ok(permit)) => {
                    if self.over_cluster_cap() {
                        // Slot handed over while this replica is at its cluster
                        // share: give it back rather than exceed the engine cap.
                        self.denied_full.fetch_add(1, Ordering::Relaxed);
                        return Err(QueueDeny::QueueFull);
                    }
                    Ok(self.make_permit(permit, false))
                }
                Ok(Err(_)) => Err(QueueDeny::Closed),
                Err(_) => {
                    self.denied_timeout.fetch_add(1, Ordering::Relaxed);
                    Err(QueueDeny::Timeout)
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct TurnEntry {
    turns: u64,
    last_seen: Instant,
}

struct TurnTracker {
    inactivity: Duration,
    entries: HashMap<String, TurnEntry>,
}

impl TurnTracker {
    fn new(inactivity: Duration) -> Self {
        Self {
            inactivity,
            entries: HashMap::new(),
        }
    }

    fn record(&mut self, key: &str, now: Instant) -> u64 {
        if self.entries.len() >= MAX_TURN_TRACKER_ENTRIES && !self.entries.contains_key(key) {
            self.entries
                .retain(|_, v| now.saturating_duration_since(v.last_seen) < self.inactivity);
            if self.entries.len() >= MAX_TURN_TRACKER_ENTRIES {
                if let Some(oldest_k) = self
                    .entries
                    .iter()
                    .min_by_key(|(_, v)| v.last_seen)
                    .map(|(k, _)| k.clone())
                {
                    self.entries.remove(&oldest_k);
                }
            }
        }
        let entry = self.entries.entry(key.to_string()).or_insert(TurnEntry {
            turns: 0,
            last_seen: now,
        });
        if entry.turns > 0 && now.saturating_duration_since(entry.last_seen) >= self.inactivity {
            entry.turns = 1;
        } else {
            entry.turns = entry.turns.saturating_add(1);
        }
        entry.last_seen = now;
        entry.turns
    }
}

/// Lazily-created per-model queues sharing one config.
pub struct EngineQueues {
    cfg: QueueConfig,
    divisor: Arc<AtomicU32>,
    queues: Mutex<HashMap<String, Arc<EngineQueue>>>,
    turn_tracker: Mutex<TurnTracker>,
}

impl EngineQueues {
    pub fn new(cfg: QueueConfig) -> Arc<Self> {
        let inactivity_secs = std::env::var("NEBULA_TURN_PRIORITY_INACTIVITY_SECS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_TURN_PRIORITY_INACTIVITY_SECS);
        Arc::new(Self {
            cfg,
            divisor: Arc::new(AtomicU32::new(1)),
            queues: Mutex::new(HashMap::new()),
            turn_tracker: Mutex::new(TurnTracker::new(Duration::from_secs(inactivity_secs))),
        })
    }

    /// Record and return the current conversation turn depth for `(tenant, session_id)`.
    /// Resets to `1` if the session has been idle longer than `turn_priority_inactivity`.
    pub fn record_session_turn(&self, tenant: Option<&str>, session_id: Option<&str>) -> u64 {
        let Some(sid) = session_id.map(str::trim).filter(|s| !s.is_empty()) else {
            return 1;
        };
        let key = format!("{}:{}", tenant.unwrap_or(DEFAULT_TENANT), sid);
        let mut tracker = self.turn_tracker.lock().unwrap_or_else(|e| e.into_inner());
        tracker.record(&key, Instant::now())
    }

    pub fn config(&self) -> &QueueConfig {
        &self.cfg
    }

    pub fn for_model(&self, model: &str) -> Arc<EngineQueue> {
        let mut map = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        let divisor = self.divisor.clone();
        map.entry(model.to_string())
            .or_insert_with(|| EngineQueue::new_with_divisor(self.cfg.clone(), divisor))
            .clone()
    }

    /// Follow the live Gateway member count: the engine cap is shared, so N
    /// replicas must not each admit the engine's full sweet spot (issue #68).
    pub fn set_members(&self, members: u32) {
        self.divisor.store(members.max(1), Ordering::Relaxed);
    }

    pub fn members(&self) -> u32 {
        self.divisor.load(Ordering::Relaxed).max(1)
    }

    pub fn snapshot(&self) -> Vec<(String, Arc<EngineQueue>)> {
        let map = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: QueueMode, conc: usize, max_size: usize, wait_ms: u64) -> QueueConfig {
        QueueConfig {
            mode,
            max_concurrency: conc,
            max_size,
            max_wait: Duration::from_millis(wait_ms),
            tenant_share: 0.5,
        }
    }

    #[tokio::test]
    async fn admits_within_limit() {
        let q = EngineQueue::new(cfg(QueueMode::Fair, 2, 10, 100));
        let a = q.acquire(None).await.expect("a");
        let b = q.acquire(None).await.expect("b");
        assert_eq!(q.inflight(), 2);
        drop(a);
        assert_eq!(q.inflight(), 1);
        drop(b);
        assert_eq!(q.inflight(), 0);
    }

    #[tokio::test]
    async fn fast_fail_rejects_when_busy() {
        let q = EngineQueue::new(cfg(QueueMode::FastFail, 1, 10, 1000));
        let _held = q.acquire(None).await.expect("held");
        let r = q.acquire(None).await;
        assert_eq!(r.unwrap_err(), QueueDeny::QueueFull);
    }

    /// Delivery-boundary semantics (assessment A3): fairness = per-tenant
    /// WAITER SHARE, not round-robin. A hot tenant can saturate its share
    /// (tenant_queue_full beyond it) while other tenants still queue.
    #[tokio::test]
    async fn tenant_share_caps_hot_tenant_but_not_others() {
        // concurrency 1, max_size 10, share 0.5 → per-tenant waiters ≤ 5.
        let q = EngineQueue::new(cfg(QueueMode::Fair, 1, 10, 5000));
        let _held = q.acquire(Some("hot")).await.expect("held");

        let hot: Vec<_> = (0..5)
            .map(|_| {
                let q2 = q.clone();
                tokio::spawn(async move { q2.acquire(Some("hot")).await })
            })
            .collect();
        tokio::time::sleep(Duration::from_millis(30)).await;

        // 6th hot waiter exceeds the 50% share of max_size=10.
        let over = q.acquire(Some("hot")).await;
        assert_eq!(over.unwrap_err(), QueueDeny::TenantQueueFull);

        // Another tenant is NOT blocked by the hot tenant's share: it queues
        // (not rejected) and is admitted once permits flow again.
        let q3 = q.clone();
        let other = tokio::spawn(async move { q3.acquire(Some("other")).await });
        tokio::time::sleep(Duration::from_millis(30)).await;
        // Still queued — the rejection path would have resolved it already.
        assert!(!other.is_finished(), "other should be queued, not rejected");

        drop(_held);
        for h in hot {
            let _ = h.await;
        }
        assert!(other.await.unwrap().is_ok());
        let (_, denied_tenant, _) = q.denied_total();
        assert_eq!(denied_tenant, 1);
    }

    #[tokio::test]
    async fn fair_waits_then_admits() {
        let q = EngineQueue::new(cfg(QueueMode::Fair, 1, 10, 1000));
        let held = q.acquire(None).await.expect("held");
        let q2 = q.clone();
        let waiter = tokio::spawn(async move { q2.acquire(None).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(q.depth(), 1);
        drop(held);
        let permit = waiter.await.expect("join").expect("admit");
        assert_eq!(q.inflight(), 1);
        drop(permit);
    }

    #[tokio::test]
    async fn timeout_when_no_release() {
        let q = EngineQueue::new(cfg(QueueMode::Fair, 1, 10, 30));
        let _held = q.acquire(None).await.expect("held");
        let r = q.acquire(None).await;
        assert_eq!(r.unwrap_err(), QueueDeny::Timeout);
    }

    #[tokio::test]
    async fn tenant_share_caps_one_tenant() {
        // max_size=4, tenant_share=0.5 => cap 2 waiting per tenant.
        let q = EngineQueue::new(cfg(QueueMode::Fair, 1, 4, 1000));
        let _held = q.acquire(None).await.expect("held");
        let q1 = q.clone();
        let h1 = tokio::spawn(async move { q1.acquire(Some("t1")).await });
        let q2 = q.clone();
        let h2 = tokio::spawn(async move { q2.acquire(Some("t1")).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(q.depth(), 2);
        // Third t1 exceeds its share.
        let r = q.acquire(Some("t1")).await;
        assert_eq!(r.unwrap_err(), QueueDeny::TenantQueueFull);
        // Another tenant can still enqueue.
        let q3 = q.clone();
        let h3 = tokio::spawn(async move { q3.acquire(Some("t2")).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(q.depth(), 3);
        // Cancelled waiters release their queue slots.
        h1.abort();
        h2.abort();
        h3.abort();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(q.depth(), 0);
    }

    #[tokio::test]
    async fn priority_continuation_grants_headroom_beyond_cold_watermark() {
        // max_size=4, tenant_share=0.5 => cold tenant cap = 2, continuation cap = 3.
        let q = EngineQueue::new(cfg(QueueMode::Fair, 1, 4, 1000));
        let held = q.acquire(Some("agent")).await.expect("held");
        let q1 = q.clone();
        let h1 = tokio::spawn(async move { q1.acquire(Some("agent")).await });
        let q2 = q.clone();
        let h2 = tokio::spawn(async move { q2.acquire(Some("agent")).await });
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Cold 3rd request from "agent" is rejected at the base tenant cap (2).
        let cold = q.acquire_with_priority(Some("agent"), false).await;
        assert_eq!(cold.unwrap_err(), QueueDeny::TenantQueueFull);

        // Active Tool-Pause continuation request from "agent" gets 25% headroom
        // (+1 slot) and queues instead of failing with 429.
        let q3 = q.clone();
        let cont = tokio::spawn(async move { q3.acquire_with_priority(Some("agent"), true).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!cont.is_finished(), "continuation request must be queued");

        drop(held);
        // Priority continuation waiter wakes up AHEAD of earlier cold waiters h1/h2.
        let p_cont = cont.await.unwrap().unwrap();
        assert!(
            !h1.is_finished(),
            "cold waiter h1 should still wait while continuation runs"
        );
        drop(p_cont);
        let p1 = h1.await.unwrap().unwrap();
        drop(p1);
        let p2 = h2.await.unwrap().unwrap();
        drop(p2);
        assert_eq!(q.admitted_continuation_total(), 1);
    }

    #[tokio::test]
    async fn priority_continuation_handoffs_are_capped_to_prevent_cold_starvation() {
        let q = EngineQueue::new(cfg(QueueMode::Fair, 1, 20, 2000));
        let mut cur = q.acquire(Some("cold")).await.expect("initial");

        // Enqueue 1 cold waiter first, followed by 5 priority continuation waiters.
        let q_cold = q.clone();
        let cold_waiter = tokio::spawn(async move { q_cold.acquire(Some("cold")).await });
        tokio::time::sleep(Duration::from_millis(15)).await;

        let mut prio_waiters = Vec::new();
        for _ in 0..5 {
            let qp = q.clone();
            prio_waiters.push(tokio::spawn(async move {
                qp.acquire_with_priority(Some("prio"), true).await
            }));
        }
        tokio::time::sleep(Duration::from_millis(15)).await;

        // First MAX_CONSECUTIVE_PRIORITY_HANDOFFS (4) releases wake the first 4 priority waiters.
        for pw in prio_waiters.drain(..MAX_CONSECUTIVE_PRIORITY_HANDOFFS) {
            drop(cur);
            cur = pw.await.unwrap().expect("priority waiter admitted");
            assert!(
                !cold_waiter.is_finished(),
                "cold waiter still waiting during first 4 priority handoffs"
            );
        }

        // On the 5th release, the cap yields back to `self.sem`, waking `cold_waiter`
        // ahead of the 5th priority waiter.
        drop(cur);
        let p_cold = cold_waiter
            .await
            .unwrap()
            .expect("cold waiter must wake after cap");
        assert!(
            !prio_waiters[0].is_finished(),
            "5th priority waiter yields to cold waiter on 5th release"
        );
        drop(p_cold);
        let p_last = prio_waiters.pop().unwrap().await.unwrap().unwrap();
        drop(p_last);
    }

    #[tokio::test]
    async fn turn_priority_wakes_deeper_conversation_first() {
        let q = EngineQueue::new(cfg(QueueMode::Fair, 1, 10, 500));
        let cur = q.acquire_with_turn(Some("t"), false, 1).await.unwrap();

        let q1 = q.clone();
        let shallow = tokio::spawn(async move { q1.acquire_with_turn(Some("t"), true, 2).await });
        tokio::time::sleep(Duration::from_millis(10)).await;

        let q2 = q.clone();
        let deep = tokio::spawn(async move { q2.acquire_with_turn(Some("t"), true, 12).await });
        tokio::time::sleep(Duration::from_millis(10)).await;

        drop(cur);
        // Deep session (turn 12) must wake ahead of shallow session (turn 2)
        let p_deep = deep.await.unwrap().expect("deep waiter woken first");
        assert!(
            !shallow.is_finished(),
            "shallow waiter should still be queued while deep waiter holds slot"
        );
        drop(p_deep);
        let p_shallow = shallow.await.unwrap().expect("shallow waiter wakes next");
        drop(p_shallow);
    }

    #[test]
    fn turn_tracker_resets_after_inactivity() {
        let mut tracker = TurnTracker::new(Duration::from_secs(120));
        let t0 = Instant::now();
        assert_eq!(tracker.record("t:s1", t0), 1);
        assert_eq!(tracker.record("t:s1", t0 + Duration::from_secs(10)), 2);
        assert_eq!(tracker.record("t:s1", t0 + Duration::from_secs(20)), 3);
        // Idle >= 120s resets turn depth back to 1
        assert_eq!(tracker.record("t:s1", t0 + Duration::from_secs(141)), 1);
    }
}
