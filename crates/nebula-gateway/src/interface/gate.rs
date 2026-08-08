//! C5 tooling capability gate.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use nebula_common::{tool_calling_for_engine, ModelSpec, SupportLevel};
use nebula_meta::MetaStore;
use serde_json::{json, Value};

/// True when the request body asks for tools / tool_choice.
pub fn request_has_tools(payload: &Value) -> bool {
    if let Some(tools) = payload.get("tools") {
        match tools {
            Value::Array(a) if !a.is_empty() => return true,
            Value::Object(o) if !o.is_empty() => return true,
            _ => {}
        }
    }
    if let Some(tc) = payload.get("tool_choice") {
        match tc {
            Value::String(s) if !s.is_empty() && s != "none" => return true,
            Value::Object(o) if !o.is_empty() => return true,
            _ => {}
        }
    }
    false
}

/// Stable OpenAI-shaped 400 for unsupported tooling.
pub fn unsupported_tooling_response(engine_type: Option<&str>) -> Response {
    let detail = match engine_type {
        Some(et) if !et.is_empty() => {
            format!("engine '{et}' does not support tools / tool_choice")
        }
        _ => "tools / tool_choice is not supported for this model".to_string(),
    };
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": {
                "message": detail,
                "type": "unsupported",
                "code": "tool_calling_unsupported"
            }
        })),
    )
        .into_response()
}

/// Look up model spec engine_type from etcd (`/models/{uid}/spec`).
pub async fn resolve_engine_type_for_model(
    store: &dyn MetaStore,
    model: Option<&str>,
) -> Option<String> {
    let model = model?.trim();
    if model.is_empty() {
        return None;
    }
    let key = format!("/models/{model}/spec");
    match store.get(&key).await {
        Ok(Some((data, _))) => {
            let spec: ModelSpec = serde_json::from_slice(&data).ok()?;
            spec.engine_type
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.to_ascii_lowercase())
        }
        _ => None,
    }
}

/// If the body requests tools, enforce `tool_calling` capability.
/// Returns `Some(Response)` to short-circuit; `None` to continue.
pub async fn check_tooling_gate(
    store: &dyn MetaStore,
    payload: &Value,
    model: Option<&str>,
) -> Option<Response> {
    if !request_has_tools(payload) {
        return None;
    }
    let engine_type = resolve_engine_type_for_model(store, model).await;
    let level = tool_calling_for_engine(engine_type.as_deref());
    match level {
        SupportLevel::Supported => None,
        SupportLevel::Unknown => {
            tracing::warn!(
                model = model.unwrap_or(""),
                engine_type = engine_type.as_deref().unwrap_or(""),
                "tool_calling capability unknown; allowing request"
            );
            None
        }
        SupportLevel::Unsupported => {
            Some(unsupported_tooling_response(engine_type.as_deref()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_tools_array_and_tool_choice() {
        assert!(!request_has_tools(&json!({"model": "m"})));
        assert!(request_has_tools(&json!({
            "tools": [{"type": "function", "function": {"name": "f"}}]
        })));
        assert!(request_has_tools(&json!({"tool_choice": "auto"})));
        assert!(!request_has_tools(&json!({"tool_choice": "none"})));
        assert!(!request_has_tools(&json!({"tools": []})));
    }

    #[test]
    fn unsupported_response_type_stable() {
        let resp = unsupported_tooling_response(Some("mock"));
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
