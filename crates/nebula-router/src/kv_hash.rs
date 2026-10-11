//! vLLM-compatible prefix block hashing (P1c).
//!
//! Replicates vLLM's `sha256_cbor` block hashing exactly:
//! `sha256(cbor2.dumps((parent_hash, tuple(token_ids), None), canonical=True))`
//! chained per full block (`block_size` tokens, default 16). Verified against
//! two real instances: v0.11 (default seed `vllm-none-hash`) and the deployed
//! v0.23.0 container on the test rig (`PYTHONHASHSEED=42`) — see tests.

use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// vLLM default KV cache block size (tokens per block).
pub const DEFAULT_BLOCK_SIZE: usize = 16;

/// Parent of the first block: `sha256_cbor(PYTHONHASHSEED)`.
///
/// vLLM v0.23 derives `NONE_HASH` from the engine's `PYTHONHASHSEED` env —
/// and **randomizes it per process when unset** (`os.urandom`), which makes
/// cross-replica prefix matching impossible. Deployments using prefix
/// affinity must therefore pin `PYTHONHASHSEED` on every engine (the node /
/// k8s-controller do this via `NEBULA_KV_HASH_SEED`, default "42") and the
/// router reads the same variable. v0.11 defaulted the seed to
/// "vllm-none-hash" when unset.
fn none_hash() -> &'static [u8; 32] {
    static NONE: OnceLock<[u8; 32]> = OnceLock::new();
    NONE.get_or_init(|| {
        let seed = std::env::var("NEBULA_KV_HASH_SEED").unwrap_or_else(|_| "42".to_string());
        sha256_cbor_text(&seed)
    })
}

/// The seed-derived parent hash for an explicit seed string (test hook).
#[cfg(test)]
fn none_hash_for_seed(seed: &str) -> [u8; 32] {
    sha256_cbor_text(seed)
}

