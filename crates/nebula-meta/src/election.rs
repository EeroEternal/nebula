//! Lease-based leader election over MetaStore.
//!
//! Leader record is stored at a well-known key with an expiry timestamp. The
//! holder renews before expiry; a new holder increments `epoch` (fencing token)
//! when leadership changes. Works with both EtcdMetaStore (TTL put as crash
//! safety) and MemoryMetaStore (expiry checked in-process).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::types::MetaStore;

/// Default etcd/memory key for scheduler leadership.
pub const SCHEDULER_ELECTION_KEY: &str = "/nebula/election/scheduler";

#[derive(Clone, Debug, Default)]
pub struct LeaderGate {
    is_leader: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
}

impl LeaderGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_leader(&self) -> bool {
        self.is_leader.load(Ordering::Acquire)
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> (bool, u64) {
        (
            self.is_leader.load(Ordering::Acquire),
            self.epoch.load(Ordering::Acquire),
        )
    }

    fn become_leader(&self, epoch: u64) {
        self.epoch.store(epoch, Ordering::Release);
        self.is_leader.store(true, Ordering::Release);
    }

    fn become_follower(&self) {
        self.is_leader.store(false, Ordering::Release);
    }

    /// Test / bootstrap helper: force leadership without running the election loop.
    pub fn force_leader(&self, epoch: u64) {
        self.become_leader(epoch);
    }
}

#[derive(Debug, Clone)]
pub struct ElectionConfig {
    pub key: String,
    pub holder_id: String,
    pub ttl_ms: u64,
    pub renew_interval: Duration,
}

