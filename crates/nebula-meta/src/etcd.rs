use std::sync::Arc;

use anyhow::Result;
use etcd_client::{
    Client, Compare, CompareOp, EventType, GetOptions, PutOptions, Txn, TxnOp, WatchOptions,
};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;

use crate::types::{MetaStore, WatchEvent, WatchStream};

#[derive(Clone)]
pub struct EtcdMetaStore {
    client: Arc<Mutex<Client>>,
}

impl EtcdMetaStore {
    pub async fn connect(endpoints: &[String]) -> Result<Self> {
        let c = Client::connect(endpoints, None).await?;
        Ok(Self {
            client: Arc::new(Mutex::new(c)),
        })
    }

    fn ttl_to_seconds(ttl_ms: u64) -> i64 {
        let mut secs = (ttl_ms as f64 / 1000.0).ceil() as i64;
        if secs <= 0 {
            secs = 1;
        }
        secs
    }
}

#[async_trait::async_trait]
impl MetaStore for EtcdMetaStore {
    async fn put(&self, key: &str, value: Vec<u8>, ttl_ms: Option<u64>) -> Result<u64> {
        let mut cli = self.client.lock().await;

        let mut opts = PutOptions::new();
        if let Some(ttl_ms) = ttl_ms {
            let ttl_secs = Self::ttl_to_seconds(ttl_ms);
            let lease = cli.lease_grant(ttl_secs, None).await?;
            opts = opts.with_lease(lease.id());
        }

        let resp = cli.put(key, value, Some(opts)).await?;
        let rev = resp.header().map(|h| h.revision()).unwrap_or_default();
        Ok(rev as u64)
    }

    async fn get(&self, key: &str) -> Result<Option<(Vec<u8>, u64)>> {
        let mut cli = self.client.lock().await;
        let resp = cli.get(key, None).await?;
        let kv = match resp.kvs().first() {
            Some(kv) => kv,
            None => return Ok(None),
        };
        let rev = kv.mod_revision() as u64;
        Ok(Some((kv.value().to_vec(), rev)))
    }

    async fn delete(&self, key: &str) -> Result<u64> {
        let mut cli = self.client.lock().await;
        let resp = cli.delete(key, None).await?;
        let rev = resp.header().map(|h| h.revision()).unwrap_or_default();
        Ok(rev as u64)
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<(String, Vec<u8>, u64)>> {
        let mut cli = self.client.lock().await;
        let opts = GetOptions::new().with_prefix();
        let resp = cli.get(prefix, Some(opts)).await?;

        let mut out = Vec::new();
        for kv in resp.kvs() {
            let k = String::from_utf8_lossy(kv.key()).to_string();
            let v = kv.value().to_vec();
            let rev = kv.mod_revision() as u64;
            out.push((k, v, rev));
        }
        Ok(out)
    }

    async fn list_prefix_snapshot(
        &self,
        prefix: &str,
    ) -> Result<(Vec<(String, Vec<u8>, u64)>, u64)> {
        let mut cli = self.client.lock().await;
        let opts = GetOptions::new().with_prefix();
        let resp = cli.get(prefix, Some(opts)).await?;

        let snap_rev = resp.header().map(|h| h.revision() as u64).unwrap_or(0);
        let mut out = Vec::new();
        for kv in resp.kvs() {
            let k = String::from_utf8_lossy(kv.key()).to_string();
            let v = kv.value().to_vec();
            let rev = kv.mod_revision() as u64;
            out.push((k, v, rev));
        }
        Ok((out, snap_rev))
    }

    async fn compare_and_swap(
        &self,
        key: &str,
        expected_revision: u64,
        value: Vec<u8>,
    ) -> Result<(bool, u64)> {
        let mut cli = self.client.lock().await;

        let cmp = Compare::mod_revision(key, CompareOp::Equal, expected_revision as i64);
        let put = TxnOp::put(key, value, None);
        let txn = Txn::new().when([cmp]).and_then([put]).or_else([]);
        let resp = cli.txn(txn).await?;

        if resp.succeeded() {
            let rev = resp.header().map(|h| h.revision()).unwrap_or_default();
            return Ok((true, rev as u64));
        }

        // failed CAS: return current mod_revision if present, else 0
        let current = cli.get(key, None).await?;
        let current_rev = current
            .kvs()
            .first()
            .map(|kv| kv.mod_revision() as u64)
            .unwrap_or(0);
        Ok((false, current_rev))
    }

    async fn watch_prefix(
        &self,
        prefix: &str,
        start_revision_exclusive: Option<u64>,
    ) -> Result<WatchStream> {
        let mut cli = self.client.lock().await;

        let mut opts = WatchOptions::new().with_prefix();
        if let Some(min_rev) = start_revision_exclusive {
            // etcd watch start_revision is inclusive, so +1 for exclusive semantics
            opts = opts.with_start_revision((min_rev.saturating_add(1)) as i64);
        }

        let (_watcher, mut stream) = cli.watch(prefix, Some(opts)).await?;
        let watch_prefix = prefix.to_string();

        let (tx, rx) = tokio::sync::mpsc::channel::<WatchEvent>(1024);
        tokio::spawn(async move {
            while let Some(item) = stream.message().await.transpose() {
                let resp = match item {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(prefix=%watch_prefix, error=%e, "etcd watch stream error");
                        return;
                    }
                };

                for ev in resp.events() {
                    match ev.event_type() {
                        EventType::Put => {
                            if let Some(kv) = ev.kv() {
                                let key = String::from_utf8_lossy(kv.key()).to_string();
                                let _ = tx
                                    .send(WatchEvent {
                                        key,
                                        value: Some(kv.value().to_vec()),
                                        revision: kv.mod_revision() as u64,
                                    })
                                    .await;
                            }
                        }
                        EventType::Delete => {
                            if let Some(kv) = ev.kv() {
                                let key = String::from_utf8_lossy(kv.key()).to_string();
                                let _ = tx
                                    .send(WatchEvent {
                                        key,
                                        value: None,
                                        revision: kv.mod_revision() as u64,
                                    })
                                    .await;
                            }
                        }
                    }
                }

                if resp.canceled() {
                    let reason = resp.cancel_reason().to_string();
                    tracing::warn!(
                        prefix=%watch_prefix,
                        %reason,
                        "etcd watch canceled (possibly compacted); caller should resync"
                    );
                    return;
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}