fn put_uint(out: &mut Vec<u8>, n: u64) {
    match n {
        0..=23 => out.push(n as u8),
        24..=255 => {
            out.push(24);
            out.push(n as u8);
        }
        256..=65535 => {
            out.push(25);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        65536..=4294967295 => {
            out.push(26);
            out.extend_from_slice(&(n as u32).to_be_bytes());
        }
        _ => {
            out.push(27);
            out.extend_from_slice(&n.to_be_bytes());
        }
    }
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    // CBOR major type 2 (byte string), minimal length encoding.
    match b.len() {
        0..=23 => out.push(0x40 | b.len() as u8),
        24..=255 => {
            out.push(0x58);
            out.push(b.len() as u8);
        }
        _ => {
            out.push(0x59);
            out.extend_from_slice(&(b.len() as u16).to_be_bytes());
        }
    }
    out.extend_from_slice(b);
}

fn put_array_header(out: &mut Vec<u8>, n: usize) {
    match n {
        0..=23 => out.push(0x80 | n as u8),
        24..=255 => {
            out.push(0x98);
            out.push(n as u8);
        }
        _ => {
            out.push(0x99);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
    }
}

/// `sha256_cbor` over a CBOR text string (the seed hashing path).
fn sha256_cbor_text(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    let mut buf = Vec::with_capacity(b.len() + 1);
    // CBOR major type 3 (text string), minimal length encoding.
    match b.len() {
        0..=23 => buf.push(0x60 | b.len() as u8),
        24..=255 => {
            buf.push(0x78);
            buf.push(b.len() as u8);
        }
        _ => {
            buf.push(0x79);
            buf.extend_from_slice(&(b.len() as u16).to_be_bytes());
        }
    }
    buf.extend_from_slice(b);
    let mut h = Sha256::new();
    h.update(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.finalize());
    out
}

/// `sha256_cbor` over the 3-tuple `(parent, token_ids, extra_keys=None)`.
fn block_hash(parent: &[u8; 32], tokens: &[u32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(8 + tokens.len() * 3);
    put_array_header(&mut buf, 3);
    put_bytes(&mut buf, parent);
    put_array_header(&mut buf, tokens.len());
    for t in tokens {
        put_uint(&mut buf, *t as u64);
    }
    buf.push(0xf6); // extra_keys = null (no LoRA / mm / cache_salt)
    let mut h = Sha256::new();
    h.update(&buf);
    let mut out = [0u8; 32];
    out.copy_from_slice(&h.finalize());
    out
}

fn chain(token_ids: &[u32], block_size: usize, none: &[u8; 32]) -> Vec<[u8; 32]> {
    let block_size = block_size.max(1);
    let mut parent = *none;
    let mut hashes = Vec::with_capacity(token_ids.len() / block_size);
    for chunk in token_ids.chunks_exact(block_size) {
        let h = block_hash(&parent, chunk);
        parent = h;
        hashes.push(h);
    }
    hashes
}

/// Compute the chained block hashes for a token sequence (seed from
/// `NEBULA_KV_HASH_SEED`).
///
/// Only full blocks are hashed (partial trailing block is skipped), matching
/// vLLM's `request_block_hasher`. Longer shared prefixes → longer matching
/// hash chains.
pub fn compute_block_hashes(token_ids: &[u32], block_size: usize) -> Vec<[u8; 32]> {
    chain(token_ids, block_size, none_hash())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ground truth from real vLLM instances:
    // - v0.11 default seed "vllm-none-hash" (reference Python implementation)
    // - v0.23.0 deployed on the test rig, PYTHONHASHSEED=42, algo sha256_cbor
    //   (extracted by calling vllm.utils.hashing + kv_cache_utils directly
    //   inside the vllm/vllm-openai:v0.23.0 container)
    const NONE_V0011_HEX: &str = "9bd96a485ad84efdafb72ee48a1d7a69bcead0f8f0433173941b276b9581eef0";
    const NONE_V0023_42_HEX: &str =
        "9fa6eb003f14425eb9c8e6baacfbcaab26257357b829c7bfa74ffcc4f31c1780";
    const H0_42_HEX: &str = "3f9c7995b81c03a2ab8ad3046caf562b8e746d217638d4dc5c672504f0cb3b66";
    const H1_42_HEX: &str = "45e3af14cac8deda96eeda45e3cea05b95ff309dc5c00e2073ecfc7a800e250c";
    const H0_V0011_HEX: &str = "c78a554a1b648c084577886fa29e20b89d606beb26cafd7f90ba7482f31901bc";
    const H1_V0011_HEX: &str = "ce46e4cdf9f7c965a59bd3429627f27c1400980fc64be5e6706e1a9c55f39878";
    const HSMALL_V0011_HEX: &str =
        "584698135cd5ab0176f303fbf2976c62ea85c419e3997499bc66a175bdf3ada1";

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn hex32(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    #[test]
    fn none_hash_seed_vectors_match_real_vllm() {
        assert_eq!(hex(&none_hash_for_seed("vllm-none-hash")), NONE_V0011_HEX);
        assert_eq!(hex(&none_hash_for_seed("42")), NONE_V0023_42_HEX);
    }

    #[test]
    fn chain_matches_v023_deployed_instance() {
        // tokens 100..132, PYTHONHASHSEED=42 — captured from the deployed
        // vllm/vllm-openai:v0.23.0 container itself.
        let toks: Vec<u32> = (100..132).map(|i| i as u32).collect();
        let hs = chain(&toks, 16, &hex32(NONE_V0023_42_HEX));
        assert_eq!(hs.len(), 2);
        assert_eq!(hex(&hs[0]), H0_42_HEX);
        assert_eq!(hex(&hs[1]), H1_42_HEX);
    }

    #[test]
    fn chain_matches_reference_v011() {
        let toks: Vec<u32> = (100..132).map(|i| i as u32).collect();
        let hs = chain(&toks, 16, &hex32(NONE_V0011_HEX));
        assert_eq!(hs.len(), 2);
        assert_eq!(hex(&hs[0]), H0_V0011_HEX);
        assert_eq!(hex(&hs[1]), H1_V0011_HEX);
        // Same first block, different second block → first hash matches,
        // second diverges (chaining).
        let toks3: Vec<u32> = (100..116).chain(500..520).map(|i| i as u32).collect();
        let hs3 = chain(&toks3, 16, &hex32(NONE_V0011_HEX));
        assert_eq!(hex(&hs3[0]), H0_V0011_HEX);
        assert_ne!(hex(&hs3[1]), H1_V0011_HEX);
    }

    #[test]
    fn wide_token_ids_use_minimal_cbor_ints() {
        // [1, 2, 65535, 65536, 42] ×3 + [9] — exercises 1/2/3/5-byte int
        // encodings; vector from the reference implementation.
        let base = [1u32, 2, 65535, 65536, 42];
        let toks: Vec<u32> = base.iter().cycle().take(15).cloned().chain([9]).collect();
        let hs = chain(&toks, 16, &hex32(NONE_V0011_HEX));
        assert_eq!(hs.len(), 1);
        assert_eq!(hex(&hs[0]), HSMALL_V0011_HEX);
    }

    #[test]
    fn partial_trailing_block_skipped() {
        let toks: Vec<u32> = (0..35).collect();
        let none = none_hash_for_seed("42");
        assert_eq!(chain(&toks, 16, &none).len(), 2);
        assert_eq!(chain(&toks[..16], 16, &none).len(), 1);
        assert_eq!(chain(&toks[..15], 16, &none).len(), 0);
    }
}
