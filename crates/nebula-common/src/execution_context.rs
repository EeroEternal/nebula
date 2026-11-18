use serde::{Deserialize, Serialize};

const MAX_HINT_HEADER_LEN: usize = 1024;
const MAX_HINT_PREFIX_KEY_LEN: usize = 128;
pub const MAX_SESSION_ID_LEN: usize = 128;

/// Source from which a request's `session_id` was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionIdSource {
    /// Standard `x-session-id` request header.
    PrimaryHeader,
    /// Operator-configured fallback header (`NEBULA_SESSION_ID_HEADERS` or `auto` preset).
    FallbackHeader,
    /// Operator-configured JSON body dot-path (`NEBULA_SESSION_ID_BODY_FIELDS` or `auto` preset).
    BodyField,
}

impl SessionIdSource {
    pub fn as_metric_label(self) -> &'static str {
        match self {
            Self::PrimaryHeader => "header",
            Self::FallbackHeader => "fallback_header",
            Self::BodyField => "body",
        }
    }
}

/// Generic, opt-in Session ID extraction policy for callers that cannot set `x-session-id`.
///
/// Off by default (`fallback_headers` and `body_fields` empty) so only `x-session-id` is read
/// unless the operator enables `NEBULA_SESSION_ID_EXTRACT=auto` or specifies explicit
/// `NEBULA_SESSION_ID_HEADERS` / `NEBULA_SESSION_ID_BODY_FIELDS`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionExtractConfig {
    pub fallback_headers: Vec<String>,
    pub body_fields: Vec<Vec<String>>,
}

impl SessionExtractConfig {
    pub const AUTO_HEADERS: &'static [&'static str] =
        &["x-conversation-id", "x-thread-id", "openai-conversation-id"];
    pub const AUTO_BODY_FIELDS: &'static [&'static str] = &[
        "session_id",
        "conversation_id",
        "metadata.session_id",
        "extra_body.session_id",
        "user",
    ];

    pub fn auto_preset() -> Self {
        Self::from_lists(
            Self::AUTO_HEADERS.iter().copied(),
            Self::AUTO_BODY_FIELDS.iter().copied(),
        )
    }

    pub fn from_lists<'a>(
        headers: impl IntoIterator<Item = &'a str>,
        body_paths: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut fallback_headers = Vec::new();
        for raw in headers {
            let name = raw.trim().to_ascii_lowercase();
            if !name.is_empty() && name != HEADER_SESSION_ID && !fallback_headers.contains(&name) {
                fallback_headers.push(name);
            }
        }
        let mut body_fields = Vec::new();
        for raw in body_paths {
            let segs: Vec<String> = raw
                .trim()
                .split('.')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            if !segs.is_empty() && !body_fields.contains(&segs) {
                body_fields.push(segs);
            }
        }
        Self {
            fallback_headers,
            body_fields,
        }
    }

    pub fn from_env() -> Self {
        let mode = std::env::var("NEBULA_SESSION_ID_EXTRACT")
            .ok()
            .map(|v| v.trim().to_ascii_lowercase());
        if matches!(mode.as_deref(), Some("0" | "false" | "off" | "none")) {
            return Self::default();
        }
        let auto = matches!(mode.as_deref(), Some("auto" | "1" | "true" | "on"));
        let mut headers: Vec<String> = Vec::new();
        let mut paths: Vec<String> = Vec::new();

        if let Ok(raw_h) = std::env::var("NEBULA_SESSION_ID_HEADERS") {
            headers.extend(
                raw_h
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            );
        } else if auto {
            headers.extend(Self::AUTO_HEADERS.iter().map(|s| (*s).to_string()));
        }

        if let Ok(raw_b) = std::env::var("NEBULA_SESSION_ID_BODY_FIELDS") {
            paths.extend(
                raw_b
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            );
        } else if auto {
            paths.extend(Self::AUTO_BODY_FIELDS.iter().map(|s| (*s).to_string()));
        }

        Self::from_lists(
            headers.iter().map(String::as_str),
            paths.iter().map(String::as_str),
        )
    }

    pub fn is_fallback_enabled(&self) -> bool {
        !self.fallback_headers.is_empty() || !self.body_fields.is_empty()
    }
}

