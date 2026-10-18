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

/// Per-replica canonical keys for a model snapshot.
pub type ReplicaSnapshot = Vec<(u32, Vec<Vec<u8>>)>;

/// Per-replica `(last_seen_ms, epoch, generation)` ownership map for a block hash.
type OwnerMap = std::collections::BTreeMap<u32, (u64, u64, u64)>;

static NEXT_LOCAL_EPOCH: AtomicU64 = AtomicU64::new(1);

fn allocate_publisher_epoch() -> u64 {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(1);
    let seq = NEXT_LOCAL_EPOCH.fetch_add(1, Ordering::Relaxed);
    ts.wrapping_shl(12).wrapping_add(seq).max(1)
}

#[inline]
fn is_older_watermark(
    inc_epoch: u64,
    inc_gen: u64,
    inc_ts: u64,
    ref_epoch: u64,
    ref_gen: u64,
    ref_ts: u64,
) -> bool {
    if inc_epoch == ref_epoch {
        inc_gen < ref_gen
    } else {
        inc_ts < ref_ts
    }
}

/// Encode `(epoch, generation)` into a 16-byte sequence frame.
pub fn encode_watermark(epoch: u64, gen: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(16);
    out.extend_from_slice(&epoch.to_be_bytes());
    out.extend_from_slice(&gen.to_be_bytes());
    out
}

/// Parse a sequence frame into `(epoch, generation)`. Supports 16-byte
/// `(epoch, gen)` frames as well as legacy 8-byte `gen`-only frames (`epoch = 0`).
pub fn parse_watermark(seq_frame: &[u8]) -> (u64, u64) {
    if seq_frame.len() == 16 {
        let mut e_buf = [0u8; 8];
        let mut g_buf = [0u8; 8];
        e_buf.copy_from_slice(&seq_frame[..8]);
        g_buf.copy_from_slice(&seq_frame[8..16]);
        (u64::from_be_bytes(e_buf), u64::from_be_bytes(g_buf))
    } else if seq_frame.len() == 8 {
        let mut g_buf = [0u8; 8];
        g_buf.copy_from_slice(seq_frame);
        (0, u64::from_be_bytes(g_buf))
    } else {
        (0, 0)
    }
}

/// Rebuildable multi-owner map of `(model_uid, block_hash) → {replica_id: (last_seen_ms, epoch, generation)}`.
///
/// Tracks per-replica ownership so `pd_split` deployments (where both `Prefill`
/// and one or more `Decode` replicas emit `BlockStored` for the same block)
/// retain every holder instead of letting the last writer overwrite earlier
/// holders. Also tracks a publisher `(epoch, generation)` watermark plus
/// per-replica removal tombstones `(model_uid, block_hash, replica_id) → (epoch, removed_gen, ts_ms)`:
/// - Within the same publisher `epoch` (`indexd` or a single router), `generation`
///   strictly orders interleaved snapshots (`!reset` / `!snapshot`) and live deltas.
/// - When `indexd` restarts with a new `epoch`, its fresh `generation` counter is
///   never blocked by the previous incarnation's `model_reset_watermark`.
/// - Across different HA peer routers (`epoch_a != epoch_b`), independent counters
///   are never compared directly; recency falls back to `last_seen_ms`.
type TombstoneKey = (String, Vec<u8>, u32);
type TombstoneVal = (u64, u64, u64);
type Watermark = (u64, u64);

pub struct PrefixIndex {
    map: DashMap<(String, Vec<u8>), OwnerMap>,
    tombstones: DashMap<TombstoneKey, TombstoneVal>,
    model_reset_watermark: DashMap<String, Watermark>,
    watermark_lock: std::sync::RwLock<()>,
    epoch: u64,
    generation: AtomicU64,
    inserts_total: AtomicU64,
    stale_dropped_total: AtomicU64,
}

impl PrefixIndex {
    pub fn new() -> Arc<Self> {
        Self::with_epoch(allocate_publisher_epoch())
    }

    pub fn with_epoch(epoch: u64) -> Arc<Self> {
        Arc::new(Self {
            map: DashMap::new(),
            tombstones: DashMap::new(),
            model_reset_watermark: DashMap::new(),
            watermark_lock: std::sync::RwLock::new(()),
            epoch,
            generation: AtomicU64::new(0),
            inserts_total: AtomicU64::new(0),
            stale_dropped_total: AtomicU64::new(0),
        })
    }

