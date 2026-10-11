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

/// Canonical index key: the 8-byte external form (see [`crate::kv_hash::external`]).
/// Ints arrive as u64 (vLLM publishes `int.from_bytes(h, "big") & (2^64-1)`);
/// byte hashes (int publication disabled) are canonicalized to their last 8
/// bytes so both wire forms and request-side 32-byte hashes share one keyspace.
fn hash_to_key(v: &Value) -> Option<Vec<u8>> {
    match v {
        Value::Integer(i) => i.as_u64().map(|i| i.to_be_bytes().to_vec()),
        Value::Binary(b) if b.len() >= 8 => Some(b[b.len() - 8..].to_vec()),
        _ => None,
    }
}

fn hashes_of(v: &Value) -> Option<Vec<Vec<u8>>> {
    v.as_array()
        .map(|arr| arr.iter().filter_map(hash_to_key).collect())
}

fn parse_event(v: &Value) -> Option<KvEvent> {
    match v {
        // Engine path (v0.23 real wire format): msgspec map
        // {"type": "BlockStored", "block_hashes": [u64|bytes], "medium": ...}
        // (events are dict-externalized before publication).
        Value::Map(m) => {
            let get = |k: &str| {
                m.iter()
                    .find(|(key, _)| key.as_str() == Some(k))
                    .map(|(_, val)| val)
            };
            match get("type")?.as_str()? {
                "BlockStored" => Some(KvEvent::Stored {
                    block_hashes: hashes_of(get("block_hashes")?)?,
                    medium: get("medium").and_then(|m| m.as_str()).map(str::to_string),
                }),
                "BlockRemoved" => Some(KvEvent::Removed {
                    block_hashes: hashes_of(get("block_hashes")?)?,
                }),
                "AllBlocksCleared" => Some(KvEvent::Cleared),
                _ => None,
            }
        }
        // Synthetic / in-container probe path: tagged array
        // ["BlockStored", block_hashes, parent, token_ids, block_size, ...].
        Value::Array(arr) => match arr.first()?.as_str()? {
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
        },
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

    /// Gauge for /metrics: live index entries (routing hints).
    pub fn entries(&self) -> usize {
        self.map.len()
    }

    /// Live entries per model (O(n) — for metrics scrapes only).
    pub fn entries_by_model(&self) -> std::collections::HashMap<String, usize> {
        let mut out = std::collections::HashMap::new();
        for e in self.map.iter() {
            *out.entry(e.key().0.clone()).or_insert(0) += 1;
        }
        out
    }

    /// Distinct models currently in the index (for snapshot rounds).
    pub fn models(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        for e in self.map.iter() {
            seen.insert(e.key().0.clone());
        }
        seen.into_iter().collect()
    }

    /// Per-replica canonical keys for one model (snapshot payload).
    pub fn snapshot_for(&self, model_uid: &str) -> Vec<(u32, Vec<Vec<u8>>)> {
        let mut per: std::collections::BTreeMap<u32, Vec<Vec<u8>>> = Default::default();
        for e in self.map.iter() {
            if e.key().0 == model_uid {
                per.entry(e.value().0).or_default().push(e.key().1.clone());
            }
        }
        per.into_iter().collect()
    }

    /// Drop every entry of a model (snapshot replace semantics).
    pub fn clear_model(&self, model_uid: &str) {
        let keys: Vec<(String, Vec<u8>)> = self
            .map
            .iter()
            .filter(|e| e.key().0 == model_uid)
            .map(|e| e.key().clone())
            .collect();
        for k in keys {
            self.map.remove(&k);
        }
    }

    pub fn apply(&self, model_uid: &str, replica_id: u32, events: &[KvEvent], now_ms: u64) {
        // Canonical 8-byte external keyspace (see hash_to_key).
        let canon = |h: &Vec<u8>| -> Vec<u8> {
            if h.len() >= 8 {
                h[h.len() - 8..].to_vec()
            } else {
                h.clone()
            }
        };
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
                            .insert((model_uid.to_string(), canon(h)), (replica_id, now_ms));
                    }
                    self.inserts_total.fetch_add(1, Ordering::Relaxed);
                }
                KvEvent::Removed { block_hashes } => {
                    for h in block_hashes {
                        let key = (model_uid.to_string(), canon(h));
                        if let Some(entry) = self.map.get(&key) {
                            if entry.value().0 == replica_id {
                                drop(entry);
                                self.map.remove(&key);
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
            let key = crate::kv_hash::external(h).to_vec();
            if let Some(entry) = self.map.get(&(model_uid.to_string(), key)) {
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

/// KV-event ingestion loop (P1c).
///
/// Engines connect to `bind` (`NEBULA_ROUTER_KV_EVENTS_BIND`); the loop
/// parses batches into `index`. When `push_to` is set
/// (`NEBULA_ROUTER_KV_EVENTS_PUSH_TO`, comma-separated peer sink addrs)
/// every raw frame is also forwarded to peer routers' sinks (HA fanout).
/// Forwarding is best-effort: PUB drops are self-healing because engines
/// re-emit BlockStored on cache reuse.
///
/// shortcut: zeromq 0.4's PubSocket::bind and SubSocket::connect never
/// resolve (verified locally), so HA uses only the proven pair —
/// Pub-connect → Sub-bind; upgrade the crate to relax this.
pub async fn kv_events_loop(
    bind: Option<String>,
    push_to: Vec<String>,
    index: Arc<PrefixIndex>,
) -> anyhow::Result<()> {
    use zeromq::{Socket, SocketRecv, SocketSend};
    let Some(bind) = bind else {
        anyhow::bail!("kv events loop needs BIND");
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<bytes::Bytes>>();
    if !push_to.is_empty() {
        tokio::spawn(async move {
            // shortcut: zeromq 0.4–0.6 PUB sockets never reconnect to a
            // re-bound peer (verified locally), so the pusher is rebuilt
            // every 30s; frames buffer in the channel meanwhile.
            let mut pusher = connect_pusher(&push_to).await;
            tracing::info!(sinks = ?push_to, "kv events fanout pushing");
            let mut last_rebuild = std::time::Instant::now();
            while let Some(frames) = rx.recv().await {
                if last_rebuild.elapsed() >= std::time::Duration::from_secs(30) {
                    pusher = connect_pusher(&push_to).await;
                    last_rebuild = std::time::Instant::now();
                }
                let mut msg = zeromq::ZmqMessage::from(frames[0].clone());
                for f in &frames[1..] {
                    msg.push_back(f.clone());
                }
                // Best-effort: drops self-heal via reuse events.
                let _ = pusher.send(msg).await;
            }
        });
    }
    let mut socket = zeromq::SubSocket::new();
    socket.subscribe("").await?;
    socket.bind(&bind).await?;
    tracing::info!(bind = %bind, "kv events subscriber bound");
    loop {
        let frames: Vec<bytes::Bytes> = socket.recv().await?.into_vec();
        if frames.len() < 3 {
            continue;
        }
        let _ = tx.send(frames.clone());
        apply_frames(&frames, &index);
    }
}

/// Peer-sink ingestion (HA): bind a SUB socket that fanout peers push to
/// (`NEBULA_ROUTER_KV_EVENTS_SINK_BIND` on the replica routers). Besides
/// raw engine frames (`{model}/{replica}`), the control topics from the
/// index service are handled here: `!reset/{model}` clears a model's
/// entries, `!snapshot/{model}/{replica}` applies a snapshot batch.
pub async fn kv_sink_loop(sink_bind: String, index: Arc<PrefixIndex>) -> anyhow::Result<()> {
    use zeromq::{Socket, SocketRecv};
    let mut socket = zeromq::SubSocket::new();
    socket.subscribe("").await?;
    socket.bind(&sink_bind).await?;
    tracing::info!(sink = %sink_bind, "kv events peer sink bound");
    loop {
        let frames: Vec<bytes::Bytes> = socket.recv().await?.into_vec();
        if frames.len() < 3 {
            continue;
        }
        let topic = String::from_utf8_lossy(&frames[0]).to_string();
        if topic.starts_with('!') {
            apply_control(&topic, &frames[2], &index);
        } else {
            apply_frames(&frames, &index);
        }
    }
}

/// Handle index-service control topics (see [`kv_sink_loop`]).
fn apply_control(topic: &str, payload: &[u8], index: &PrefixIndex) {
    if let Some(model) = topic.strip_prefix("!reset/") {
        index.clear_model(model);
        return;
    }
    if let Some(rest) = topic.strip_prefix("!snapshot/") {
        if let Some((model, replica)) = rest.rsplit_once('/') {
            if let Ok(replica) = replica.parse::<u32>() {
                let events = parse_batch(payload);
                index.apply(model, replica, &events, now_ms());
            }
        }
    }
}

/// Encode one msgpack batch `[ts, [BlockStored(map), ...], 0]` carrying the
/// given canonical 8-byte keys — the index service's snapshot wire form
/// (parses back through the regular map-form path).
pub fn encode_stored_batch(keys: &[Vec<u8>]) -> Vec<u8> {
    use rmpv::encode::write_value;
    let events: Vec<rmpv::Value> = keys
        .chunks(512)
        .map(|chunk| {
            rmpv::Value::Map(vec![
                (rmpv::Value::from("type"), rmpv::Value::from("BlockStored")),
                (
                    rmpv::Value::from("block_hashes"),
                    rmpv::Value::Array(
                        chunk
                            .iter()
                            .map(|k| rmpv::Value::Binary(k.as_slice().into()))
                            .collect(),
                    ),
                ),
                (rmpv::Value::from("medium"), rmpv::Value::from("GPU")),
            ])
        })
        .collect();
    let batch = rmpv::Value::Array(vec![
        rmpv::Value::from(0.0_f64),
        rmpv::Value::Array(events),
        rmpv::Value::from(0u64),
    ]);
    let mut buf = Vec::new();
    let _ = write_value(&mut buf, &batch);
    buf
}

/// Build a fresh PUB pusher connected to all sinks (see the rebuild note
/// in [`kv_events_loop`]).
pub async fn connect_pusher(push_to: &[String]) -> zeromq::PubSocket {
    use zeromq::Socket;
    let mut pusher = zeromq::PubSocket::with_options(zeromq::SocketOptions::default());
    for addr in push_to {
        if let Err(e) = pusher.connect(addr).await {
            tracing::warn!(addr = %addr, error = %e, "fanout connect failed");
        }
    }
    pusher
}

fn apply_frames(frames: &[bytes::Bytes], index: &PrefixIndex) {
    let Some((model_uid, replica_id)) = parse_topic(&frames[0]) else {
        tracing::warn!(
            topic = %String::from_utf8_lossy(&frames[0]),
            "unparseable kv event topic"
        );
        return;
    };
    let events = parse_batch(&frames[2]);
    if !events.is_empty() {
        index.apply(&model_uid, replica_id, &events, now_ms());
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
                    block_hashes: vec![[0xaa; 8].to_vec(), [0xbb; 8].to_vec()],
                    medium: Some("GPU".into()),
                },
                KvEvent::Removed {
                    block_hashes: vec![[0xaa; 8].to_vec()],
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

    // Raw zmq payloads captured from the deployed
    // vllm/vllm-openai:v0.23.0 container on the test rig
    // (PYTHONHASHSEED=42, algo sha256_cbor, tokens 100..132):
    // outer batch [ts, events, dp_rank]; BlockStored with 2 bin32 hashes.
    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    fn last8(hex32: &str) -> Vec<u8> {
        unhex(hex32)[24..].to_vec()
    }

    #[test]
    fn int_hashes_share_keyspace_with_request_hashes() {
        // Real engine numbers (v0.23, PYTHONHASHSEED=42): the engine published
        // int 1015703622044383654 = last-8-bytes-BE of the 32-byte block hash
        // for the calibrated prompt. A request computing the full 32-byte hash
        // must hit the same index entry.
        let idx = PrefixIndex::new();
        let ev = Value::Array(vec![
            Value::from("BlockStored"),
            Value::Array(vec![Value::from(1015703622044383654u64)]),
            Value::Nil,
            Value::Array((0..16).map(Value::from).collect()),
            Value::from(16),
            Value::Nil,
            Value::from("GPU"),
        ]);
        let mut buf = Vec::new();
        write_value(
            &mut buf,
            &Value::Array(vec![Value::from(1.0_f64), Value::Array(vec![ev])]),
        )
        .unwrap();
        idx.apply("m", 1, &parse_batch(&buf), 1);
        let mut req = [0u8; 32];
        req[24..].copy_from_slice(&1015703622044383654u64.to_be_bytes());
        assert_eq!(idx.lookup("m", &[req]), Some(1));
    }

    #[test]
    fn snapshot_roundtrip_via_control_topics() {
        // Build an index across two replicas, snapshot it, wipe, restore via
        // the control-topic path (!reset + !snapshot/...) — lookups identical.
        let idx = PrefixIndex::new();
        let k = |b: u8| vec![b; 8];
        idx.apply(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![k(0xaa), k(0xbb)],
                medium: None,
            }],
            100,
        );
        idx.apply(
            "m",
            1,
            &[KvEvent::Stored {
                block_hashes: vec![k(0xcc)],
                medium: None,
            }],
            100,
        );
        let snap = idx.snapshot_for("m");
        assert_eq!(snap.len(), 2);

        let restored = PrefixIndex::new();
        apply_control("!reset/m", &[], &restored);
        for (replica, keys) in &snap {
            let payload = encode_stored_batch(keys);
            apply_control(&format!("!snapshot/m/{replica}"), &payload, &restored);
        }
        let mut any = [0u8; 32];
        any[24..].copy_from_slice(&[0xaa; 8]);
        let mut deep = [0u8; 32];
        deep[24..].copy_from_slice(&[0xcc; 8]);
        assert_eq!(restored.lookup("m", &[any]), Some(0));
        assert_eq!(restored.lookup("m", &[deep]), Some(1));
        assert_eq!(restored.len(), 3);

        // Replace semantics: reset alone clears.
        apply_control("!reset/m", &[], &restored);
        assert_eq!(restored.len(), 0);
    }

    #[test]
    fn parses_real_engine_map_form_frame() {
        // Raw zmq payload captured from the LIVE engine (v0.23 on the rig,
        // recreated pods): outer [ts, events, dp_rank] with the event as a
        // msgpack MAP {"type":"BlockStored","block_hashes":[u64],"medium":"GPU"}.
        let payload = unhex(
            "93cb41dab2eb710e14e5918ba474797065ab426c6f636b53746f726564ac626c6f636b5f68617368657391cfcdfa5d16f31c2abcb1706172656e745f626c6f636b5f68617368c0a9746f6b656e5f696473dc0010cd906fcd1e2acd3af1cd2424cd0171cd09b0cd236bcd0fc2cd3022cd0a6ecd0edfcd11e7ce00014aa8cd271acd013bcd01f5aa626c6f636b5f73697a6510a76c6f72615f6964c0a66d656469756da3475055a96c6f72615f6e616d65c0aa65787472615f6b65797391c0a967726f75705f69647800b26b765f63616368655f737065635f6b696e64ae66756c6c5f617474656e74696f6e00",
        );
        let evs = parse_batch(&payload);
        assert_eq!(evs.len(), 1);
        assert_eq!(
            evs[0],
            KvEvent::Stored {
                block_hashes: vec![0xcdfa_5d16_f31c_2abcu64.to_be_bytes().to_vec()],
                medium: Some("GPU".into()),
            }
        );
    }

    #[test]
    fn parses_real_v023_deployed_frames() {
        let stored = unhex(
            "93cb3ff8000000000000919cab426c6f636b53746f72656492c4203f9c7995b81c03a2ab8ad3046caf562b8e746d217638d4dc5c672504f0cb3b66c42045e3af14cac8deda96eeda45e3cea05b95ff309dc5c00e2073ecfc7a800e250cc0dc00206465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7fcc80cc81cc82cc8310c0a3475055c0c0c0c0c000",
        );
        let evs = parse_batch(&stored);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            KvEvent::Stored {
                block_hashes,
                medium,
            } => {
                assert_eq!(medium.as_deref(), Some("GPU"));
                assert_eq!(
                    block_hashes,
                    &vec![
                        last8("3f9c7995b81c03a2ab8ad3046caf562b8e746d217638d4dc5c672504f0cb3b66"),
                        last8("45e3af14cac8deda96eeda45e3cea05b95ff309dc5c00e2073ecfc7a800e250c"),
                    ]
                );
            }
            other => panic!("expected BlockStored, got {other:?}"),
        }

        let removed = unhex(
            "93cb3ff999999999999a9194ac426c6f636b52656d6f76656491c42045e3af14cac8deda96eeda45e3cea05b95ff309dc5c00e2073ecfc7a800e250ca3475055c000",
        );
        assert_eq!(
            parse_batch(&removed),
            vec![KvEvent::Removed {
                block_hashes: vec![last8(
                    "45e3af14cac8deda96eeda45e3cea05b95ff309dc5c00e2073ecfc7a800e250c",
                )],
            }]
        );
    }
}
