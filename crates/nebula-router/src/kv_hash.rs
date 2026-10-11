//! vLLM-compatible prefix block hashing (P1c).
//!
//! Replicates vLLM v0.11+ default `sha256_cbor` block hashing exactly:
//! `sha256(cbor2.dumps((parent_hash, tuple(token_ids), None), canonical=True))`
//! chained per full block (`block_size` tokens, default 16). The seed value
//! `NONE_HASH` is `sha256_cbor("vllm-none-hash")` (vLLM `init_none_hash`).
//! Test vectors cross-checked against the reference Python implementation.

use sha2::{Digest, Sha256};

/// vLLM default KV cache block size (tokens per block).
pub const DEFAULT_BLOCK_SIZE: usize = 16;

/// `sha256_cbor("vllm-none-hash")` — parent of the first block.
pub const NONE_HASH: [u8; 32] = [
    0x9b, 0xd9, 0x6a, 0x48, 0x5a, 0xd8, 0x4e, 0xfd, 0xaf, 0xb7, 0x2e, 0xe4, 0x8a, 0x1d, 0x7a, 0x69,
    0xbc, 0xea, 0xd0, 0xf8, 0xf0, 0x43, 0x31, 0x73, 0x94, 0x1b, 0x27, 0x6b, 0x95, 0x81, 0xee, 0xf0,
];

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

/// `sha256_cbor` over the 3-tuple `(parent, token_ids, extra_keys=None)`.
fn block_hash(parent: Option<&[u8; 32]>, tokens: &[u32]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(8 + tokens.len() * 3);
    put_array_header(&mut buf, 3);
    match parent {
        Some(p) => put_bytes(&mut buf, p),
        None => buf.push(0xf6), // null
    }
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

/// Compute the chained block hashes for a token sequence.
///
/// Only full blocks are hashed (partial trailing block is skipped), matching
/// vLLM's `request_block_hasher`. Longer shared prefixes → longer matching
/// hash chains.
pub fn compute_block_hashes(token_ids: &[u32], block_size: usize) -> Vec<[u8; 32]> {
    let block_size = block_size.max(1);
    let mut parent = None;
    let mut hashes = Vec::with_capacity(token_ids.len() / block_size);
    for chunk in token_ids.chunks_exact(block_size) {
        let h = block_hash(parent.as_ref(), chunk);
        parent = Some(h);
        hashes.push(h);
    }
    hashes
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors from the reference implementation:
    //   sha256(cbor2.dumps((parent, tuple(tokens), None), canonical=True))
    const H0_HEX: &str = "966914ae0d8904244961df6cdc5cafb4ba9e6666ff62b443bc93470105a0d13a";
    const H1_HEX: &str = "8e61e775793275e75e01b8dc44fe37ccd5ff1f0620d7f7319c95c0c97d416ea7";
    const HSMALL_HEX: &str = "d7943e0293bbe5401ea0742489e9a8122a935730869dc576ca16f24dac3f63df";

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn none_hash_matches_vllm_seed() {
        assert_eq!(
            hex(&NONE_HASH),
            "9bd96a485ad84efdafb72ee48a1d7a69bcead0f8f0433173941b276b9581eef0"
        );
    }

    #[test]
    fn debug_preimage() {
        let toks: Vec<u32> = (100..116).map(|i| i as u32).collect();
        let mut buf = Vec::new();
        put_array_header(&mut buf, 3);
        buf.push(0xf6);
        put_array_header(&mut buf, toks.len());
        for t in &toks {
            put_uint(&mut buf, *t as u64);
        }
        buf.push(0xf6);
        assert_eq!(
            buf.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "83f690186418651866186718681869186a186b186c186d186e186f1870187118721873f6"
        );
    }

    #[test]
    fn chain_matches_reference() {
        // tokens 100..132 (32 tokens → 2 blocks of 16)
        let toks: Vec<u32> = (100..132).map(|i| i as u32).collect();
        let hs = compute_block_hashes(&toks, 16);
        assert_eq!(hs.len(), 2);
        assert_eq!(hex(&hs[0]), H0_HEX);
        assert_eq!(hex(&hs[1]), H1_HEX);
        // Same first block, different second block → first hash matches,
        // second diverges (chaining).
        let toks3: Vec<u32> = (100..116).chain(500..520).map(|i| i as u32).collect();
        let hs3 = compute_block_hashes(&toks3, 16);
        assert_eq!(hex(&hs3[0]), H0_HEX);
        assert_ne!(hex(&hs3[1]), H1_HEX);
    }

    #[test]
    fn wide_token_ids_use_minimal_cbor_ints() {
        // [1, 2, 65535, 65536, 42] ×3 + [9] — exercises 1/2/3/5-byte int
        // encodings; vector from the reference implementation.
        let base = [1u32, 2, 65535, 65536, 42];
        let toks: Vec<u32> = base.iter().cycle().take(15).cloned().chain([9]).collect();
        let hs = compute_block_hashes(&toks, 16);
        assert_eq!(hs.len(), 1);
        assert_eq!(hex(&hs[0]), HSMALL_HEX);
    }

    #[test]
    fn partial_trailing_block_skipped() {
        let toks: Vec<u32> = (0..35).collect();
        assert_eq!(compute_block_hashes(&toks, 16).len(), 2);
        assert_eq!(compute_block_hashes(&toks[..16], 16).len(), 1);
        assert_eq!(compute_block_hashes(&toks[..15], 16).len(), 0);
    }
}
