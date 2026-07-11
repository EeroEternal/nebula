use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use tokio::sync::{broadcast, RwLock};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::types::{MetaStore, WatchEvent, WatchStream};

#[derive(Debug, Clone)]
pub struct MemoryMetaStore {
    inner: Arc<RwLock<Inner>>,
    tx: broadcast::Sender<WatchEvent>,
}

#[derive(Debug, Default)]
struct Inner {
    revision: u64,
    kv: BTreeMap<String, (Vec<u8>, u64)>,
}

impl MemoryMetaStore {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
            tx,
        }
    }

    fn next_revision(inner: &mut Inner) -> u64 {
        inner.revision = inner.revision.saturating_add(1);
        inner.revision
    }

    fn emit(&self, event: WatchEvent) {
        let _ = self.tx.send(event);
    }
}

impl Default for MemoryMetaStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl MetaStore for MemoryMetaStore {
    async fn put(&self, key: &str, value: Vec<u8>, _ttl_ms: Option<u64>) -> Result<u64> {
        let (rev, event) = {
            let mut inner = self.inner.write().await;
            let rev = Self::next_revision(&mut inner);
            inner.kv.insert(key.to_string(), (value.clone(), rev));
            (
                rev,
                WatchEvent {
                    key: key.to_string(),
                    value: Some(value),
                    revision: rev,
                },
            )
        };
        self.emit(event);
        Ok(rev)
    }

    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, u64)>> {
        let inner = self.inner.read().await;
        Ok(inner.kv.get(key).map(|(v, rev)| (v.clone(), *rev)))
    }

    async fn delete(&self, key: &str) -> Result<u64> {
        let (rev, existed, event) = {
            let mut inner = self.inner.write().await;
            let existed = inner.kv.remove(key).is_some();
            let rev = Self::next_revision(&mut inner);
            let event = WatchEvent {
                key: key.to_string(),
                value: None,
                revision: rev,
            };
            (rev, existed, event)
        };

        if existed {
            self.emit(event);
        }

        Ok(rev)
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>, u64)>> {
        let inner = self.inner.read().await;
        let mut out = Vec::new();
        for (k, (v, rev)) in inner
            .kv
            .range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
        {
            out.push((k.clone(), v.clone(), *rev));
        }
        Ok(out)
    }

    async fn list_prefix_snapshot(
        &self,
        prefix: &str,
    ) -> Result<(Vec<(String, Vec<u8>, u64)>, u64)> {
        let inner = self.inner.read().await;
        let snap_rev = inner.revision;
        let mut out = Vec::new();
        for (k, (v, rev)) in inner
            .kv
            .range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
        {
            out.push((k.clone(), v.clone(), *rev));
        }
        Ok((out, snap_rev))
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected_revision: u64,
        value: Vec<u8>,
    ) -> Result<(bool, u64)> {
        let (ok, rev, event) = {
            let mut inner = self.inner.write().await;
            let current_rev = inner.kv.get(key).map(|(_, rev)| *rev).unwrap_or(0);
            if current_rev != expected_revision {
                return Ok((false, current_rev));
            }
            let rev = Self::next_revision(&mut inner);
            inner.kv.insert(key.to_string(), (value.clone(), rev));
            let event = WatchEvent {
                key: key.to_string(),
                value: Some(value),
                revision: rev,
            };
            (true, rev, event)
        };

        if ok {
            self.emit(event);
        }

        Ok((ok, rev))
    }

    async fn watch_prefix(
        &self,
        prefix: &str,
        start_revision_exclusive: Option<u64>,
    ) -> Result<WatchStream> {
        let prefix = prefix.to_string();
        let min_rev = start_revision_exclusive.unwrap_or(0);
        let rx = self.tx.subscribe();
        let stream = BroadcastStream::new(rx).filter_map(move |msg| match msg {
            Ok(ev) => {
                if ev.revision <= min_rev {
                    return None;
                }
                if ev.key.starts_with(&prefix) {
                    Some(ev)
                } else {
                    None
                }
            }
            Err(_) => None,
        });

        Ok(Box::pin(stream))
    }
}
