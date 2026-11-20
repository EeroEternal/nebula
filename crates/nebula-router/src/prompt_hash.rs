//! Prompt → block hashes for prefix-affinity routing (P1c).
//!
//! Loads HuggingFace tokenizers from `NEBULA_ROUTER_TOKENIZER_DIR`
//! (`<dir>/<model_name>/tokenizer.json` or `<dir>/<model_uid>/…`), encodes
//! with special tokens enabled (matching vLLM's prompt tokenization) and
//! chains the vLLM-compatible block hashes.
//!
//! Chat bodies (`/v1/chat/completions`) are rendered with the model's
//! `chat_template` (from `tokenizer_config.json`, precompiled once per model)
//! via mini-jinja — including special tokens (`bos_token`, `eos_token`,
//! `pad_token`, `unk_token`), `tojson`, Python string methods, whitespace
//! control, and per-request `chat_template_kwargs` (such as `thinking` /
//! `enable_thinking` / `reasoning_effort` on DeepSeek-V4.1-Flash & Qwen3) —
//! then tokenized with `add_special_tokens=false`, matching
//! `tokenizer.apply_chat_template(messages, tokenize=True, **kwargs)`.

use std::sync::Arc;

use dashmap::DashMap;
use tokenizers::Tokenizer;

use crate::kv_hash::{compute_block_hashes, DEFAULT_BLOCK_SIZE};

struct CachedChatTemplate {
    env: minijinja::Environment<'static>,
    bos_token: Option<String>,
    eos_token: Option<String>,
    pad_token: Option<String>,
    unk_token: Option<String>,
}

pub struct PromptHasher {
    dir: Option<String>,
    cache: DashMap<String, Option<Arc<Tokenizer>>>,
    templates: DashMap<String, Option<Arc<CachedChatTemplate>>>,
    block_size: usize,
    model_block_sizes: DashMap<String, usize>,
    model_tokenizer_modes: DashMap<String, String>,
}

/// jinja2's `tojson`, which is what every HuggingFace chat template means by `| tojson`.
///
/// jinja2 renders `json.dumps(value)` (default separators, `ensure_ascii=True`) and then
/// escapes the HTML-sensitive characters. minijinja's own filter (feature `json`) emits
/// compact, unescaped JSON, so the rendered prompt would differ from the engine's
/// `apply_chat_template` byte for byte and none of the block hashes would ever match.
/// Key order also has to survive: Python dicts keep insertion order, so `serde_json` is
/// built with `preserve_order` in this crate and this function must not re-sort.
fn py_json_dumps(value: &minijinja::Value) -> String {
    let compact = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
    let mut out = String::with_capacity(compact.len() + 16);
    let mut in_str = false;
    let mut escaped = false;
    for ch in compact.chars() {
        if in_str {
            if escaped {
                escaped = false;
                out.push(ch);
                continue;
            }
            match ch {
                '\\' => {
                    escaped = true;
                    out.push(ch);
                }
                '"' => {
                    in_str = false;
                    out.push(ch);
                }
                '<' => out.push_str("\\u003c"),
                '>' => out.push_str("\\u003e"),
                '&' => out.push_str("\\u0026"),
                '\'' => out.push_str("\\u0027"),
                c if (c as u32) > 0x7f => push_py_unicode_escape(&mut out, c),
                c => out.push(c),
            }
        } else {
            match ch {
                '"' => {
                    in_str = true;
                    out.push(ch);
                }
                ':' => out.push_str(": "),
                ',' => out.push_str(", "),
                c => out.push(c),
            }
        }
    }
    out
}

fn push_py_unicode_escape(out: &mut String, ch: char) {
    let mut buf = [0u16; 2];
    for unit in ch.encode_utf16(&mut buf) {
        out.push_str(&format!("\\u{unit:04x}"));
    }
}

