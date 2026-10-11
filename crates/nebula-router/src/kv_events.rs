//! KV cache event ingestion and the prefix→replica index (P1c).
//!
//! vLLM publishes events over ZeroMQ (msgpack, `array_like` + `tag`) as
//! batches `[ts, [event, ...]]`. Events are `["BlockStored", block_hashes,
//! parent, token_ids, block_size, lora_id, medium]`,
//! `["BlockRemoved", block_hashes, medium]` and `["AllBlocksCleared"]`.
//! Block hashes are bytes (`sha256_cbor`) or legacy ints.
//!
//! The index is a rebuildable cache (drop it and you only lose routing
//! hints): `apply` on events, `lookup` votes by longest matched chain depth.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use rmpv::Value;

/// Ingested KV cache event (only the fields routing needs).
#[derive(Debug, Clone, PartialEq)]
pub enum KvEvent {
    Stored {
        block_hashes: Vec<Vec<u8>>,
        /// "GPU" (or None) prefixes are routable; CPU/offload tiers are not
        /// recorded in v1 — routing there would need a tier reload.
        medium: Option<String>,
    },
    Removed {
        block_hashes: Vec<Vec<u8>>,
    },
    Cleared,
}

fn hash_to_bytes(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Binary(b) => Some(b.to_vec()),
        // Legacy int block hashes (pre-sha256_cbor vLLM): 8-byte big-endian.
        Value::Integer(i) => i.as_i64().map(|i| i.to_be_bytes().to_vec()),
        _ => None,
    }
}

fn hashes_of(v: &Value) -> Option<Vec<Vec<u8>>> {
    v.as_array()
        .map(|arr| arr.iter().filter_map(hash_to_bytes).collect())
}

fn parse_event(v: &Value) -> Option<KvEvent> {
    let arr = v.as_array()?;
    match arr.first()?.as_str()? {
        "BlockStored" => Some(KvEvent::Stored {
            block_hashes: hashes_of(arr.get(1)?)?,
            // Field order: tag, block_hashes, parent, token_ids, block_size,
            // lora_id, medium.
            medium: arr.get(6).and_then(|m| m.as_str()).map(str::to_string),
        }),
        "BlockRemoved" => Some(KvEvent::Removed {
            block_hashes: hashes_of(arr.get(1)?)?,
        }),
        "AllBlocksCleared" => Some(KvEvent::Cleared),
        _ => None,
    }
}

/// Parse one msgpack event batch payload (`[ts, [events...]]`).
pub fn parse_batch(payload: &[u8]) -> Vec<KvEvent> {
    let mut rd = payload;
    let Ok(batch) = rmpv::decode::read_value(&mut rd) else {
        return Vec::new();
    };
    let Some(arr) = batch.as_array() else {
        return Vec::new();
    };
    arr.get(1)
        .and_then(Value::as_array)
        .map(|events| events.iter().filter_map(parse_event).collect())
        .unwrap_or_default()
}

/// Parses a publisher topic into `(model_uid, replica_id)`.
/// Engines publish on `"{model_uid}/{replica_id}"` (set via
/// `--kv-events-config` by the node / k8s-controller).
pub fn parse_topic(topic: &[u8]) -> Option<(String, u32)> {
    let s = std::str::from_utf8(topic).ok()?;
    let (model, replica) = s.rsplit_once('/')?;
    let replica = replica.parse().ok()?;
    Some((model.to_string(), replica))
}

/// Rebuildable map of `(model_uid, block_hash) → (replica_id, last_seen_ms)`.
pub struct PrefixIndex {
    map: DashMap<(String, Vec<u8>), (u32, u64)>,
    inserts_total: AtomicU64,
}