fn sanitize_session_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_SESSION_ID_LEN
        || trimmed.chars().any(|c| c.is_control())
    {
        return None;
    }
    Some(trimmed.to_string())
}

/// Extract a sanitized `session_id` according to `x-session-id` first, then optional
/// configured fallback headers, then optional configured JSON body dot-paths.
pub fn extract_session_id_with_config(
    headers: &axum::http::HeaderMap,
    body: Option<&[u8]>,
    cfg: &SessionExtractConfig,
) -> Option<(String, SessionIdSource)> {
    if let Some(sid) = headers
        .get(HEADER_SESSION_ID)
        .and_then(|v| v.to_str().ok())
        .and_then(sanitize_session_id)
    {
        return Some((sid, SessionIdSource::PrimaryHeader));
    }

    for name in &cfg.fallback_headers {
        if let Some(sid) = headers
            .get(name.as_str())
            .and_then(|v| v.to_str().ok())
            .and_then(sanitize_session_id)
        {
            return Some((sid, SessionIdSource::FallbackHeader));
        }
    }

    if cfg.body_fields.is_empty() {
        return None;
    }
    let body = body?;
    let first_non_ws = body.iter().copied().find(|b| !b.is_ascii_whitespace())?;
    if first_non_ws != b'{' {
        return None;
    }
    let doc: serde_json::Value = serde_json::from_slice(body).ok()?;
    extract_session_id_from_json(&doc, cfg).map(|sid| (sid, SessionIdSource::BodyField))
}

/// Extract a sanitized `session_id` from a parsed JSON body using `cfg.body_fields`.
pub fn extract_session_id_from_json(
    doc: &serde_json::Value,
    cfg: &SessionExtractConfig,
) -> Option<String> {
    for path in &cfg.body_fields {
        let mut cur = doc;
        let mut matched = true;
        for seg in path {
            match cur.get(seg.as_str()) {
                Some(next) => cur = next,
                None => {
                    matched = false;
                    break;
                }
            }
        }
        if !matched {
            continue;
        }
        let candidate = match cur {
            serde_json::Value::String(s) => sanitize_session_id(s),
            serde_json::Value::Number(n) => sanitize_session_id(&n.to_string()),
            _ => None,
        };
        if let Some(sid) = candidate {
            return Some(sid);
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct InferenceHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer_session_affinity: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer_prefix_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer_low_latency: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefer_low_cost: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<u64>,
}

/// Propagated request context across Gateway → Router (and into traces/audit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct ExecutionContext {
    pub request_id: String,
    pub session_id: Option<String>,
    pub tenant_id: Option<String>,
    pub priority: Option<i32>,
    pub deadline_ms: Option<u64>,
    pub budget_tokens: Option<u32>,
    /// Opt-in replica pin (`x-nebula-replica-id`); Router honors when set.
    pub pinned_replica_id: Option<u32>,
    /// Weak semantic hint (advisory only; Router may ignore).
    pub inference_hint: Option<InferenceHint>,
    /// `true` means hint source is privileged and sanitized by Gateway.
    pub hint_trusted: bool,
}

pub const HEADER_REQUEST_ID: &str = "x-nebula-request-id";
pub const HEADER_REPLICA_ID: &str = "x-nebula-replica-id";
pub const HEADER_SESSION_ID: &str = "x-session-id";
pub const HEADER_TENANT_ID: &str = "x-nebula-tenant-id";
pub const HEADER_PRIORITY: &str = "x-nebula-priority";
pub const HEADER_DEADLINE_MS: &str = "x-nebula-deadline-ms";
pub const HEADER_BUDGET_TOKENS: &str = "x-nebula-budget-tokens";
pub const HEADER_INFERENCE_HINT: &str = "x-nebula-inference-hint";
pub const HEADER_HINT_TRUSTED: &str = "x-nebula-hint-trusted";
/// Internal trust token header sent to downstream engines (e.g. PowerLLM) so they can bypass duplicate auth.
pub const HEADER_INTERNAL_AUTH: &str = "x-nebula-internal-auth";

pub fn parse_and_sanitize_inference_hint(raw: &str) -> Option<InferenceHint> {
    if raw.len() > MAX_HINT_HEADER_LEN {
        return None;
    }
    let mut hint: InferenceHint = serde_json::from_str(raw).ok()?;
    if let Some(prefix) = hint.prefer_prefix_key.as_mut() {
        *prefix = prefix.trim().to_string();
        if prefix.is_empty() || prefix.len() > MAX_HINT_PREFIX_KEY_LEN {
            hint.prefer_prefix_key = None;
        }
    }
    if hint.prefer_session_affinity.is_none()
        && hint.prefer_prefix_key.is_none()
        && hint.prefer_low_latency.is_none()
        && hint.prefer_low_cost.is_none()
        && hint.expires_at_ms.is_none()
    {
        return None;
    }
    Some(hint)
}

/// Build context from inbound headers (Router / Gateway).
///
/// When `auth_tenant_id` is provided (from token binding), it **overrides** any
/// client-supplied `x-nebula-tenant-id` so tenants cannot spoof identity.
pub fn build_execution_context(
    headers: &axum::http::HeaderMap,
    auth_tenant_id: Option<&str>,
    default_priority: Option<i32>,
) -> ExecutionContext {
    let header_str = |name: &str| -> Option<String> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let request_id =
        header_str(HEADER_REQUEST_ID).unwrap_or_else(|| format!("req_{}", uuid::Uuid::new_v4()));

    let tenant_id = auth_tenant_id
        .map(|s| s.to_string())
        .or_else(|| header_str(HEADER_TENANT_ID));

    let priority = headers
        .get(HEADER_PRIORITY)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i32>().ok())
        .or(default_priority);

    let deadline_ms = headers
        .get(HEADER_DEADLINE_MS)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let budget_tokens = headers
        .get(HEADER_BUDGET_TOKENS)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok());

    let pinned_replica_id = header_str(HEADER_REPLICA_ID).and_then(|s| s.parse::<u32>().ok());
    let inference_hint =
        header_str(HEADER_INFERENCE_HINT).and_then(|s| parse_and_sanitize_inference_hint(&s));
    let hint_trusted = headers
        .get(HEADER_HINT_TRUSTED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "1")
        .unwrap_or(false);

    ExecutionContext {
        request_id,
        session_id: header_str(HEADER_SESSION_ID).and_then(|s| sanitize_session_id(&s)),
        tenant_id,
        priority,
        deadline_ms,
        budget_tokens,
        pinned_replica_id,
        inference_hint,
        hint_trusted,
    }
}

