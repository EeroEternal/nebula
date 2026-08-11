use serde::{Deserialize, Serialize};

/// Propagated request context across Gateway → Router (and into traces/audit).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ExecutionContext {
    pub request_id: String,
    pub session_id: Option<String>,
    pub tenant_id: Option<String>,
    pub priority: Option<i32>,
    pub deadline_ms: Option<u64>,
    pub budget_tokens: Option<u32>,
    /// Opt-in replica pin (`x-nebula-replica-id`); Router honors when set.
    pub pinned_replica_id: Option<u32>,
}

pub const HEADER_REQUEST_ID: &str = "x-nebula-request-id";
pub const HEADER_REPLICA_ID: &str = "x-nebula-replica-id";
pub const HEADER_SESSION_ID: &str = "x-session-id";
pub const HEADER_TENANT_ID: &str = "x-nebula-tenant-id";
pub const HEADER_PRIORITY: &str = "x-nebula-priority";
pub const HEADER_DEADLINE_MS: &str = "x-nebula-deadline-ms";
pub const HEADER_BUDGET_TOKENS: &str = "x-nebula-budget-tokens";

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

    let request_id = header_str(HEADER_REQUEST_ID)
        .unwrap_or_else(|| format!("req_{}", uuid::Uuid::new_v4()));

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

    ExecutionContext {
        request_id,
        session_id: header_str(HEADER_SESSION_ID),
        tenant_id,
        priority,
        deadline_ms,
        budget_tokens,
        pinned_replica_id,
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
    }
}
