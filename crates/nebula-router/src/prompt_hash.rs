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

/// jinja2 environment as chat templates expect it: `tojson` plus Python string methods.
fn chat_env(tpl: &str) -> Result<minijinja::Environment<'_>, minijinja::Error> {
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
    env.add_template("chat", tpl)?;
    Ok(env)
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

    /// The model's chat template (`tokenizer_config.json`). A list of named
    /// templates resolves to the one named "default" (transformers rule).
    fn chat_template_for(&self, model_uid: &str, model_name: Option<&str>) -> Option<Arc<String>> {
        #[cfg(test)]
        if let Some(hit) = self
            .templates
            .get(&format!("__test__/{model_uid}/tokenizer_config.json"))
        {
            return hit.value().clone();
        }
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

    /// Chain vLLM-compatible block hashes from already-tokenized `ids`.
    pub fn hashes_from_ids(&self, ids: &[u32]) -> Vec<[u8; 32]> {
        compute_block_hashes(ids, self.block_size)
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
        Some(self.hashes_from_ids(&ids))
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

    /// Render a chat body (transformers-compatible subset) and return its
    /// token ids (used by both block hashing and LMCache controller lookup).
    pub fn chat_ids(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        body: &serde_json::Value,
    ) -> Option<Vec<u32>> {
        let messages = body.get("messages")?;
        let tk = self.tokenizer_for(model_uid, model_name)?;
        let tpl = self.chat_template_for(model_uid, model_name)?;

        let env = chat_env(tpl.as_str()).ok()?;
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
        Some(enc.get_ids().to_vec())
    }

    /// Render a chat body (transformers-compatible subset) and chain its
    /// block hashes. The context mirrors `apply_chat_template`:
    /// `messages`, `add_generation_prompt`, `tools` when present.
    #[cfg(test)]
    pub fn chat_hashes(
        &self,
        model_uid: &str,
        model_name: Option<&str>,
        body: &serde_json::Value,
    ) -> Option<Vec<[u8; 32]>> {
        let ids = self.chat_ids(model_uid, model_name, body)?;
        Some(self.hashes_from_ids(&ids))
    }

    #[cfg(test)]
    pub(crate) fn seed_for_test(&self, model_uid: &str, tk: Tokenizer, tpl: &str) {
        self.cache.insert(
            format!("__test__/{model_uid}/tokenizer.json"),
            Some(Arc::new(tk)),
        );
        self.templates.insert(
            format!("__test__/{model_uid}/tokenizer_config.json"),
            Some(Arc::new(tpl.to_string())),
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
        };
        assert!(h.prompt_hashes("m", Some("org/model"), "hello").is_none());
        assert!(h
            .chat_hashes("m", None, &serde_json::json!({"messages": []}))
            .is_none());
    }
}
