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
        self.compare_and_swap_guarded(key, expected_revision, value, &[])
            .await
    }

    async fn compare_and_swap_guarded(
        &self,
        key: &str,
        expected_revision: u64,
        value: Vec<u8>,
        guards: &[(String, u64)],
    ) -> Result<(bool, u64)> {
        let (ok, rev, event) = {
            let mut inner = self.inner.write().await;
            let current_rev = inner.kv.get(key).map(|(_, rev)| *rev).unwrap_or(0);
            if current_rev != expected_revision
                || guards
                    .iter()
                    .any(|(key, rev)| inner.kv.get(key).map_or(0, |(_, current)| *current) != *rev)
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn put_get_delete_and_prefix_snapshot_boundaries() {
        let store = MemoryMetaStore::new();
        assert!(store.get("/a/1").await.unwrap().is_none());

        let r1 = store
            .put("/models/m1/spec", b"v1".to_vec(), None)
            .await
            .unwrap();
        let r2 = store
            .put("/models/m2/spec", b"v2".to_vec(), None)
            .await
            .unwrap();
        let _r3 = store
            .put("/models_extra/x", b"other".to_vec(), None)
            .await
            .unwrap();
        assert!(r2 > r1);

        let (v, rev) = store.get("/models/m1/spec").await.unwrap().unwrap();
        assert_eq!(v, b"v1");
        assert_eq!(rev, r1);

        let (snap, snap_rev) = store.list_prefix_snapshot("/models/").await.unwrap();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].0, "/models/m1/spec");
        assert_eq!(snap[1].0, "/models/m2/spec");
        assert!(snap_rev >= r2);

        store.delete("/models/m1/spec").await.unwrap();
        assert!(store.get("/models/m1/spec").await.unwrap().is_none());
        let listed = store.list_prefix("/models/").await.unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn compare_and_swap_and_guarded_atomicity() {
        let store = MemoryMetaStore::new();

        // Create succeeds only with expected_revision == 0.
        let (ok, rev1) = store
            .compare_and_swap("/placements/m1", 0, b"plan1".to_vec())
            .await
            .unwrap();
        assert!(ok);
        assert_eq!(rev1, 1);

        // Stale revision 0 fails and returns current revision.
        let (ok_dup, cur) = store
            .compare_and_swap("/placements/m1", 0, b"plan_stale".to_vec())
            .await
            .unwrap();
        assert!(!ok_dup);
        assert_eq!(cur, rev1);

        // Guarded CAS fails if any guard key's revision mismatches, leaving target untouched.
        let (_, guard_rev) = store
            .compare_and_swap("/models/m1/spec", 0, b"spec1".to_vec())
            .await
            .unwrap();
        let (ok_bad_guard, _) = store
            .compare_and_swap_guarded(
                "/placements/m1",
                rev1,
                b"plan2".to_vec(),
                &[("/models/m1/spec".to_string(), guard_rev + 99)],
            )
            .await
            .unwrap();
        assert!(!ok_bad_guard);
        assert_eq!(
            store.get("/placements/m1").await.unwrap().unwrap().0,
            b"plan1"
        );

        // Matching guard commits atomically.
        let (ok_good_guard, rev2) = store
            .compare_and_swap_guarded(
                "/placements/m1",
                rev1,
                b"plan2".to_vec(),
                &[("/models/m1/spec".to_string(), guard_rev)],
            )
            .await
            .unwrap();
        assert!(ok_good_guard);
        assert!(rev2 > rev1);
        assert_eq!(
            store.get("/placements/m1").await.unwrap().unwrap().0,
            b"plan2"
        );
    }

    #[tokio::test]
    async fn watch_prefix_filters_prefix_and_revision_and_skips_missing_delete() {
        let store = MemoryMetaStore::new();
        let r1 = store.put("/tenants/t1", b"1".to_vec(), None).await.unwrap();

        // Cutoff is r1 + 1: the next write (revision == r1 + 1) must be filtered
        // by `ev.revision <= min_rev`, while `r1 + 2` is delivered.
        let mut stream = store.watch_prefix("/tenants/", Some(r1 + 1)).await.unwrap();

        // Filtered by revision cutoff (revision == r1 + 1).
        store
            .put("/tenants/filtered", b"old".to_vec(), None)
            .await
            .unwrap();
        // Unrelated prefix and deleting a nonexistent key must not emit.
        store.put("/models/m1", b"x".to_vec(), None).await.unwrap();
        store.delete("/tenants/missing").await.unwrap();
        let r_t2 = store.put("/tenants/t2", b"2".to_vec(), None).await.unwrap();
        store.delete("/tenants/t1").await.unwrap();

        let ev1 = stream.next().await.unwrap();
        assert_eq!(ev1.key, "/tenants/t2");
        assert_eq!(ev1.value.as_deref(), Some(b"2".as_slice()));
        assert_eq!(ev1.revision, r_t2);

        let ev2 = stream.next().await.unwrap();
        assert_eq!(ev2.key, "/tenants/t1");
        assert!(ev2.value.is_none());
    }
}