    /// Publisher epoch identifier for this index instance.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Allocate the next monotonic generation number within this instance's epoch.
    pub fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Current generation watermark (used by `indexd` when taking a snapshot).
    pub fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    fn observe_local_generation(&self, gen: u64) {
        let mut cur = self.generation.load(Ordering::Relaxed);
        while gen > cur {
            match self.generation.compare_exchange_weak(
                cur,
                gen,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
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

    /// Number of events/snapshots ignored because a newer watermark was already present.
    pub fn stale_dropped_total(&self) -> u64 {
        self.stale_dropped_total.load(Ordering::Relaxed)
    }

    /// Gauge for /metrics: live index entries (distinct `(model_uid, block_hash)` keys).
    pub fn entries(&self) -> usize {
        self.map.len()
    }

    /// Live entries per model (O(n) — for metrics scrapes only).
    pub fn entries_by_model(&self) -> std::collections::HashMap<String, usize> {
        let mut out = std::collections::HashMap::new();
        for e in self.map.iter() {
            if !e.value().is_empty() {
                *out.entry(e.key().0.clone()).or_insert(0) += 1;
            }
        }
        out
    }

    /// Distinct models currently in the index (for snapshot rounds).
    pub fn models(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        for e in self.map.iter() {
            if !e.value().is_empty() {
                seen.insert(e.key().0.clone());
            }
        }
        seen.into_iter().collect()
    }

    /// Per-replica canonical keys for one model (snapshot payload).
    pub fn snapshot_for(&self, model_uid: &str) -> ReplicaSnapshot {
        let mut per: std::collections::BTreeMap<u32, Vec<Vec<u8>>> = Default::default();
        for e in self.map.iter() {
            if e.key().0 == model_uid {
                for &replica_id in e.value().keys() {
                    per.entry(replica_id).or_default().push(e.key().1.clone());
                }
            }
        }
        per.into_iter().collect()
    }

    /// Snapshot the publisher `generation` watermark and per-replica keys for a model
    /// atomically with respect to in-flight generation allocation + event application.
    pub fn snapshot_for_with_gen(&self, model_uid: &str) -> (u64, ReplicaSnapshot) {
        let _guard = self
            .watermark_lock
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let gen = self.current_generation().max(1);
        (gen, self.snapshot_for(model_uid))
    }

    /// Drop every entry of a model unconditionally (unversioned reset).
    #[cfg(test)]
    pub fn clear_model(&self, model_uid: &str) {
        self.clear_model_at_watermark(model_uid, 0, 0);
    }

    /// Drop entries of `model_uid` for a snapshot reset at `(epoch, reset_gen)`.
    ///
    /// - Within the same `epoch`, preserves any newer delta (`e_gen > reset_gen`)
    ///   that arrived before an in-flight snapshot reset, and ignores a stale
    ///   `!reset` if a higher `reset_gen` for that `epoch` was already applied.
    /// - When `indexd` restarts with a new `epoch` (`epoch != prev_epoch`),
    ///   adopts the new `(epoch, reset_gen)` watermark instead of getting stuck
    ///   on the previous incarnation's higher `reset_gen`.
    pub fn clear_model_at_watermark(&self, model_uid: &str, epoch: u64, reset_gen: u64) {
        let _guard = self
            .watermark_lock
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if reset_gen == 0 {
            self.model_reset_watermark.remove(model_uid);
        } else {
            if epoch != 0 && epoch == self.epoch {
                self.observe_local_generation(reset_gen);
            }
            let mut prev_epoch_to_purge: Option<u64> = None;
            if let Some(prev) = self.model_reset_watermark.get(model_uid) {
                let (prev_epoch, prev_gen) = *prev.value();
                if prev_epoch == epoch && reset_gen < prev_gen {
                    self.stale_dropped_total.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(
                        model_uid = %model_uid,
                        epoch,
                        reset_gen,
                        prev_gen,
                        "ignoring stale snapshot reset from same epoch"
                    );
                    return;
                }
                if prev_epoch != epoch {
                    prev_epoch_to_purge = Some(prev_epoch);
                }
            }
            self.model_reset_watermark
                .insert(model_uid.to_string(), (epoch, reset_gen));
            let tkeys: Vec<(String, Vec<u8>, u32)> = self
                .tombstones
                .iter()
                .filter(|e| {
                    e.key().0 == model_uid
                        && (prev_epoch_to_purge.is_some_and(|old_ep| e.value().0 == old_ep)
                            || (e.value().0 == epoch && e.value().1 < reset_gen))
                })
                .map(|e| e.key().clone())
                .collect();
            for k in tkeys {
                self.tombstones.remove(&k);
            }
        }

        let keys: Vec<(String, Vec<u8>)> = self
            .map
            .iter()
            .filter(|e| e.key().0 == model_uid)
            .map(|e| e.key().clone())
            .collect();
        for k in keys {
            if reset_gen == 0 {
                self.map.remove(&k);
            } else {
                self.map.remove_if_mut(&k, |_, owners| {
                    // Keep only entries from the same epoch that are strictly
                    // newer than `reset_gen`; older entries (or entries from a
                    // superseded indexd epoch) are replaced by the snapshot.
                    owners.retain(|_, &mut (_, e_epoch, e_gen)| {
                        e_epoch == epoch && e_gen > reset_gen
                    });
                    owners.is_empty()
                });
            }
        }
        if reset_gen == 0 {
            let tkeys: Vec<(String, Vec<u8>, u32)> = self
                .tombstones
                .iter()
                .filter(|e| e.key().0 == model_uid)
                .map(|e| e.key().clone())
                .collect();
            for k in tkeys {
                self.tombstones.remove(&k);
            }
        }
    }

    pub fn apply(&self, model_uid: &str, replica_id: u32, events: &[KvEvent], now_ms: u64) {
        let _guard = self
            .watermark_lock
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let gen = self.next_generation();
        self.apply_with_watermark_locked(model_uid, replica_id, events, now_ms, self.epoch, gen);
    }

    #[cfg(test)]
    pub fn apply_with_gen(
        &self,
        model_uid: &str,
        replica_id: u32,
        events: &[KvEvent],
        now_ms: u64,
        gen: u64,
    ) {
        self.apply_with_watermark(model_uid, replica_id, events, now_ms, self.epoch, gen);
    }

    pub fn apply_with_watermark(
        &self,
        model_uid: &str,
        replica_id: u32,
        events: &[KvEvent],
        now_ms: u64,
        epoch: u64,
        gen: u64,
    ) {
        let _guard = self
            .watermark_lock
            .write()
            .unwrap_or_else(|e| e.into_inner());
        self.apply_with_watermark_locked(model_uid, replica_id, events, now_ms, epoch, gen);
    }

    fn apply_with_watermark_locked(
        &self,
        model_uid: &str,
        replica_id: u32,
        events: &[KvEvent],
        now_ms: u64,
        epoch: u64,
        gen: u64,
    ) {
        let (epoch, gen) = if epoch == 0 && gen == 0 {
            (self.epoch, self.next_generation())
        } else if gen == 0 {
            (epoch, self.next_generation())
        } else {
            // Only advance local generation when the frame explicitly carries
            // this instance's non-zero epoch; legacy 8-byte wire frames have
            // `epoch == 0` and must never impersonate `self.epoch`.
            if epoch != 0 && epoch == self.epoch {
                self.observe_local_generation(gen);
            }
            (epoch, gen)
        };
        if let Some((reset_epoch, reset_gen)) = self
            .model_reset_watermark
            .get(model_uid)
            .map(|g| *g.value())
        {
            if epoch == reset_epoch && gen < reset_gen {
                self.stale_dropped_total.fetch_add(1, Ordering::Relaxed);
                tracing::debug!(
                    model_uid = %model_uid,
                    replica_id,
                    epoch,
                    gen,
                    reset_gen,
                    "dropping kv event older than model reset watermark"
                );
                return;
            }
        }
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
                    let mut inserted_any = false;
                    for h in block_hashes {
                        let c = canon(h);
                        let key = (model_uid.to_string(), c.clone());
                        let tkey = (model_uid.to_string(), c, replica_id);
                        if let dashmap::mapref::entry::Entry::Occupied(tomb) =
                            self.tombstones.entry(tkey)
                        {
                            let (t_epoch, t_gen, t_ts) = *tomb.get();
                            if is_older_watermark(epoch, gen, now_ms, t_epoch, t_gen, t_ts) {
                                self.stale_dropped_total.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            tomb.remove();
                        }
                        let mut owners = self.map.entry(key).or_default();
                        if let Some(&(ex_ts, ex_epoch, ex_gen)) = owners.get(&replica_id) {
                            if is_older_watermark(epoch, gen, now_ms, ex_epoch, ex_gen, ex_ts) {
                                self.stale_dropped_total.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                        }
                        owners.insert(replica_id, (now_ms, epoch, gen));
                        inserted_any = true;
                    }
                    if inserted_any {
                        self.inserts_total.fetch_add(1, Ordering::Relaxed);
                    }
                }
                KvEvent::Removed { block_hashes } => {
                    for h in block_hashes {
                        let c = canon(h);
                        let key = (model_uid.to_string(), c.clone());
                        let tkey = (model_uid.to_string(), c, replica_id);
                        let mut skip = false;
                        if let dashmap::mapref::entry::Entry::Occupied(mut existing) =
                            self.map.entry(key)
                        {
                            if let Some(&(ex_ts, ex_epoch, ex_gen)) =
                                existing.get().get(&replica_id)
                            {
                                if is_older_watermark(epoch, gen, now_ms, ex_epoch, ex_gen, ex_ts) {
                                    skip = true;
                                } else {
                                    existing.get_mut().remove(&replica_id);
                                    if existing.get().is_empty() {
                                        existing.remove();
                                    }
                                }
                            }
                        }
                        if skip {
                            self.stale_dropped_total.fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        self.tombstones
                            .entry(tkey)
                            .and_modify(|(t_epoch, t_gen, t_ts)| {
                                if !is_older_watermark(epoch, gen, now_ms, *t_epoch, *t_gen, *t_ts)
                                {
                                    *t_epoch = epoch;
                                    *t_gen = gen;
                                    *t_ts = now_ms;
                                }
                            })
                            .or_insert((epoch, gen, now_ms));
                    }
                }
                KvEvent::Cleared => {
                    let keys: Vec<(String, Vec<u8>)> = self
                        .map
                        .iter()
                        .filter(|e| {
                            e.key().0 == model_uid
                                && e.value()
                                    .get(&replica_id)
                                    .map(|&(ex_ts, ex_epoch, ex_gen)| {
                                        !is_older_watermark(
                                            epoch, gen, now_ms, ex_epoch, ex_gen, ex_ts,
                                        )
                                    })
                                    .unwrap_or(false)
                        })
                        .map(|e| e.key().clone())
                        .collect();
                    for k in keys {
                        self.map.remove_if_mut(&k, |_, owners| {
                            owners.remove(&replica_id);
                            owners.is_empty()
                        });
                        self.tombstones
                            .insert((k.0, k.1, replica_id), (epoch, gen, now_ms));
                    }
                }
            }
        }
    }

    /// Atomically allocate the next generation within `self.epoch`, stamp
    /// `frames[1]` with `(self.epoch, gen)`, and apply the event batch while
    /// holding `watermark_lock` so concurrent snapshots never observe an
    /// allocated generation before its entries are applied.
    pub fn ingest_and_stamp_frames(&self, frames: &mut [bytes::Bytes]) -> (u64, u64) {
        let parsed = if let Some((model_uid, replica_id)) = parse_topic(&frames[0]) {
            Some((model_uid, replica_id, parse_batch(&frames[2])))
        } else {
            tracing::warn!(
                topic = %String::from_utf8_lossy(&frames[0]),
                "unparseable kv event topic"
            );
            None
        };
        let _guard = self
            .watermark_lock
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let gen = self.next_generation();
        let epoch = self.epoch;
        frames[1] = bytes::Bytes::from(encode_watermark(epoch, gen));
        if let Some((model_uid, replica_id, events)) = parsed {
            if !events.is_empty() {
                self.apply_with_watermark_locked(
                    &model_uid,
                    replica_id,
                    &events,
                    now_ms(),
                    epoch,
                    gen,
                );
            }
        }
        (epoch, gen)
    }

    /// Pick the replica holding the longest matched prefix chain across all replicas.
    pub fn lookup(&self, model_uid: &str, hashes: &[[u8; 32]]) -> Option<u32> {
        self.lookup_in(model_uid, hashes, None)
    }

    /// Pick the replica (optionally restricted to `allowed_replicas`) holding
    /// the longest matched prefix chain.
    ///
    /// Chaining makes any matched hash proof of exact token-level prefix
    /// identity up to that block, so depth beats count; equal `(depth, count)`
    /// breaks by generation when from the same publisher epoch (or `last_seen_ms`
    /// across distinct publisher epochs), then lowest `replica_id` for determinism.
    pub fn lookup_in(
        &self,
        model_uid: &str,
        hashes: &[[u8; 32]],
        allowed_replicas: Option<&[u32]>,
    ) -> Option<u32> {
        // replica_id -> (max_depth, matched_count, max_ts, epoch_of_max, max_gen)
        let mut per_replica: std::collections::BTreeMap<u32, (usize, usize, u64, u64, u64)> =
            std::collections::BTreeMap::new();
        for (depth, h) in hashes.iter().enumerate() {
            let key = crate::kv_hash::external(h).to_vec();
            if let Some(entry) = self.map.get(&(model_uid.to_string(), key)) {
                for (&replica, &(ts, epoch, gen)) in entry.value().iter() {
                    if let Some(allowed) = allowed_replicas {
                        if !allowed.contains(&replica) {
                            continue;
                        }
                    }
                    let slot = per_replica.entry(replica).or_insert((0, 0, 0, epoch, 0));
                    slot.0 = slot.0.max(depth);
                    slot.1 += 1;
                    if slot.3 == epoch {
                        if gen >= slot.4 {
                            slot.4 = gen;
                            slot.2 = slot.2.max(ts);
                        }
                    } else if ts >= slot.2 {
                        slot.2 = ts;
                        slot.3 = epoch;
                        slot.4 = gen;
                    }
                }
            }
        }
        per_replica
            .into_iter()
            .max_by(
                |(r_a, (depth_a, count_a, ts_a, epoch_a, gen_a)),
                 (r_b, (depth_b, count_b, ts_b, epoch_b, gen_b))| {
                    depth_a
                        .cmp(depth_b)
                        .then_with(|| count_a.cmp(count_b))
                        .then_with(|| {
                            if epoch_a == epoch_b {
                                gen_a.cmp(gen_b).then_with(|| ts_a.cmp(ts_b))
                            } else {
                                ts_a.cmp(ts_b)
                            }
                        })
                        .then_with(|| r_b.cmp(r_a))
                },
            )
            .map(|(replica, _)| replica)
    }

    /// Return the deepest matched prefix block count (`replica_id -> matched_blocks`)
    /// across all replicas (or restricted to `allowed_replicas`).
    ///
    /// Because each block hash in `hashes` is chained from the root, a match at
    /// `depth` proves exact prefix identity of `depth + 1` blocks (`(depth + 1) * 16`
    /// tokens), powering Batch 5.2's joint `SavedPrefill - QueueWait` cost function.
    pub fn lookup_matches_in(
        &self,
        model_uid: &str,
        hashes: &[[u8; 32]],
        allowed_replicas: Option<&[u32]>,
    ) -> std::collections::HashMap<u32, usize> {
        let mut out = std::collections::HashMap::new();
        for (depth, h) in hashes.iter().enumerate() {
            let key = crate::kv_hash::external(h).to_vec();
            if let Some(entry) = self.map.get(&(model_uid.to_string(), key)) {
                for &replica in entry.value().keys() {
                    if let Some(allowed) = allowed_replicas {
                        if !allowed.contains(&replica) {
                            continue;
                        }
                    }
                    let slot = out.entry(replica).or_insert(0usize);
                    *slot = (*slot).max(depth + 1);
                }
            }
        }
        out
    }

    /// Drop replica entries not refreshed within the TTL (KV blocks are LRU-evicted
    /// inside the engine; the index must not outlive them).
    pub fn evict_expired(&self, now_ms: u64, ttl_ms: u64) -> usize {
        let cutoff = now_ms.saturating_sub(ttl_ms);
        let keys: Vec<(String, Vec<u8>)> = self.map.iter().map(|e| e.key().clone()).collect();
        let mut evicted = 0usize;
        for k in keys {
            self.map.remove_if_mut(&k, |_, owners| {
                let before = owners.len();
                owners.retain(|_, &mut (ts, _, _)| ts >= cutoff);
                evicted += before.saturating_sub(owners.len());
                owners.is_empty()
            });
        }
        let tkeys: Vec<(String, Vec<u8>, u32)> = self
            .tombstones
            .iter()
            .filter(|e| e.value().2 < cutoff)
            .map(|e| e.key().clone())
            .collect();
        for k in tkeys {
            self.tombstones.remove(&k);
        }
        evicted
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
        let mut frames: Vec<bytes::Bytes> = socket.recv().await?.into_vec();
        if frames.len() < 3 {
            continue;
        }
        index.ingest_and_stamp_frames(&mut frames);
        let _ = tx.send(frames);
    }
}

/// Env-driven KV-event runtime: ingest loop (BIND + optional PUSH_TO fanout),
/// peer sink (SINK_BIND), and the TTL sweep. **Shared by the standalone router
/// and the gateway's embedded mode** so both feed the prefix index identically
/// (assessment P0-2: embedded mode used to start only the etcd sync loops,
/// leaving the index permanently empty there). Must be called inside a tokio
/// runtime; no-op spawns nothing when no BIND/SINK is configured.
pub fn start_kv_runtime(index: Arc<PrefixIndex>) {
    let kv_bind = std::env::var("NEBULA_ROUTER_KV_EVENTS_BIND").ok();
    let kv_push_to: Vec<String> = std::env::var("NEBULA_ROUTER_KV_EVENTS_PUSH_TO")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if let Some(bind) = kv_bind {
        let index = index.clone();
        let push_to = kv_push_to.clone();
        tokio::spawn(async move {
            if let Err(e) = kv_events_loop(Some(bind), push_to, index).await {
                tracing::error!(error = %e, "kv events loop exited");
            }
        });
    }
    if let Ok(sink) = std::env::var("NEBULA_ROUTER_KV_EVENTS_SINK_BIND") {
        let index = index.clone();
        tokio::spawn(async move {
            if let Err(e) = kv_sink_loop(sink, index).await {
                tracing::error!(error = %e, "kv sink loop exited");
            }
        });
    }
    let ttl_ms = std::env::var("NEBULA_ROUTER_PREFIX_TTL_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300_000);
    let index = index.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let evicted = index.evict_expired(now, ttl_ms);
            if evicted > 0 {
                tracing::debug!(evicted, "prefix index ttl sweep");
            }
        }
    });
}

/// Peer-sink ingestion (HA): bind a SUB socket that fanout peers push to
/// (`NEBULA_ROUTER_KV_EVENTS_SINK_BIND` on the replica routers). Besides
/// raw engine frames (`{model}/{replica}`), the control topics from the
/// index service are handled here: `!reset/{model}` clears a model's
/// entries up to the snapshot's `(epoch, generation)` watermark, and
/// `!snapshot/{model}/{replica}` applies a snapshot batch at that watermark.
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
        let (epoch, gen) = parse_watermark(&frames[1]);
        if topic.starts_with('!') {
            apply_control_with_watermark(&topic, epoch, gen, &frames[2], &index);
        } else {
            apply_frames_with_watermark(&frames, &index, epoch, gen);
        }
    }
}