/// Inject ExecutionContext into outbound proxy headers (Gateway → Router).
pub fn inject_execution_context(headers: &mut axum::http::HeaderMap, ctx: &ExecutionContext) {
    insert_header(headers, HEADER_REQUEST_ID, &ctx.request_id);
    if let Some(ref s) = ctx.session_id {
        insert_header(headers, HEADER_SESSION_ID, s);
    }
    if let Some(ref t) = ctx.tenant_id {
        insert_header(headers, HEADER_TENANT_ID, t);
    }
    if let Some(p) = ctx.priority {
        insert_header(headers, HEADER_PRIORITY, &p.to_string());
    }
    if let Some(d) = ctx.deadline_ms {
        insert_header(headers, HEADER_DEADLINE_MS, &d.to_string());
    }
    if let Some(b) = ctx.budget_tokens {
        insert_header(headers, HEADER_BUDGET_TOKENS, &b.to_string());
    }
    if let Some(r) = ctx.pinned_replica_id {
        insert_header(headers, HEADER_REPLICA_ID, &r.to_string());
    }
    if let Some(ref hint) = ctx.inference_hint {
        if let Ok(s) = serde_json::to_string(hint) {
            insert_header(headers, HEADER_INFERENCE_HINT, &s);
        }
    }
    insert_header(
        headers,
        HEADER_HINT_TRUSTED,
        if ctx.hint_trusted { "1" } else { "0" },
    );
}

