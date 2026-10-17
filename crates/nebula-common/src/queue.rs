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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

type PriorityWaiterQueue = Arc<Mutex<VecDeque<oneshot::Sender<OwnedSemaphorePermit>>>>;

/// One engine's (model's) bounded fair queue.
pub struct EngineQueue {
    cfg: QueueConfig,
    sem: Arc<Semaphore>,
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
/// continuation waiter before falling back to the cold FIFO semaphore.
pub struct EnginePermit {
    permit: Option<OwnedSemaphorePermit>,
    priority_waiters: PriorityWaiterQueue,
    inflight: Arc<AtomicUsize>,
}

impl Drop for EnginePermit {
    fn drop(&mut self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
        if let Some(permit) = self.permit.take() {
            let mut q = self
                .priority_waiters
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let mut handoff = Some(permit);
            while let Some(tx) = q.pop_front() {
                match tx.send(handoff.take().unwrap()) {
                    Ok(()) => break,
                    Err(returned_permit) => {
                        // That priority waiter already timed out or acquired via
                        // semaphore; try the next priority waiter.
                        handoff = Some(returned_permit);
                    }
                }
            }
            // If `handoff` is still `Some`, dropping it here returns the slot to
            // `self.sem` for cold FIFO waiters.
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
        let max = cfg.max_concurrency.max(1);
        Arc::new(Self {
            cfg,
            sem: Arc::new(Semaphore::new(max)),
            priority_waiters: Arc::new(Mutex::new(VecDeque::new())),
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
        // Fast path: a slot is free.
        if let Ok(permit) = self.sem.clone().try_acquire_owned() {
            return Ok(self.make_permit(permit, priority_continuation));
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

        let waited = if priority_continuation {
            let (tx, mut rx) = oneshot::channel();
            {
                let mut q = self
                    .priority_waiters
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                q.push_back(tx);
            }
            let sem = self.sem.clone();
            tokio::time::timeout(effective_wait, async move {
                tokio::select! {
                    biased;
                    Ok(permit) = &mut rx => Ok(permit),
                    res = sem.acquire_owned() => res,
                }
            })
            .await
        } else {
            tokio::time::timeout(effective_wait, self.sem.clone().acquire_owned()).await
        };

        match waited {
            Ok(Ok(permit)) => Ok(self.make_permit(permit, priority_continuation)),
            Ok(Err(_)) => Err(QueueDeny::Closed),
            Err(_) => {
                self.denied_timeout.fetch_add(1, Ordering::Relaxed);
                Err(QueueDeny::Timeout)
            }
        }
    }
}

/// Lazily-created per-model queues sharing one config.
pub struct EngineQueues {
    cfg: QueueConfig,
    queues: Mutex<HashMap<String, Arc<EngineQueue>>>,
}

impl EngineQueues {
    pub fn new(cfg: QueueConfig) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            queues: Mutex::new(HashMap::new()),
        })
    }

    pub fn config(&self) -> &QueueConfig {
        &self.cfg
    }

    pub fn for_model(&self, model: &str) -> Arc<EngineQueue> {
        let mut map = self.queues.lock().unwrap_or_else(|e| e.into_inner());
        map.entry(model.to_string())
            .or_insert_with(|| EngineQueue::new(self.cfg.clone()))
            .clone()
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
}
