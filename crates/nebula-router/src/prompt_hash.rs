//! Prompt → block hashes for prefix-affinity routing (P1c).
//!
//! Loads HuggingFace tokenizers from `NEBULA_ROUTER_TOKENIZER_DIR`
//! (`<dir>/<model_name>/tokenizer.json` or `<dir>/<model_uid>/…`), encodes
//! with special tokens enabled (matching vLLM's prompt tokenization) and
//! chains the vLLM-compatible block hashes.
//!
//! shortcut: only raw `prompt`-style bodies hash exactly; chat bodies go
//! through the engine's chat template and are not hashed in v1 (they fall
//! back visibly via the strategy fallback counters). Upgrade when a chat
//! template renderer is available in-router.

use std::sync::Arc;

use dashmap::DashMap;
use tokenizers::Tokenizer;

use crate::kv_hash::{compute_block_hashes, DEFAULT_BLOCK_SIZE};

pub struct PromptHasher {
    dir: Option<String>,
    cache: DashMap<String, Option<Arc<Tokenizer>>>,
    block_size: usize,
}

impl PromptHasher {
    pub fn from_env() -> Self {
        Self {
            dir: std::env::var("NEBULA_ROUTER_TOKENIZER_DIR").ok(),
            cache: DashMap::new(),
            block_size: DEFAULT_BLOCK_SIZE,
        }
    }

    fn tokenizer_for(&self, model_uid: &str, model_name: Option<&str>) -> Option<Arc<Tokenizer>> {
        let dir = self.dir.as_deref()?;
        let candidates = [
            model_name.map(|n| format!("{dir}/{n}/tokenizer.json")),
            Some(format!("{dir}/{model_uid}/tokenizer.json")),
        ];
        let key = candidates
            .iter()
            .flatten()
            .next()
            .cloned()
            .unwrap_or_else(|| dir.to_string());
        if let Some(hit) = self.cache.get(&key) {
            return hit.value().clone();
        }
        let loaded = candidates
            .iter()
            .flatten()
            .find_map(|p| Tokenizer::from_file(p).ok())
            .map(Arc::new);
        self.cache.insert(key, loaded.clone());
        loaded
    }

    /// Tokenize `prompt` and chain its block hashes. `None` when no tokenizer
    /// is available for the model (caller falls back visibly).
    pub fn prompt_hashes(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        prompt: &str,
    ) -> Option<Vec<[u8; 32]>> {
        let tk = self.tokenizer_for(model_uid, model_name)?;
        let enc = tk.encode(prompt, true).ok()?;
        Some(compute_block_hashes(enc.get_ids(), self.block_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // shortcut: no tokenizer fixture is vendored; from_env without the env
    // var must degrade to None (routing falls back, never breaks).
    #[test]
    fn no_env_dir_yields_none() {
        let h = PromptHasher {
            dir: None,
            cache: DashMap::new(),
            block_size: 16,
        };
        assert!(h.prompt_hashes("m", Some("org/model"), "hello").is_none());
    }
}