fn insert_header(headers: &mut axum::http::HeaderMap, name: &'static str, value: &str) {
    if let (Ok(n), Ok(v)) = (
        axum::http::HeaderName::from_bytes(name.as_bytes()),
        axum::http::HeaderValue::from_str(value),
    ) {
        headers.insert(n, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn auth_tenant_overrides_header() {
        let mut h = HeaderMap::new();
        h.insert(HEADER_TENANT_ID, "spoof".parse().unwrap());
        h.insert(HEADER_PRIORITY, "5".parse().unwrap());
        let ctx = build_execution_context(&h, Some("real-tenant"), Some(1));
        assert_eq!(ctx.tenant_id.as_deref(), Some("real-tenant"));
        assert_eq!(ctx.priority, Some(5));
    }

    #[test]
    fn inject_roundtrip_fields() {
        let ctx = ExecutionContext {
            request_id: "req_1".into(),
            session_id: Some("s1".into()),
            tenant_id: Some("t1".into()),
            priority: Some(2),
            deadline_ms: Some(99),
            budget_tokens: Some(1000),
            pinned_replica_id: None,
            inference_hint: Some(InferenceHint {
                prefer_session_affinity: Some(true),
                prefer_prefix_key: Some("k".into()),
                prefer_low_latency: Some(true),
                prefer_low_cost: None,
                expires_at_ms: Some(100),
            }),
            hint_trusted: true,
        };
        let mut out = HeaderMap::new();
        inject_execution_context(&mut out, &ctx);
        assert_eq!(
            out.get(HEADER_TENANT_ID).and_then(|v| v.to_str().ok()),
            Some("t1")
        );
        assert_eq!(
            out.get(HEADER_BUDGET_TOKENS).and_then(|v| v.to_str().ok()),
            Some("1000")
        );
        assert_eq!(
            out.get(HEADER_HINT_TRUSTED).and_then(|v| v.to_str().ok()),
            Some("1")
        );
    }

    #[test]
    fn sanitize_hint_filters_invalid_prefix() {
        let raw = r#"{"prefer_prefix_key":"   ","prefer_low_latency":true}"#;
        let hint = parse_and_sanitize_inference_hint(raw).expect("valid hint");
        assert_eq!(hint.prefer_prefix_key, None);
        assert_eq!(hint.prefer_low_latency, Some(true));
    }

    #[test]
    fn session_extractor_respects_precedence_and_sanitizes() {
        let cfg = SessionExtractConfig::auto_preset();
        let body = br#"{"model":"ds","metadata":{"session_id":"meta-sess-1"},"user":"u-fallback"}"#;

        // 1. Default config is strictly off unless x-session-id is present.
        let empty_headers = HeaderMap::new();
        assert_eq!(
            extract_session_id_with_config(
                &empty_headers,
                Some(body),
                &SessionExtractConfig::default()
            ),
            None
        );

        // 2. Body dot-path extraction in configured order (`metadata.session_id` before `user`).
        assert_eq!(
            extract_session_id_with_config(&empty_headers, Some(body), &cfg),
            Some(("meta-sess-1".to_string(), SessionIdSource::BodyField))
        );

        // 3. Fallback header beats body field.
        let mut fb_headers = HeaderMap::new();
        fb_headers.insert("x-conversation-id", "conv-42".parse().unwrap());
        assert_eq!(
            extract_session_id_with_config(&fb_headers, Some(body), &cfg),
            Some(("conv-42".to_string(), SessionIdSource::FallbackHeader))
        );

        // 4. Primary `x-session-id` beats everything.
        fb_headers.insert(HEADER_SESSION_ID, "primary-99".parse().unwrap());
        assert_eq!(
            extract_session_id_with_config(&fb_headers, Some(body), &cfg),
            Some(("primary-99".to_string(), SessionIdSource::PrimaryHeader))
        );

        // 5. Oversized or control-char session IDs are rejected, falling through to `user`.
        let bad_body = format!(
            r#"{{"metadata":{{"session_id":"{}"}},"user":"u-valid"}}"#,
            "x".repeat(MAX_SESSION_ID_LEN + 1)
        );
        assert_eq!(
            extract_session_id_with_config(&empty_headers, Some(bad_body.as_bytes()), &cfg),
            Some(("u-valid".to_string(), SessionIdSource::BodyField))
        );
    }
}