impl ElectionConfig {
    pub fn scheduler(holder_id: impl Into<String>) -> Self {
        Self {
            key: SCHEDULER_ELECTION_KEY.to_string(),
            holder_id: holder_id.into(),
            ttl_ms: 10_000,
            renew_interval: Duration::from_millis(3_000),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LeaderRecord {
    holder_id: String,
    epoch: u64,
    expires_at_ms: u64,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// One election step: renew if we hold, otherwise try to acquire when expired.
pub async fn election_step<S: MetaStore + ?Sized>(
    store: &S,
    gate: &LeaderGate,
    cfg: &ElectionConfig,
) -> Result<()> {
    let now = now_ms();
    let current = store.get(&cfg.key).await?;

    match current {
        None => try_acquire(store, gate, cfg, 0, None, now).await,
        Some((bytes, rev)) => {
            let record: LeaderRecord = match serde_json::from_slice(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    warn!(error=%e, key=%cfg.key, "corrupt leader record; attempting takeover");
                    return try_acquire(store, gate, cfg, rev, None, now).await;
                }
            };

            if record.holder_id == cfg.holder_id && record.expires_at_ms > now {
                // We hold leadership — renew.
                let renewed = LeaderRecord {
                    holder_id: cfg.holder_id.clone(),
                    epoch: record.epoch,
                    expires_at_ms: now.saturating_add(cfg.ttl_ms),
                };
                let val = serde_json::to_vec(&renewed)?;
                let (ok, _) = store
                    .compare_and_swap(&cfg.key, rev, val)
                    .await?;
                if ok {
                    if !gate.is_leader() || gate.epoch() != record.epoch {
                        info!(
                            holder=%cfg.holder_id,
                            epoch=record.epoch,
                            "acquired / confirmed scheduler leadership"
                        );
                    }
                    gate.become_leader(record.epoch);
                } else {
                    warn!(holder=%cfg.holder_id, "lost leadership during renew (CAS conflict)");
                    gate.become_follower();
                }
                Ok(())
            } else if record.expires_at_ms <= now {
                // Lease expired — take over with epoch+1.
                try_acquire(store, gate, cfg, rev, Some(record.epoch), now).await
            } else {
                // Someone else holds a valid lease.
                if gate.is_leader() {
                    info!(
                        holder=%cfg.holder_id,
                        other=%record.holder_id,
                        "leadership taken by peer; becoming follower"
                    );
                }
                gate.become_follower();
                // Keep observing the latest known epoch so healthz/fencing stay coherent.
                if record.epoch > gate.epoch() {
                    gate.epoch.store(record.epoch, Ordering::Release);
                }
                Ok(())
            }
        }
    }
}

async fn try_acquire<S: MetaStore + ?Sized>(
    store: &S,
    gate: &LeaderGate,
    cfg: &ElectionConfig,
    expected_rev: u64,
    previous_epoch: Option<u64>,
    now: u64,
) -> Result<()> {
    let new_epoch = previous_epoch.unwrap_or(0).saturating_add(1).max(1);
    let record = LeaderRecord {
        holder_id: cfg.holder_id.clone(),
        epoch: new_epoch,
        expires_at_ms: now.saturating_add(cfg.ttl_ms),
    };
    let val = serde_json::to_vec(&record)?;

    // Prefer CAS; also attach TTL on put path for etcd crash safety when creating.
    let (ok, _) = store.compare_and_swap(&cfg.key, expected_rev, val).await?;

    if ok {
        info!(
            holder=%cfg.holder_id,
            epoch=new_epoch,
            "won scheduler leadership"
        );
        gate.become_leader(new_epoch);
    } else {
        gate.become_follower();
    }
    Ok(())
}

/// Run election forever (production / long-lived tasks).
pub async fn run_election_loop<S: MetaStore + ?Sized>(
    store: &S,
    gate: LeaderGate,
    cfg: ElectionConfig,
) {
    info!(
        key=%cfg.key,
        holder=%cfg.holder_id,
        ttl_ms=cfg.ttl_ms,
        "scheduler leader election started"
    );
    loop {
        if let Err(e) = election_step(store, &gate, &cfg).await {
            warn!(error=%e, "election step failed");
            gate.become_follower();
        }
        tokio::time::sleep(cfg.renew_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryMetaStore;

    #[tokio::test]
    async fn single_candidate_becomes_leader() {
        let store = MemoryMetaStore::new();
        let gate = LeaderGate::new();
        let cfg = ElectionConfig {
            key: "/test/election".into(),
            holder_id: "a".into(),
            ttl_ms: 5_000,
            renew_interval: Duration::from_millis(50),
        };

        election_step(&store, &gate, &cfg).await.unwrap();
        assert!(gate.is_leader());
        assert_eq!(gate.epoch(), 1);

        election_step(&store, &gate, &cfg).await.unwrap();
        assert!(gate.is_leader());
        assert_eq!(gate.epoch(), 1);
    }

    #[tokio::test]
    async fn two_candidates_exactly_one_leader() {
        let store = MemoryMetaStore::new();
        let gate_a = LeaderGate::new();
        let gate_b = LeaderGate::new();
        let cfg_a = ElectionConfig {
            key: "/test/election".into(),
            holder_id: "a".into(),
            ttl_ms: 5_000,
            renew_interval: Duration::from_millis(50),
        };
        let cfg_b = ElectionConfig {
            key: "/test/election".into(),
            holder_id: "b".into(),
            ttl_ms: 5_000,
            renew_interval: Duration::from_millis(50),
        };

        election_step(&store, &gate_a, &cfg_a).await.unwrap();
        election_step(&store, &gate_b, &cfg_b).await.unwrap();

        let leaders = [gate_a.is_leader(), gate_b.is_leader()]
            .into_iter()
            .filter(|x| *x)
            .count();
        assert_eq!(leaders, 1);
        assert_eq!(gate_a.epoch().max(gate_b.epoch()), 1);
    }

    #[tokio::test]
    async fn failover_increments_epoch() {
        let store = MemoryMetaStore::new();
        let gate_a = LeaderGate::new();
        let gate_b = LeaderGate::new();
        let mut cfg_a = ElectionConfig {
            key: "/test/election".into(),
            holder_id: "a".into(),
            ttl_ms: 200,
            renew_interval: Duration::from_millis(50),
        };
        let cfg_b = ElectionConfig {
            key: "/test/election".into(),
            holder_id: "b".into(),
            ttl_ms: 5_000,
            renew_interval: Duration::from_millis(50),
        };

        election_step(&store, &gate_a, &cfg_a).await.unwrap();
        assert!(gate_a.is_leader());
        assert_eq!(gate_a.epoch(), 1);

        // Stop renewing A; wait for lease expiry.
        cfg_a.ttl_ms = 1;
        tokio::time::sleep(Duration::from_millis(250)).await;

        election_step(&store, &gate_b, &cfg_b).await.unwrap();
        assert!(gate_b.is_leader());
        assert_eq!(gate_b.epoch(), 2);

        election_step(&store, &gate_a, &cfg_a).await.unwrap();
        assert!(!gate_a.is_leader());
    }
}
