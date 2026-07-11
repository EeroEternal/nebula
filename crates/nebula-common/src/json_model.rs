//! Lightweight JSON helpers for hot-path model header injection (avoid full DOM parse).

/// Peek a top-level `"model":"..."` string without building a DOM.
pub fn peek_json_model_field(body: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(body).ok()?;
    let key = "\"model\"";
    let idx = s.find(key)?;
    let mut after = s[idx + key.len()..].trim_start();
    if !after.starts_with(':') {
        return None;
    }
    after = after[1..].trim_start();
    if !after.starts_with('"') {
        return None;
    }
    let rest = &after[1..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let next = chars.next()?;
                out.push(next);
            }
            '"' => return Some(out),
            other => out.push(other),
        }
    }
    None
}

/// Replace the first `"model":"..."` string value (byte copy, no DOM).
pub fn rewrite_json_model_field(body: &[u8], new_model: &str) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(body).ok()?;
    let key = "\"model\"";
    let key_idx = s.find(key)?;
    let after_key = key_idx + key.len();
    let tail = &s[after_key..];
    let colon_rel = tail.find(':')?;
    let after_colon = &tail[colon_rel + 1..];
    let quote_rel = after_colon.find('"')?;
    let value_start = after_key + colon_rel + 1 + quote_rel + 1;
    let bytes = s.as_bytes();
    let mut i = value_start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i = i.saturating_add(2);
            }
            b'"' => {
                let mut out = Vec::with_capacity(body.len() + new_model.len());
                out.extend_from_slice(&body[..value_start]);
                out.extend_from_slice(new_model.as_bytes());
                out.extend_from_slice(&body[i..]);
                return Some(out);
            }
            _ => i += 1,
        }
    }
    None
}

/// Trusted internal header: raw model id/name from the client body (Gateway → Router).
pub const HEADER_NEBULA_MODEL: &str = "x-nebula-model";
/// Trusted internal header: resolved model_uid (optional).
pub const HEADER_NEBULA_MODEL_UID: &str = "x-nebula-model-uid";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peek_and_rewrite_model() {
        let body = br#"{"model":"gemma4_31b","messages":[{"role":"user","content":"hi"}]}"#;
        assert_eq!(peek_json_model_field(body).as_deref(), Some("gemma4_31b"));
        let rewritten = rewrite_json_model_field(body, "gemma-4-31b-it").unwrap();
        assert_eq!(
            peek_json_model_field(&rewritten).as_deref(),
            Some("gemma-4-31b-it")
        );
        assert!(rewritten.windows(2).any(|w| w == b"hi"));
    }

    #[test]
    fn peek_model_not_first_field() {
        let body = br#"{"stream":true,"model":"m1","input":"x"}"#;
        assert_eq!(peek_json_model_field(body).as_deref(), Some("m1"));
    }

    #[test]
    fn peek_missing_model_returns_none() {
        assert!(peek_json_model_field(br#"{"input":"x"}"#).is_none());
        assert!(rewrite_json_model_field(br#"{"input":"x"}"#, "m").is_none());
    }

    #[test]
    fn rewrite_preserves_large_suffix() {
        let suffix = "x".repeat(64 * 1024);
        let body = format!(r#"{{"model":"old","input":"{suffix}"}}"#);
        let rewritten = rewrite_json_model_field(body.as_bytes(), "new").unwrap();
        assert_eq!(peek_json_model_field(&rewritten).as_deref(), Some("new"));
        let as_str = std::str::from_utf8(&rewritten).unwrap();
        assert!(as_str.contains("\"input\":\""));
        assert!(as_str.ends_with(&format!("{suffix}\"}}")));
        assert_eq!(rewritten.len(), body.len()); // "old" and "new" same length
    }
}
