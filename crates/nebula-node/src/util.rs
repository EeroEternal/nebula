use std::time::{SystemTime, UNIX_EPOCH};

use nebula_meta::{EtcdMetaStore, MetaStore};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Put a node-ephemeral key on the shared lease (or per-put TTL fallback).
pub async fn put_node_ephemeral(
    store: &EtcdMetaStore,
    key: &str,
    bytes: Vec<u8>,
    ttl_ms: u64,
    lease_id: Option<i64>,
) -> anyhow::Result<u64> {
    if let Some(id) = lease_id {
        store.put_with_lease(key, bytes, id).await
    } else {
        store.put(key, bytes, Some(ttl_ms)).await
    }
}