/// Python `str` methods that real chat templates call.
///
/// Qwen3 / DeepSeek templates call `.startswith("<tool_response>")` to detect tool results,
/// and minijinja exposes no string methods at all, so a tool-bearing body fails to render:
/// `chat_ids` returns `None`, prefix affinity silently degrades to `no_signal`, and the
/// Tool-Pause pin has no token prefix to pin. Deliberately a subset - extend it from a real
/// template that fails, not speculatively.
fn py_str_method(
    value: &minijinja::Value,
    method: &str,
    args: &[minijinja::Value],
) -> Option<Result<minijinja::Value, minijinja::Error>> {
    let s = value.as_str()?;
    let arg = |i: usize| args.get(i).and_then(|v| v.as_str()).map(str::to_string);
    let text = |t: String| Some(Ok(minijinja::Value::from(t)));
    let truth = |b: bool| Some(Ok(minijinja::Value::from(b)));
    let strip = |left: bool, right: bool| -> String {
        let chars = arg(0);
        match (left, right, chars) {
            (true, true, Some(chars)) => s.trim_matches(|c| chars.contains(c)).to_string(),
            (true, false, Some(chars)) => s.trim_start_matches(|c| chars.contains(c)).to_string(),
            (false, true, Some(chars)) => s.trim_end_matches(|c| chars.contains(c)).to_string(),
            (true, true, None) => s.trim().to_string(),
            (true, false, None) => s.trim_start().to_string(),
            _ => s.trim_end().to_string(),
        }
    };
    match method {
        "startswith" | "endswith" => {
            let starts = method == "startswith";
            let test = |p: &str| {
                if starts {
                    s.starts_with(p)
                } else {
                    s.ends_with(p)
                }
            };
            let v = args.first()?;
            // A plain string first: iterating a string yields chars, not prefixes.
            if let Some(p) = v.as_str() {
                return truth(test(p));
            }
            let it = v.try_iter().ok()?;
            truth(
                it.filter_map(|x| x.as_str().map(str::to_string))
                    .any(|p| test(&p)),
            )
        }
        "strip" => text(strip(true, true)),
        "lstrip" => text(strip(true, false)),
        "rstrip" => text(strip(false, true)),
        "lower" => text(s.to_lowercase()),
        "upper" => text(s.to_uppercase()),
        "capitalize" => {
            let mut it = s.chars();
            text(match it.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), it.as_str().to_lowercase()),
                None => String::new(),
            })
        }
        "replace" => {
            let from = arg(0)?;
            let to = arg(1).unwrap_or_default();
            text(match args.get(2).and_then(|v| v.as_i64()) {
                Some(limit) if limit >= 0 => s.replacen(&from, &to, limit as usize),
                _ => s.replace(&from, &to),
            })
        }
        "split" | "rsplit" => {
            let parts: Vec<String> = match arg(0).filter(|p| !p.is_empty()) {
                // Whitespace splitting (Python's `str.split()`); maxsplit is not used by
                // any template we run, so it is ignored rather than approximated.
                None => s.split_whitespace().map(str::to_string).collect(),
                Some(sep) => s.split(sep.as_str()).map(str::to_string).collect(),
            };
            Some(Ok(minijinja::Value::from_serialize(parts)))
        }
        "find" | "rfind" => {
            let sub = arg(0)?;
            let byte = if method == "find" {
                s.find(&sub)
            } else {
                s.rfind(&sub)
            };
            // Python reports a character index, -1 on miss.
            let idx = byte
                .map(|b| s[..b].chars().count() as i64)
                .unwrap_or(-1);
            Some(Ok(minijinja::Value::from(idx)))
        }
        "count" => {
            let sub = arg(0)?;
            let n = if sub.is_empty() {
                0
            } else {
                s.matches(sub.as_str()).count()
            };
            Some(Ok(minijinja::Value::from(n as u64)))
        }
        "join" => {
            let items: Vec<String> = args
                .first()
                .and_then(|v| v.try_iter().ok())
                .map(|it| {
                    it.filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            text(items.join(s))
        }
        _ => Some(Err(minijinja::Error::new(
            minijinja::ErrorKind::UnknownMethod,
            format!(
                "chat template used unsupported string method '{method}' (extend py_str_method in prompt_hash.rs)"
            ),
        ))),
    }
}