impl PrefixIndex {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            map: DashMap::new(),
            inserts_total: AtomicU64::new(0),
        })
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn inserts_total(&self) -> u64 {
        self.inserts_total.load(Ordering::Relaxed)
    }

    pub fn apply(&self, model_uid: &str, replica_id: u32, events: &[KvEvent], now_ms: u64) {
        for ev in events {
            match ev {
                KvEvent::Stored {
                    block_hashes,
                    medium,
                } => {
                    // shortcut: CPU/offload-stored blocks are not routable in
                    // v1; revisit when a tiered reload path exists.
                    if matches!(medium.as_deref(), Some("CPU") | Some("STORAGE")) {
                        continue;
                    }
                    for h in block_hashes {
                        self.map
                            .insert((model_uid.to_string(), h.clone()), (replica_id, now_ms));
                    }
                    self.inserts_total.fetch_add(1, Ordering::Relaxed);
                }
                KvEvent::Removed { block_hashes } => {
                    for h in block_hashes {
                        if let Some(entry) = self.map.get(&(model_uid.to_string(), h.clone())) {
                            if entry.value().0 == replica_id {
                                drop(entry);
                                self.map.remove(&(model_uid.to_string(), h.clone()));
                            }
                        }
                    }
                }
                KvEvent::Cleared => {
                    let keys: Vec<(String, Vec<u8>)> = self
                        .map
                        .iter()
                        .filter(|e| e.key().0 == model_uid && e.value().0 == replica_id)
                        .map(|e| e.key().clone())
                        .collect();
                    for k in keys {
                        self.map.remove(&k);
                    }
                }
            }
        }
    }

    /// Pick the replica holding the longest matched prefix chain.
    ///
    /// Chaining makes any matched hash proof of exact token-level prefix
    /// identity up to that block, so depth beats count; ties break to the
    /// lowest replica_id for determinism.
    pub fn lookup(&self, model_uid: &str, hashes: &[[u8; 32]]) -> Option<u32> {
        let mut best: Option<(usize, usize, u32)> = None; // (depth, count, replica)
        for (depth, h) in hashes.iter().enumerate() {
            if let Some(entry) = self.map.get(&(model_uid.to_string(), h.to_vec())) {
                let replica = entry.value().0;
                let cur = best.unwrap_or((0, 0, u32::MAX));
                let next = if cur.2 == replica {
                    (depth.max(cur.0), cur.1 + 1, replica)
                } else {
                    let better = depth > cur.0 || (depth == cur.0 && 1 > cur.1);
                    if cur.2 == u32::MAX || better {
                        (depth, 1, replica)
                    } else {
                        cur
                    }
                };
                best = Some(next);
            }
        }
        best.filter(|b| b.2 != u32::MAX).map(|b| b.2)
    }

    /// Drop entries not refreshed within the TTL (KV blocks are LRU-evicted
    /// inside the engine; the index must not outlive them).
    pub fn evict_expired(&self, now_ms: u64, ttl_ms: u64) -> usize {
        let cutoff = now_ms.saturating_sub(ttl_ms);
        let keys: Vec<(String, Vec<u8>)> = self
            .map
            .iter()
            .filter(|e| e.value().1 < cutoff)
            .map(|e| e.key().clone())
            .collect();
        let n = keys.len();
        for k in keys {
            self.map.remove(&k);
        }
        n
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// ZeroMQ SUB ingestion loop: engines publish on topic `"{model_uid}/{replica}"`
/// (set via `--kv-events-config` by the node / k8s-controller); each message
/// is `(topic, seq, msgpack batch)`. PUB drops on slow subscribers — the
/// index is a cache, loss only costs routing hints.
pub async fn kv_events_loop(bind: String, index: Arc<PrefixIndex>) -> anyhow::Result<()> {
    use zeromq::{Socket, SocketRecv};
    let mut socket = zeromq::SubSocket::new();
    socket.subscribe("").await?;
    socket.bind(&bind).await?;
    tracing::info!(bind = %bind, "kv events subscriber bound");
    loop {
        let frames: Vec<bytes::Bytes> = socket.recv().await?.into_vec();
        if frames.len() < 3 {
            continue;
        }
        let Some((model_uid, replica_id)) = parse_topic(&frames[0]) else {
            tracing::warn!(
                topic = %String::from_utf8_lossy(&frames[0]),
                "unparseable kv event topic"
            );
            continue;
        };
        let events = parse_batch(&frames[2]);
        if !events.is_empty() {
            index.apply(&model_uid, replica_id, &events, now_ms());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmpv::encode::write_value;

    fn stored_event(hashes: &[&[u8]], medium: Option<&str>) -> Value {
        Value::Array(vec![
            Value::from("BlockStored"),
            Value::Array(hashes.iter().map(|h| Value::Binary(h.to_vec())).collect()),
            Value::Nil,
            Value::Array((0..16).map(Value::from).collect()),
            Value::from(16),
            Value::Nil,
            match medium {
                Some(m) => Value::from(m),
                None => Value::Nil,
            },
        ])
    }

    fn removed_event(hashes: &[&[u8]]) -> Value {
        Value::Array(vec![
            Value::from("BlockRemoved"),
            Value::Array(hashes.iter().map(|h| Value::Binary(h.to_vec())).collect()),
            Value::from("GPU"),
        ])
    }

    fn batch(events: Vec<Value>) -> Vec<u8> {
        let mut buf = Vec::new();
        write_value(
            &mut buf,
            &Value::Array(vec![Value::from(1.5_f64), Value::Array(events)]),
        )
        .unwrap();
        buf
    }

    const H0: &[u8] = &[0xaa; 32];
    const H1: &[u8] = &[0xbb; 32];

    #[test]
    fn parse_and_index_roundtrip() {
        let events = parse_batch(&batch(vec![
            stored_event(&[H0, H1], Some("GPU")),
            removed_event(&[H0]),
        ]));
        assert_eq!(
            events,
            vec![
                KvEvent::Stored {
                    block_hashes: vec![H0.to_vec(), H1.to_vec()],
                    medium: Some("GPU".into()),
                },
                KvEvent::Removed {
                    block_hashes: vec![H0.to_vec()],
                },
            ]
        );

        let idx = PrefixIndex::new();
        idx.apply("m", 1, &events, 100);
        assert_eq!(idx.len(), 1); // H0 removed, H1 kept
        assert_eq!(idx.inserts_total(), 1);
    }

    #[test]
    fn lookup_prefers_deepest_chain() {
        let idx = PrefixIndex::new();
        // replica 0 holds block 0; replica 1 holds blocks 0 and 1.
        idx.apply(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![H0.to_vec()],
                medium: None,
            }],
            1,
        );
        idx.apply(
            "m",
            1,
            &[KvEvent::Stored {
                block_hashes: vec![H0.to_vec(), H1.to_vec()],
                medium: None,
            }],
            1,
        );
        let a: [[u8; 32]; 2] = [H0.try_into().unwrap(), H1.try_into().unwrap()];
        // Single-hash lookup: either holder is a valid answer (the index keeps
        // one owner per hash — last writer wins).
        assert!(idx.lookup("m", &a[..1]).is_some());
        assert_eq!(idx.lookup("m", &a), Some(1)); // deeper chain wins
        assert_eq!(idx.lookup("other", &a), None);
    }

    #[test]
    fn cleared_removes_only_that_replica() {
        let idx = PrefixIndex::new();
        idx.apply(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![H0.to_vec()],
                medium: None,
            }],
            1,
        );
        idx.apply(
            "m",
            1,
            &[KvEvent::Stored {
                block_hashes: vec![H1.to_vec()],
                medium: None,
            }],
            1,
        );
        idx.apply("m", 0, &[KvEvent::Cleared], 2);
        assert_eq!(idx.len(), 1);
        let a: [[u8; 32]; 1] = [H0.try_into().unwrap()];
        assert_eq!(idx.lookup("m", &a), None);
    }

    #[test]
    fn cpu_medium_not_recorded_and_ttl_evicts() {
        let idx = PrefixIndex::new();
        idx.apply(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![H0.to_vec()],
                medium: Some("CPU".into()),
            }],
            1,
        );
        assert_eq!(idx.len(), 0);

        idx.apply(
            "m",
            1,
            &[KvEvent::Stored {
                block_hashes: vec![H1.to_vec()],
                medium: None,
            }],
            100,
        );
        assert_eq!(idx.evict_expired(100 + 10_000, 5_000), 1);
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn topic_parses() {
        assert_eq!(parse_topic(b"qwen_7b/1"), Some(("qwen_7b".into(), 1)));
        assert_eq!(parse_topic(b"nobody"), None);
    }
}
