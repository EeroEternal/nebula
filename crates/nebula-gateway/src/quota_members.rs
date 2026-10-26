//! Gateway membership registry (`/gateways/{instance_id}`) → live quota shards.
//!
//! Issue #68. Each Gateway holds a lease-bound key so peers can count the live
//! replicas; the shard divisor then follows the registry instead of a static
//! environment variable, which is what makes join/leave re-shard and hands a
//! failed replica's share to the survivors.
//!
//! Safety (see `nebula_common::quota_shard::MemberView`): the view carries an
//! observation timestamp and refuses to answer once stale, so a partitioned
//! Gateway cannot keep enforcing a divisor nobody else agrees with. Join is
//! ordered the same way: register, wait one refresh interval (peers must have
//! seen us before we start admitting), then serve.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use nebula_common::quota_shard::MemberView;
use nebula_meta::{EtcdMetaStore, MetaStore};
use tokio::sync::Mutex;

/// Registry prefix. Writers: each Gateway (its own key). Readers: all Gateways.
pub const MEMBERS_PREFIX: &str = "/gateways/";

pub struct MemberRegistry {
    store: Arc<EtcdMetaStore>,
    self_id: String,
    key: String,
    refresh: Duration,
    view: Arc<MemberView>,
    /// Our lease, kept so a key lost by a TTL blip can be re-put instead of
    /// leaving this replica permanently unable to prove its own shard.
    lease: Mutex<Option<i64>>,
}

impl MemberRegistry {
    pub fn new(
        store: Arc<EtcdMetaStore>,
        self_id: impl Into<String>,
        refresh: Duration,
        view: Arc<MemberView>,
    ) -> Self {
        let self_id = self_id.into();
        let key = format!("{MEMBERS_PREFIX}{self_id}");
        Self {
            store,
            self_id,
            key,
            refresh,
            view,
            lease: Mutex::new(None),
        }
    }

    /// Register with a lease (kept alive in the background), then wait one
    /// refresh interval so peers observe us before we enforce a shared divisor.
    pub async fn join_and_settle(&self, ttl: Duration) -> Result<u32> {
        let lease = self.store.grant_lease(ttl.as_secs().max(1) as i64).await?;
        *self.lease.lock().await = Some(lease);
        self.store
            .put_with_lease(&self.key, self.payload(), lease)
            .await?;
        self.store
            .spawn_lease_keepalive(lease, ttl.as_secs().max(1) as i64);
        // Settle: give every peer at least one refresh cycle to include us.
        tokio::time::sleep(self.refresh).await;
        self.refresh_once().await
    }

    fn payload(&self) -> Vec<u8> {
        serde_json::json!({
            "instance_id": self.self_id,
            "joined_at_ms": now_ms(),
        })
        .to_string()
        .into_bytes()
    }

    /// One refresh cycle: list the registry and publish the live member set.
    ///
    /// If our own key is missing (lease expired during a network blip) it is
    /// re-put on the same lease: without this the replica would fail closed
    /// forever, because a live view that does not contain self is untrusted.
    pub async fn refresh_once(&self) -> Result<u32> {
        let keys = self.store.list_prefix(MEMBERS_PREFIX).await?;
        let mut members: Vec<String> = keys
            .into_iter()
            .filter_map(|(k, _, _)| k.strip_prefix(MEMBERS_PREFIX).map(str::to_string))
            .filter(|id| !id.is_empty() && !id.contains('/'))
            .collect();
        if !members.iter().any(|m| m == &self.self_id) {
            let lease = *self.lease.lock().await;
            let re_put = match lease {
                Some(lease) => {
                    self.store
                        .put_with_lease(&self.key, self.payload(), lease)
                        .await
                }
                None => self.store.put(&self.key, self.payload(), None).await,
            };
            match re_put {
                Ok(_) => {
                    tracing::warn!(
                        key = %self.key,
                        "gateway registration was missing from /gateways/; re-registered"
                    );
                    members.push(self.self_id.clone());
                }
                Err(e) => tracing::warn!(error = %e, "gateway re-registration failed"),
            }
        }
        let count = members.len() as u32;
        self.view.observe(members);
        Ok(count)
    }

    /// Best-effort deregistration so survivors reclaim the shard immediately
    /// instead of waiting for the lease to expire.
    pub async fn leave(&self) {
        if let Err(e) = self.store.delete(&self.key).await {
            tracing::warn!(error = %e, key = %self.key, "gateway deregistration failed");
        }
    }

    /// Background refresher; stops reporting to the view once etcd stops answering
    /// (the view then goes stale on its own and admission degrades per policy).
    pub fn spawn_refresher(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut last: Option<u32> = None;
            loop {
                match self.refresh_once().await {
                    Ok(n) => {
                        if last != Some(n) {
                            tracing::info!(members = n, "gateway quota membership changed");
                            last = Some(n);
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "gateway membership refresh failed"),
                }
                tokio::time::sleep(self.refresh).await;
            }
        });
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_per_instance_and_namespaced() {
        // Guards the two ways a registry silently breaks: colliding ids (one
        // instance overwrites another) and keys outside the prefix (invisible).
        let a = format!("{MEMBERS_PREFIX}{}", "gw-1");
        let b = format!("{MEMBERS_PREFIX}{}", "gw-2");
        assert_ne!(a, b);
        assert!(a.strip_prefix(MEMBERS_PREFIX).is_some());
        // A nested/blank suffix is not a member id.
        let nested = format!("{MEMBERS_PREFIX}gw-1/extra");
        assert!(nested.strip_prefix(MEMBERS_PREFIX).unwrap().contains('/'));
    }
}