/// Handle index-service control topics (see [`kv_sink_loop`]).
#[cfg(test)]
fn apply_control(topic: &str, payload: &[u8], index: &PrefixIndex) {
    apply_control_with_watermark(topic, index.epoch(), 0, payload, index);
}

#[cfg(test)]
pub fn apply_control_with_gen(topic: &str, gen: u64, payload: &[u8], index: &PrefixIndex) {
    apply_control_with_watermark(topic, index.epoch(), gen, payload, index);
}

pub fn apply_control_with_watermark(
    topic: &str,
    epoch: u64,
    gen: u64,
    payload: &[u8],
    index: &PrefixIndex,
) {
    if let Some(model) = topic.strip_prefix("!reset/") {
        index.clear_model_at_watermark(model, epoch, gen);
        return;
    }
    if let Some(rest) = topic.strip_prefix("!snapshot/") {
        if let Some((model, replica)) = rest.rsplit_once('/') {
            if let Ok(replica) = replica.parse::<u32>() {
                let events = parse_batch(payload);
                index.apply_with_watermark(model, replica, &events, now_ms(), epoch, gen);
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

fn apply_frames_with_watermark(frames: &[bytes::Bytes], index: &PrefixIndex, epoch: u64, gen: u64) {
    let Some((model_uid, replica_id)) = parse_topic(&frames[0]) else {
        tracing::warn!(
            topic = %String::from_utf8_lossy(&frames[0]),
            "unparseable kv event topic"
        );
        return;
    };
    let events = parse_batch(&frames[2]);
    if !events.is_empty() {
        index.apply_with_watermark(&model_uid, replica_id, &events, now_ms(), epoch, gen);
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
    fn interleaved_snapshot_and_delta_respect_generation_watermark() {
        let k = |b: u8| vec![b; 8];
        let mut h_aa = [0u8; 32];
        h_aa[24..].copy_from_slice(&[0xaa; 8]);
        let mut h_bb = [0u8; 32];
        h_bb[24..].copy_from_slice(&[0xbb; 8]);
        let mut h_cc = [0u8; 32];
        h_cc[24..].copy_from_slice(&[0xcc; 8]);

        // Suppose indexd took a snapshot at gen=10 where replica 0 held [0xaa, 0xbb].
        let snap_gen = 10;
        let snap_payload = encode_stored_batch(&[k(0xaa), k(0xbb)]);

        // Meanwhile on the router, newer deltas (gen=11, gen=12) arrive BEFORE the
        // delayed snapshot (gen=10):
        // - gen=11: replica 0 evicts 0xaa
        // - gen=12: replica 1 stores 0xbb and 0xcc
        let router_idx = PrefixIndex::new();
        router_idx.apply_with_gen(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![k(0xaa), k(0xbb)],
                medium: None,
            }],
            100,
            snap_gen,
        );
        router_idx.apply_with_gen(
            "m",
            0,
            &[KvEvent::Removed {
                block_hashes: vec![k(0xaa)],
            }],
            110,
            11,
        );
        router_idx.apply_with_gen(
            "m",
            1,
            &[KvEvent::Stored {
                block_hashes: vec![k(0xbb), k(0xcc)],
                medium: None,
            }],
            120,
            12,
        );

        // Delayed snapshot at gen=10 arrives now (!reset/m + !snapshot/m/0):
        // It must NOT wipe gen=12 entries, must NOT resurrect 0xaa (evicted at gen=11),
        // and must NOT overwrite 0xbb back to replica 0.
        apply_control_with_gen("!reset/m", snap_gen, &[], &router_idx);
        apply_control_with_gen("!snapshot/m/0", snap_gen, &snap_payload, &router_idx);

        assert_eq!(
            router_idx.lookup("m", &[h_aa]),
            None,
            "stale snapshot must not resurrect block evicted at newer generation"
        );
        assert_eq!(
            router_idx.lookup("m", &[h_bb]),
            Some(1),
            "stale snapshot must not overwrite block moved to replica 1 at newer generation"
        );
        assert_eq!(
            router_idx.lookup("m", &[h_cc]),
            Some(1),
            "stale snapshot reset must not wipe delta stored at newer generation"
        );
        assert!(router_idx.stale_dropped_total() > 0);
    }

    #[test]
    fn restarted_indexd_with_newer_epoch_is_not_rejected_by_prior_high_generation() {
        let k = |b: u8| vec![b; 8];
        let mut h_aa = [0u8; 32];
        h_aa[24..].copy_from_slice(&[0xaa; 8]);
        let mut h_bb = [0u8; 32];
        h_bb[24..].copy_from_slice(&[0xbb; 8]);

        let router_idx = PrefixIndex::new();
        let old_epoch = 1_000;
        let restarted_epoch = 2_000;

        // Before indexd restart, router received events at epoch=1000, gen=500.
        router_idx.apply_with_watermark(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![k(0xaa)],
                medium: None,
            }],
            100,
            old_epoch,
            500,
        );
        assert_eq!(router_idx.lookup("m", &[h_aa]), Some(0));

        // indexd restarts with epoch=2000 and gen resets to 1.
        // Its snapshot (!reset/m + !snapshot/m/1) and subsequent live events (gen=2)
        // must be accepted and must replace the old epoch=1000 entries.
        apply_control_with_watermark("!reset/m", restarted_epoch, 1, &[], &router_idx);
        let snap_payload = encode_stored_batch(&[k(0xbb)]);
        apply_control_with_watermark(
            "!snapshot/m/1",
            restarted_epoch,
            1,
            &snap_payload,
            &router_idx,
        );
        assert_eq!(
            router_idx.lookup("m", &[h_aa]),
            None,
            "restarted indexd reset at newer epoch must clear old epoch entries"
        );
        assert_eq!(
            router_idx.lookup("m", &[h_bb]),
            Some(1),
            "restarted indexd snapshot at gen=1 with newer epoch must be accepted"
        );

        // A removal at restarted_epoch, gen=2 must also take effect even though 2 < 500.
        router_idx.apply_with_watermark(
            "m",
            1,
            &[KvEvent::Removed {
                block_hashes: vec![k(0xbb)],
            }],
            200,
            restarted_epoch,
            2,
        );
        assert_eq!(router_idx.lookup("m", &[h_bb]), None);
    }

    #[test]
    fn multi_router_independent_epochs_do_not_reject_each_other() {
        let k = |b: u8| vec![b; 8];
        let mut h_aa = [0u8; 32];
        h_aa[24..].copy_from_slice(&[0xaa; 8]);

        let indexd_idx = PrefixIndex::new();
        let router_a_epoch = 1_700_000_001_000;
        let router_b_epoch = 1_700_000_002_000;

        // Router A has been running longer (gen=500) and forwards a Stored event at t=100.
        indexd_idx.apply_with_watermark(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![k(0xaa)],
                medium: None,
            }],
            100,
            router_a_epoch,
            500,
        );
        assert_eq!(indexd_idx.lookup("m", &[h_aa]), Some(0));

        // Router B (different epoch, lower gen=2) forwards a newer Removed event at t=200.
        // Because epochs differ, indexd must not reject Router B's gen=2 against Router A's gen=500.
        indexd_idx.apply_with_watermark(
            "m",
            0,
            &[KvEvent::Removed {
                block_hashes: vec![k(0xaa)],
            }],
            200,
            router_b_epoch,
            2,
        );
        assert_eq!(
            indexd_idx.lookup("m", &[h_aa]),
            None,
            "Router B's newer removal must not be dropped due to Router A's higher counter"
        );
    }

    /// Assessment P0-2 consistency: the shared runtime (used by BOTH the
    /// standalone router and the gateway's embedded mode) must ingest synthetic
    /// engine events end-to-end — PUB connect → SUB bind → index populated.
    #[tokio::test]
    async fn kv_runtime_ingests_synthetic_events() {
        use zeromq::{Socket, SocketSend};
        let index = PrefixIndex::new();
        std::env::set_var("NEBULA_ROUTER_KV_EVENTS_BIND", "tcp://127.0.0.1:35579");
        start_kv_runtime(index.clone());
        std::env::remove_var("NEBULA_ROUTER_KV_EVENTS_BIND");

        let stored = unhex(
            "93cb3ff8000000000000919cab426c6f636b53746f72656492c4203f9c7995b81c03a2ab8ad3046caf562b8e746d217638d4dc5c672504f0cb3b66c42045e3af14cac8deda96eeda45e3cea05b95ff309dc5c00e2073ecfc7a800e250cc0dc00206465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7fcc80cc81cc82cc8310c0a3475055c0c0c0c0c000",
        );
        let mut publisher = zeromq::PubSocket::with_options(zeromq::SocketOptions::default());
        publisher.connect("tcp://127.0.0.1:35579").await.unwrap();

        // PUB drops until the SUB's subscription propagates — retry-send.
        let mut seq = 0_u64..;
        for _ in 0..100 {
            let seq = seq.next().unwrap_or(0);
            let mut msg = zeromq::ZmqMessage::from("qwen25-7b/0");
            msg.push_back(bytes::Bytes::from(seq.to_be_bytes().to_vec()));
            msg.push_back(bytes::Bytes::from(stored.clone()));
            publisher.send(msg).await.unwrap();
            if index.entries() > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(
            index.entries() > 0,
            "shared kv runtime never ingested the synthetic event stream"
        );
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

    #[test]
    fn multi_owner_retains_prefill_and_decode_holders_and_scopes_lookup() {
        let idx = PrefixIndex::new();
        let h0: [u8; 32] = H0.try_into().unwrap();
        let h1: [u8; 32] = H1.try_into().unwrap();

        // Decode replica 1 stores [H0, H1]; later Prefill replica 0 re-stores [H0, H1].
        // A single-owner index would overwrite replica 1 with replica 0; the multi-owner
        // index retains both so candidate-scoped lookup in the Decode pool [1, 2] still
        // finds replica 1.
        idx.apply(
            "m",
            1,
            &[KvEvent::Stored {
                block_hashes: vec![H0.to_vec(), H1.to_vec()],
                medium: None,
            }],
            100,
        );
        idx.apply(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![H0.to_vec(), H1.to_vec()],
                medium: None,
            }],
            200,
        );

        assert_eq!(
            idx.lookup_in("m", &[h0, h1], Some(&[1, 2])),
            Some(1),
            "candidate-scoped lookup in Decode pool [1, 2] must find Decode replica 1 even after Prefill replica 0 also stored the blocks"
        );
        assert_eq!(
            idx.lookup_in("m", &[h0, h1], Some(&[0])),
            Some(0),
            "candidate-scoped lookup in Prefill pool [0] must still find Prefill replica 0"
        );
        assert_eq!(
            idx.lookup_in("m", &[h0, h1], Some(&[2])),
            None,
            "replica 2 does not hold the blocks"
        );

        // Removing H0, H1 from Prefill replica 0 must NOT evict Decode replica 1's ownership.
        idx.apply(
            "m",
            0,
            &[KvEvent::Removed {
                block_hashes: vec![H0.to_vec(), H1.to_vec()],
            }],
            300,
        );
        assert_eq!(idx.lookup("m", &[h0, h1]), Some(1));
    }

    #[test]
    fn legacy_8byte_frame_with_zero_epoch_does_not_impersonate_local_epoch() {
        let idx = PrefixIndex::new();
        let h0: [u8; 32] = H0.try_into().unwrap();
        assert_eq!(idx.current_generation(), 0);

        // Legacy 8-byte wire frame parses as (epoch=0, gen=999).
        let (epoch, gen) = parse_watermark(&999u64.to_be_bytes());
        assert_eq!((epoch, gen), (0, 999));

        idx.apply_with_watermark(
            "m",
            0,
            &[KvEvent::Stored {
                block_hashes: vec![H0.to_vec()],
                medium: None,
            }],
            100,
            epoch,
            gen,
        );
        // Must NOT advance local generation counter to 999.
        assert_eq!(idx.current_generation(), 0);
        assert_eq!(idx.lookup("m", &[h0]), Some(0));

        // Subsequent local event (self.epoch, gen=1) at newer timestamp (200 > 100)
        // must be accepted rather than dropped against the legacy frame's gen=999.
        idx.apply(
            "m",
            0,
            &[KvEvent::Removed {
                block_hashes: vec![H0.to_vec()],
            }],
            200,
        );
        assert_eq!(idx.current_generation(), 1);
        assert_eq!(idx.lookup("m", &[h0]), None);
    }

    #[test]
    fn ingest_and_stamp_frames_and_snapshot_for_with_gen_are_consistent() {
        let idx = PrefixIndex::new();
        let payload = encode_stored_batch(&[vec![0xaa; 8]]);
        let mut frames = vec![
            bytes::Bytes::from("m/0"),
            bytes::Bytes::from_static(&[0u8; 8]),
            bytes::Bytes::from(payload),
        ];
        let (epoch, gen) = idx.ingest_and_stamp_frames(&mut frames);
        assert_eq!(epoch, idx.epoch());
        assert_eq!(gen, 1);
        assert_eq!(parse_watermark(&frames[1]), (epoch, 1));

        let (snap_gen, snap) = idx.snapshot_for_with_gen("m");
        assert_eq!(snap_gen, 1);
        assert_eq!(snap, vec![(0, vec![vec![0xaa; 8]])]);
    }
}