/// Built-in fallback template for `--tokenizer-mode deepseek_v41` when a repackaged
/// model directory omits `chat_template` in `tokenizer_config.json`.
const DEEPSEEK_V41_FALLBACK_TEMPLATE: &str = r#"{{- bos_token if bos_token is defined else '<｜begin▁of▁sentence｜>' -}}
{%- for message in messages -%}
{%- if message['role'] == 'system' and loop.first -%}
{{- message['content'] -}}
{%- elif message['role'] == 'user' -%}
{{- '<｜User｜>' + message['content'] -}}
{%- elif message['role'] == 'assistant' -%}
{{- '<｜Assistant｜>' + message['content'] + (eos_token if eos_token is defined else '<｜end▁of▁sentence｜>') -}}
{%- endif -%}
{%- endfor -%}
{%- if add_generation_prompt -%}
{%- if thinking is defined and not thinking -%}
{{- '<｜Assistant｜></think>' -}}
{%- else -%}
{{- '<｜Assistant｜><think>\n' -}}
{%- endif -%}
{%- endif -%}"#;

const CHATML_FALLBACK_TEMPLATE: &str = r#"{%- for message in messages -%}
{{- '<|im_start|>' + message['role'] + '\n' + message['content'] + '<|im_end|>\n' -}}
{%- endfor -%}
{%- if add_generation_prompt -%}
{{- '<|im_start|>assistant\n' -}}
{%- endif -%}"#;

/// Precompiled jinja2 environment (`'static`) as chat templates expect it:
/// `tojson` filter plus Python string methods, compiled once per model.
fn chat_env_owned(tpl: String) -> Result<minijinja::Environment<'static>, minijinja::Error> {
    let mut env = minijinja::Environment::new();
    env.add_filter("tojson", |v: minijinja::Value| py_json_dumps(&v));
    env.set_unknown_method_callback(|_, value, method, args| {
        py_str_method(value, method, args).unwrap_or_else(|| {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::UnknownMethod,
                format!("unknown method: {method}"),
            ))
        })
    });
    env.add_template_owned("chat", tpl)?;
    Ok(env)
}

#[cfg(test)]
fn chat_env(tpl: &str) -> Result<minijinja::Environment<'static>, minijinja::Error> {
    chat_env_owned(tpl.to_string())
}

fn extract_special_token(cfg: &serde_json::Value, key: &str) -> Option<String> {
    match cfg.get(key)? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(obj) => obj
            .get("content")
            .and_then(|c| c.as_str())
            .map(str::to_string),
        _ => None,
    }
}

fn build_cached_template(
    tpl: String,
    bos_token: Option<String>,
    eos_token: Option<String>,
    pad_token: Option<String>,
    unk_token: Option<String>,
) -> Option<Arc<CachedChatTemplate>> {
    let env = chat_env_owned(tpl).ok()?;
    Some(Arc::new(CachedChatTemplate {
        env,
        bos_token,
        eos_token,
        pad_token,
        unk_token,
    }))
}

