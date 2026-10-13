//! Prompt → block hashes for prefix-affinity routing (P1c).
//!
//! Loads HuggingFace tokenizers from `NEBULA_ROUTER_TOKENIZER_DIR`
//! (`<dir>/<model_name>/tokenizer.json` or `<dir>/<model_uid>/…`), encodes
//! with special tokens enabled (matching vLLM's prompt tokenization) and
//! chains the vLLM-compatible block hashes.
//!
//! Chat bodies (`/v1/chat/completions`) are rendered with the model's
//! `chat_template` (from `tokenizer_config.json`) via mini-jinja — the same
//! constructs transformers uses (`tojson`, `loop.first`, whitespace control)
//! — then tokenized with `add_special_tokens=false`, exactly matching
//! `tokenizer.apply_chat_template(messages, tokenize=True)`.
//!
//! shortcut: `chat_template_kwargs` per-request overrides and exotic
//! template extensions are not rendered v1; mismatched renders just miss
//! (visible via the strategy fallback counters). Upgrade if a model family
//! needs them.

use std::sync::Arc;

use dashmap::DashMap;
use tokenizers::Tokenizer;

use crate::kv_hash::{compute_block_hashes, DEFAULT_BLOCK_SIZE};

pub struct PromptHasher {
    dir: Option<String>,
    cache: DashMap<String, Option<Arc<Tokenizer>>>,
    templates: DashMap<String, Option<Arc<String>>>,
    block_size: usize,
}

impl PromptHasher {
    pub fn from_env() -> Self {
        Self {
            dir: std::env::var("NEBULA_ROUTER_TOKENIZER_DIR").ok(),
            cache: DashMap::new(),
            templates: DashMap::new(),
            block_size: DEFAULT_BLOCK_SIZE,
        }
    }

    fn candidate_paths(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        file: &str,
    ) -> Vec<String> {
        let Some(dir) = self.dir.as_deref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(n) = model_name {
            out.push(format!("{dir}/{n}/{file}"));
        }
        out.push(format!("{dir}/{model_uid}/{file}"));
        out
    }

    fn tokenizer_for(&self, model_uid: &str, model_name: Option<&str>) -> Option<Arc<Tokenizer>> {
        let paths = self.candidate_paths(model_uid, model_name, "tokenizer.json");
        let key = paths.first()?.clone();
        if let Some(hit) = self.cache.get(&key) {
            return hit.value().clone();
        }
        let loaded = paths
            .iter()
            .find_map(|p| Tokenizer::from_file(p).ok())
            .map(Arc::new);
        self.cache.insert(key, loaded.clone());
        loaded
    }

    /// The model's chat template (`tokenizer_config.json`). A list of named
    /// templates resolves to the one named "default" (transformers rule).
    fn chat_template_for(&self, model_uid: &str, model_name: Option<&str>) -> Option<Arc<String>> {
        let paths = self.candidate_paths(model_uid, model_name, "tokenizer_config.json");
        let key = paths.first()?.clone();
        if let Some(hit) = self.templates.get(&key) {
            return hit.value().clone();
        }
        let loaded = paths.iter().find_map(|p| {
            let v: serde_json::Value =
                serde_json::from_reader(std::fs::File::open(p).ok()?).ok()?;
            match v.get("chat_template")? {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Array(a) => a
                    .iter()
                    .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("default"))
                    .or_else(|| a.first())
                    .and_then(|t| t.get("template"))?
                    .as_str()
                    .map(str::to_string),
                _ => None,
            }
        });
        let loaded = loaded.map(Arc::new);
        self.templates.insert(key, loaded.clone());
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

    /// Token ids for the raw prompt (used by the LMCache controller lookup).
    pub fn prompt_ids(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        prompt: &str,
    ) -> Option<Vec<u32>> {
        let tk = self.tokenizer_for(model_uid, model_name)?;
        let enc = tk.encode(prompt, true).ok()?;
        Some(enc.get_ids().to_vec())
    }

    /// Render a chat body (transformers-compatible subset) and chain its
    /// block hashes. The context mirrors `apply_chat_template`:
    /// `messages`, `add_generation_prompt`, `tools` when present.
    pub fn chat_hashes(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        body: &serde_json::Value,
    ) -> Option<Vec<[u8; 32]>> {
        let messages = body.get("messages")?;
        let tk = self.tokenizer_for(model_uid, model_name)?;
        let tpl = self.chat_template_for(model_uid, model_name)?;

        let mut env = minijinja::Environment::new();
        env.add_template("chat", tpl.as_str()).ok()?;
        // Context mirrors transformers apply_chat_template; chat_template_kwargs
        // per-request overrides are not supported v1 (shortcut).
        let ctx = minijinja::Value::from_serialize(serde_json::json!({
            "messages": messages,
            "add_generation_prompt": true,
            "tools": body.get("tools").cloned().unwrap_or(serde_json::Value::Null),
            "documents": body.get("documents")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        }));
        let rendered = env.get_template("chat").ok()?.render(ctx).ok()?;
        // apply_chat_template tokenizes the rendered string with
        // add_special_tokens=false (special tokens come from the template).
        let enc = tk.encode(rendered.as_str(), false).ok()?;
        Some(compute_block_hashes(enc.get_ids(), self.block_size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QWENISH: &str = r#"{%- if messages[0]['role'] == 'system' %}
{{- '<|im_start|>system\n' + messages[0]['content'] + '<|im_end|>\n' }}
{%- else %}
{{- '<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n' }}
{%- endif %}
{%- for message in messages %}
{%- if (message.role == "user") or (message.role == "system" and not loop.first) %}
{{- '<|im_start|>' + message.role + '\n' + message.content + '<|im_end|>\n' }}
{%- endif %}
{%- endfor %}
{%- if add_generation_prompt %}
{{- '<|im_start|>assistant\n' }}
{%- endif %}"#;

    #[test]
    fn chat_render_matches_transformers_shape() {
        let mut env = minijinja::Environment::new();
        env.add_template("t", QWENISH).unwrap();
        let body = serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are terse."},
                {"role": "user", "content": "hello"}
            ]
        });
        let ctx = minijinja::Value::from_serialize(serde_json::json!({
            "messages": body["messages"],
            "add_generation_prompt": true,
        }));
        let out = env.get_template("t").unwrap().render(ctx).unwrap();
        assert_eq!(
            out,
            "<|im_start|>system\nYou are terse.<|im_end|>\n\
             <|im_start|>user\nhello<|im_end|>\n\
             <|im_start|>assistant\n"
        );
    }

    #[test]
    fn no_env_dir_yields_none() {
        let h = PromptHasher {
            dir: None,
            cache: DashMap::new(),
            templates: DashMap::new(),
            block_size: 16,
        };
        assert!(h.prompt_hashes("m", Some("org/model"), "hello").is_none());
        assert!(h
            .chat_hashes("m", None, &serde_json::json!({"messages": []}))
            .is_none());
    }
}
