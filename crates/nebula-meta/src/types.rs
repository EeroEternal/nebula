use std::pin::Pin;

use anyhow::Result;
use async_trait::async_trait;
use futures_core::Stream;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchEvent {
    pub key: String,
    pub value: Option<Vec<u8>>,
    pub revision: u64,
}

pub type WatchStream = Pin<Box<dyn Stream<Item = WatchEvent> + Send>>;

#[async_trait]
pub trait MetaStore: Send + Sync {
    async fn put(&self, key: &str, value: Vec<u8>, ttl_ms: Option<u64>) -> Result<u64>;
    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, u64)>>;
    async fn delete(&self, key: &str) -> Result<u64>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>, u64)>>;

    /// List prefix and return the store's snapshot revision for starting a watch.
    /// Default: max of per-key mod_revisions (may miss header-only advances).
    async fn list_prefix_snapshot(
        &self,
        prefix: &str,
    ) -> Result<(Vec<(String, Vec<u8>, u64)>, u64)> {
        let items = self.list_prefix(prefix).await?;
        let snap_rev = items.iter().map(|(_, _, rev)| *rev).max().unwrap_or(0);
        Ok((items, snap_rev))
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected_revision: u64,
        value: Vec<u8>,
    ) -> Result<(bool, u64)>;

    async fn watch_prefix(
        &self,
        prefix: &str,
        start_revision_exclusive: Option<u64>,
    ) -> Result<WatchStream>;
}