fn build_chat_render_context(
    cached: &CachedChatTemplate,
    body: &serde_json::Value,
) -> Option<minijinja::Value> {
    let messages = body.get("messages")?;
    let mut ctx_map = serde_json::Map::new();
    if let Some(ref bos) = cached.bos_token {
        ctx_map.insert("bos_token".into(), serde_json::Value::String(bos.clone()));
    }
    if let Some(ref eos) = cached.eos_token {
        ctx_map.insert("eos_token".into(), serde_json::Value::String(eos.clone()));
    }
    if let Some(ref pad) = cached.pad_token {
        ctx_map.insert("pad_token".into(), serde_json::Value::String(pad.clone()));
    }
    if let Some(ref unk) = cached.unk_token {
        ctx_map.insert("unk_token".into(), serde_json::Value::String(unk.clone()));
    }
    ctx_map.insert(
        "add_generation_prompt".into(),
        serde_json::Value::Bool(true),
    );

    // Unpack per-request `chat_template_kwargs` (e.g. DeepSeek-V4.1-Flash `thinking: false`,
    // Qwen3 `enable_thinking: false`, `reasoning_effort`) matching vLLM / transformers.
    if let Some(kwargs) = body
        .get("chat_template_kwargs")
        .or_else(|| body.pointer("/extra_body/chat_template_kwargs"))
        .and_then(|v| v.as_object())
    {
        for (k, v) in kwargs {
            ctx_map.insert(k.clone(), v.clone());
        }
    }
    if !ctx_map.contains_key("reasoning_effort") {
        if let Some(re) = body.get("reasoning_effort") {
            if !re.is_null() {
                ctx_map.insert("reasoning_effort".into(), re.clone());
            }
        }
    }

    // Core request fields always win over arbitrary kwargs.
    ctx_map.insert("messages".into(), messages.clone());
    ctx_map.insert(
        "tools".into(),
        body.get("tools")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    );
    ctx_map.insert(
        "documents".into(),
        body.get("documents")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    );
    Some(minijinja::Value::from_serialize(serde_json::Value::Object(
        ctx_map,
    )))
}

impl PromptHasher {
    pub fn from_env() -> Self {
        Self {
            dir: std::env::var("NEBULA_ROUTER_TOKENIZER_DIR").ok(),
            cache: DashMap::new(),
            templates: DashMap::new(),
            block_size: DEFAULT_BLOCK_SIZE,
            model_block_sizes: DashMap::new(),
            model_tokenizer_modes: DashMap::new(),
        }
    }

    /// Register a model-specific prefix block granularity (e.g., 16 for standard
    /// GQA, 64/128 for MLA / DeepSeek / TokenSpeed prefix granularity). Values of `0`
    /// are ignored so division/chunking is always well-defined.
    pub fn set_model_block_size(&self, model_uid: &str, block_size: usize) {
        if block_size > 0 {
            self.model_block_sizes
                .insert(model_uid.to_string(), block_size);
        }
    }

    pub fn clear_model_block_size(&self, model_uid: &str) {
        self.model_block_sizes.remove(model_uid);
    }

    /// Register a model's declared `tokenizer_mode` (`"deepseek_v41"`, `"chatml"`, `"auto"`).
    pub fn set_model_tokenizer_mode(&self, model_uid: &str, mode: &str) {
        let trimmed = mode.trim();
        if !trimmed.is_empty() {
            self.model_tokenizer_modes
                .insert(model_uid.to_string(), trimmed.to_string());
            // Clear any cached `None` template entry so fallback resolution re-runs.
            self.templates.retain(|k, _| !k.contains(model_uid));
        }
    }

    pub fn clear_model_tokenizer_mode(&self, model_uid: &str) {
        self.model_tokenizer_modes.remove(model_uid);
    }

    /// True if an explicit per-model `block_size` was configured from model spec.
    pub fn has_model_block_size(&self, model_uid: &str) -> bool {
        self.model_block_sizes
            .get(model_uid)
            .map(|e| *e.value() > 0)
            .unwrap_or(false)
    }

