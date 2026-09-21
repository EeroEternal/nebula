//! Engine-level bounded fair-queue admission (gateway egress).
//!
//! Bounds in-flight concurrency per model/engine to the latency sweet spot and
//! absorbs bursts in a **bounded, per-tenant-fair queue** instead of overloading
//! the engine.
//!
//! Deliberately **not adaptive** (no AIMD): queue-induced TTFT must not be read
//! as upstream congestion — that self-locks under burst load. See
//! `docs/dev/queue-admission.md`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

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

/// One engine's (model's) bounded fair queue.
pub struct EngineQueue {
    cfg: QueueConfig,
    sem: Arc<Semaphore>,
    waiting: Mutex<WaitState>,
    inflight: Arc<AtomicUsize>,
    admitted: AtomicUsize,
    queued: AtomicUsize,
    denied_full: AtomicUsize,
    denied_tenant: AtomicUsize,
    denied_timeout: AtomicUsize,
}

/// Held for the request lifetime (including the streamed response). Dropping it
/// releases the concurrency slot.
pub struct EnginePermit {
    _permit: OwnedSemaphorePermit,
    inflight: Arc<AtomicUsize>,
}

impl Drop for EnginePermit {
    fn drop(&mut self) {
        self.inflight.fetch_sub(1, Ordering::Relaxed);
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
            waiting: Mutex::new(WaitState::default()),
            inflight: Arc::new(AtomicUsize::new(0)),
            admitted: AtomicUsize::new(0),
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

    fn make_permit(&self, permit: OwnedSemaphorePermit) -> EnginePermit {
        self.admitted.fetch_add(1, Ordering::Relaxed);
        self.inflight.fetch_add(1, Ordering::Relaxed);
        EnginePermit {
            _permit: permit,
            inflight: self.inflight.clone(),
        }
    }

    /// Acquire a concurrency slot, waiting in the bounded queue when `fair`.
    pub async fn acquire(&self, tenant: Option<&str>) -> Result<EnginePermit, QueueDeny> {
        // Fast path: a slot is free.
        if let Ok(permit) = self.sem.clone().try_acquire_owned() {
            return Ok(self.make_permit(permit));
        }

        // Busy: fast-fail, or `max_wait=0`, reject immediately.
        if self.cfg.mode == QueueMode::FastFail || self.cfg.max_wait.is_zero() {
            self.denied_full.fetch_add(1, Ordering::Relaxed);
            return Err(QueueDeny::QueueFull);
        }

        let key = tenant.unwrap_or(DEFAULT_TENANT).to_string();

        // Bounded enqueue with per-tenant cap; the guard releases the slot on
        // drop (including cancellation while waiting).
        let _waiting_guard = {
            let mut st = self.waiting.lock().unwrap_or_else(|e| e.into_inner());
            if st.total >= self.cfg.max_size {
                self.denied_full.fetch_add(1, Ordering::Relaxed);
                return Err(QueueDeny::QueueFull);
            }
            let cap = ((self.cfg.max_size as f64) * self.cfg.tenant_share).ceil() as usize;
            let cap = cap.max(1);
            let cur = st.per_tenant.get(&key).copied().unwrap_or(0);
            if cur >= cap {
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

        let waited =
            tokio::time::timeout(self.cfg.max_wait, self.sem.clone().acquire_owned()).await;

        match waited {
            Ok(Ok(permit)) => Ok(self.make_permit(permit)),
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
}