    /// Effective prefix block size for `model_uid` (defaults to `DEFAULT_BLOCK_SIZE` = 16).
    pub fn block_size_for(&self, model_uid: &str) -> usize {
        self.model_block_sizes
            .get(model_uid)
            .map(|e| *e.value())
            .filter(|&bs| bs > 0)
            .unwrap_or(self.block_size)
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
        #[cfg(test)]
        if let Some(hit) = self
            .cache
            .get(&format!("__test__/{model_uid}/tokenizer.json"))
        {
            return hit.value().clone();
        }
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

    /// The model's precompiled chat template & special tokens (`tokenizer_config.json`),
    /// falling back to the declared `tokenizer_mode` (`deepseek_v41` / `chatml`) when
    /// `tokenizer_config.json` omits `chat_template`.
    fn chat_template_for(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
    ) -> Option<Arc<CachedChatTemplate>> {
        #[cfg(test)]
        if let Some(hit) = self
            .templates
            .get(&format!("__test__/{model_uid}/tokenizer_config.json"))
        {
            return hit.value().clone();
        }
        let mode = self
            .model_tokenizer_modes
            .get(model_uid)
            .map(|e| e.value().clone());
        let paths = self.candidate_paths(model_uid, model_name, "tokenizer_config.json");
        let cache_key = match (paths.first(), mode.as_deref()) {
            (Some(p), Some(m)) => format!("{p}#{m}"),
            (Some(p), None) => p.clone(),
            (None, _) => return None,
        };
        if let Some(hit) = self.templates.get(&cache_key) {
            return hit.value().clone();
        }

        let mut bos_token = None;
        let mut eos_token = None;
        let mut pad_token = None;
        let mut unk_token = None;
        let mut tpl_str: Option<String> = None;

        for p in &paths {
            let Ok(file) = std::fs::File::open(p) else {
                continue;
            };
            let Ok(v) = serde_json::from_reader::<_, serde_json::Value>(file) else {
                continue;
            };
            bos_token = extract_special_token(&v, "bos_token").or(bos_token);
            eos_token = extract_special_token(&v, "eos_token").or(eos_token);
            pad_token = extract_special_token(&v, "pad_token").or(pad_token);
            unk_token = extract_special_token(&v, "unk_token").or(unk_token);
            if let Some(raw_tpl) = v.get("chat_template") {
                tpl_str = match raw_tpl {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Array(a) => a
                        .iter()
                        .find(|t| t.get("name").and_then(|n| n.as_str()) == Some("default"))
                        .or_else(|| a.first())
                        .and_then(|t| t.get("template"))
                        .and_then(|t| t.as_str())
                        .map(str::to_string),
                    _ => None,
                };
                if tpl_str.is_some() {
                    break;
                }
            }
        }

        if tpl_str.is_none() {
            tpl_str = match mode.as_deref() {
                Some("deepseek_v41") => Some(DEEPSEEK_V41_FALLBACK_TEMPLATE.to_string()),
                Some("chatml") => Some(CHATML_FALLBACK_TEMPLATE.to_string()),
                _ => None,
            };
        }

        let loaded = tpl_str
            .and_then(|tpl| build_cached_template(tpl, bos_token, eos_token, pad_token, unk_token));
        self.templates.insert(cache_key, loaded.clone());
        loaded
    }

    /// Chain vLLM-compatible block hashes from already-tokenized `ids` using
    /// the default block size (`DEFAULT_BLOCK_SIZE`).
    pub fn hashes_from_ids(&self, ids: &[u32]) -> Vec<[u8; 32]> {
        compute_block_hashes(ids, self.block_size)
    }

    /// Chain block hashes from `ids` using `model_uid`'s configured block size
    /// (`block_size_for(model_uid)`, defaulting to 16).
    pub fn hashes_from_ids_for_model(&self, model_uid: &str, ids: &[u32]) -> Vec<[u8; 32]> {
        compute_block_hashes(ids, self.block_size_for(model_uid))
    }

    /// Tokenize `prompt` and chain its block hashes. `None` when no tokenizer
    /// is available for the model (caller falls back visibly).
    #[cfg(test)]
    pub fn prompt_hashes(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        prompt: &str,
    ) -> Option<Vec<[u8; 32]>> {
        let ids = self.prompt_ids(model_uid, model_name, prompt)?;
        Some(self.hashes_from_ids_for_model(model_uid, &ids))
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

    /// Render a chat body (transformers-compatible subset + special tokens +
    /// `chat_template_kwargs`) using the model's precompiled template and return
    /// its token ids (used by both block hashing and LMCache controller lookup).
    pub fn chat_ids(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        body: &serde_json::Value,
    ) -> Option<Vec<u32>> {
        let tk = self.tokenizer_for(model_uid, model_name)?;
        let cached = self.chat_template_for(model_uid, model_name)?;
        let ctx = build_chat_render_context(&cached, body)?;
        let rendered = cached.env.get_template("chat").ok()?.render(ctx).ok()?;
        // apply_chat_template tokenizes the rendered string with
        // add_special_tokens=false (special tokens come from the template).
        let enc = tk.encode(rendered.as_str(), false).ok()?;
        Some(enc.get_ids().to_vec())
    }

    /// Render a chat body (transformers-compatible subset) and chain its
    /// block hashes using the model's configured block size.
    #[cfg(test)]
    pub fn chat_hashes(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        body: &serde_json::Value,
    ) -> Option<Vec<[u8; 32]>> {
        let ids = self.chat_ids(model_uid, model_name, body)?;
        Some(self.hashes_from_ids_for_model(model_uid, &ids))
    }

    #[cfg(test)]
    pub(crate) fn seed_for_test(&self, model_uid: &str, tk: Tokenizer, tpl: &str) {
        self.seed_for_test_with_tokens(model_uid, tk, tpl, None, None);
    }

    #[cfg(test)]
    pub(crate) fn seed_for_test_with_tokens(
        &self,
        model_uid: &str,
        tk: Tokenizer,
        tpl: &str,
        bos_token: Option<&str>,
        eos_token: Option<&str>,
    ) {
        self.cache.insert(
            format!("__test__/{model_uid}/tokenizer.json"),
            Some(Arc::new(tk)),
        );
        let cached = build_cached_template(
            tpl.to_string(),
            bos_token.map(str::to_string),
            eos_token.map(str::to_string),
            None,
            None,
        );
        self.templates.insert(
            format!("__test__/{model_uid}/tokenizer_config.json"),
            cached,
        );
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

    /// jinja2 renders `json.dumps` defaults (space after `,`/`:`) and escapes HTML-sensitive
    /// characters; the engine's `apply_chat_template` does the same, so a mismatch here means
    /// every tool-bearing prefix hashes differently from the KV the engine actually stored.
    #[test]
    fn tojson_matches_jinja2_htmlsafe_json_dumps() {
        let v = minijinja::Value::from_serialize(serde_json::json!({
            "type": "function",
            "html": "<x>",
            "quote": "it's",
            "zh": "你好",
            "n": 1,
            "list": [true, null],
        }));
        assert_eq!(
            py_json_dumps(&v),
            r#"{"type": "function", "html": "\u003cx\u003e", "quote": "it\u0027s", "zh": "\u4f60\u597d", "n": 1, "list": [true, null]}"#
        );
    }

    const QWEN3ISH: &str = r#"{%- if tools %}{{- 'TOOLS:' }}{%- for tool in tools %}{{- '\n' + (tool|tojson) }}{%- endfor %}{{- '\n' }}{%- endif %}{%- for m in messages %}{%- if m.content is string %}{{- '[' + m.role + ']' }}{%- if not (m.content.startswith('<tool_response>') and m.content.endswith('</tool_response>')) %}{{- m.content }}{%- endif %}{{- '\n' }}{%- endif %}{%- endfor %}"#;

    /// The Qwen3 (and DeepSeek) templates combine `| tojson` with `.startswith/.endswith`;
    /// minijinja has neither, which used to make every tool-bearing chat body unhashable.
    #[test]
    fn qwen3_style_template_renders_tools_and_skips_tool_responses() {
        let env = chat_env(QWEN3ISH).unwrap();
        let ctx = minijinja::Value::from_serialize(serde_json::json!({
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "tool", "content": "<tool_response>18C</tool_response>"},
                {"role": "tool", "content": "plain tool text"},
            ],
            "tools": [{"type": "function", "function": {"name": "get_weather"}}],
        }));
        let out = env.get_template("chat").unwrap().render(ctx).unwrap();
        assert_eq!(
            out,
            "TOOLS:\n{\"type\": \"function\", \"function\": {\"name\": \"get_weather\"}}\n\
             [system]sys\n[tool]\n[tool]plain tool text\n"
        );
    }

    /// An unmodelled method must fail loudly: the operator sees why a model's prefix signal
    /// disappeared instead of a silent `no_signal` fallback.
    #[test]
    fn unsupported_string_method_names_itself() {
        let env = chat_env("{{ m.content.frobnicate() }}").unwrap();
        let ctx = minijinja::Value::from_serialize(serde_json::json!({
            "m": {"content": "x"},
        }));
        let err = env
            .get_template("chat")
            .unwrap()
            .render(ctx)
            .unwrap_err()
            .to_string();
        assert!(err.contains("frobnicate"), "{err}");
    }

    #[test]
    fn no_env_dir_yields_none() {
        let h = PromptHasher {
            dir: None,
            cache: DashMap::new(),
            templates: DashMap::new(),
            block_size: 16,
            model_block_sizes: DashMap::new(),
            model_tokenizer_modes: DashMap::new(),
        };
        assert!(h.prompt_hashes("m", Some("org/model"), "hello").is_none());
        assert!(h
            .chat_hashes("m", None, &serde_json::json!({"messages": []}))
            .is_none());
    }

    #[test]
    fn deepseek_v41_bos_token_and_thinking_kwargs_align_render() {
        let cached = build_cached_template(
            DEEPSEEK_V41_FALLBACK_TEMPLATE.to_string(),
            Some("<｜begin▁of▁sentence｜>".to_string()),
            Some("<｜end▁of▁sentence｜>".to_string()),
            None,
            None,
        )
        .expect("compile deepseek_v41 template");

        // Default thinking=true -> opens `<think>\n`
        let body_think = serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "13*17?"}
            ]
        });
        let ctx_think = build_chat_render_context(&cached, &body_think).unwrap();
        let out_think = cached
            .env
            .get_template("chat")
            .unwrap()
            .render(ctx_think)
            .unwrap();
        assert_eq!(
            out_think,
            "<｜begin▁of▁sentence｜>You are a helpful assistant.<｜User｜>13*17?<｜Assistant｜><think>\n"
        );

        // Per-request `chat_template_kwargs: {"thinking": false}` -> closes `</think>`,
        // while preserving the exact `<｜begin▁of▁sentence｜>` token-0 alignment on the prefix.
        let body_no_think = serde_json::json!({
            "messages": [
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "13*17?"}
            ],
            "chat_template_kwargs": {"thinking": false}
        });
        let ctx_no_think = build_chat_render_context(&cached, &body_no_think).unwrap();
        let out_no_think = cached
            .env
            .get_template("chat")
            .unwrap()
            .render(ctx_no_think)
            .unwrap();
        assert_eq!(
            out_no_think,
            "<｜begin▁of▁sentence｜>You are a helpful assistant.<｜User｜>13*17?<｜Assistant｜></think>"
        );

        // AddedToken object extraction (`{"content": "..."}`) matches string form.
        let cfg = serde_json::json!({
            "bos_token": {"__type": "AddedToken", "content": "<｜begin▁of▁sentence｜>"},
            "eos_token": "<｜end▁of▁sentence｜>"
        });
        assert_eq!(
            extract_special_token(&cfg, "bos_token").as_deref(),
            Some("<｜begin▁of▁sentence｜>")
        );
        assert_eq!(
            extract_special_token(&cfg, "eos_token").as_deref(),
            Some("<｜end▁of▁sentence｜>")
        );
    }

    #[test]
    fn per_model_block_size_adjusts_hash_granularity() {
        let h = PromptHasher::from_env();
        let ids: Vec<u32> = (0..128).collect();
        // Default 16 tokens/block -> 8 blocks
        assert_eq!(h.block_size_for("deepseek-v3"), 16);
        assert_eq!(h.hashes_from_ids_for_model("deepseek-v3", &ids).len(), 8);

        // Configure 64 tokens/block (MLA / TokenSpeed prefix_granularity) -> 2 blocks
        h.set_model_block_size("deepseek-v3", 64);
        assert_eq!(h.block_size_for("deepseek-v3"), 64);
        assert_eq!(h.hashes_from_ids_for_model("deepseek-v3", &ids).len(), 2);

        h.clear_model_block_size("deepseek-v3");
        assert_eq!(h.block_size_for("deepseek-v3"), 16);
    }
}
