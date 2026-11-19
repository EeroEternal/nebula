//! C5 tooling and `/v1/score` capability gates.

use axum::{http::StatusCode, response::Response};
use nebula_common::{score_api_for_engine, tool_calling_for_engine, ModelSpec, SupportLevel};
use nebula_meta::MetaStore;
use serde_json::Value;

use crate::interface::errors::openai_error_response;

/// Outcome of the C5 tooling capability check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolingGate {
    Allow,
    AllowWithWarn,
    Deny,
}

/// Pure decision from `SupportLevel` (no I/O) — unit-testable C5 core.
pub fn tooling_gate_decision(level: SupportLevel) -> ToolingGate {
    match level {
        SupportLevel::Supported => ToolingGate::Allow,
        SupportLevel::Unknown => ToolingGate::AllowWithWarn,
        SupportLevel::Unsupported => ToolingGate::Deny,
    }
}

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
    openai_error_response(
        StatusCode::BAD_REQUEST,
        "unsupported",
        "tool_calling_unsupported",
        detail,
    )
}

/// Stable OpenAI-shaped 400 for unsupported `/v1/score`.
pub fn unsupported_score_response(engine_type: Option<&str>) -> Response {
    let detail = match engine_type {
        Some(et) if !et.is_empty() => {
            format!("engine '{et}' does not support /v1/score")
        }
        _ => "/v1/score is not supported for this model".to_string(),
    };
    openai_error_response(
        StatusCode::BAD_REQUEST,
        "unsupported",
        "score_api_unsupported",
        detail,
    )
}

/// Cold-start fallback: look up model spec `engine_type` directly from etcd (`/models/{uid}/spec`),
/// resolving an omitted `engine_type` on an existing `ModelSpec` to `DEFAULT_ENGINE_TYPE` (`"vllm"`).
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
            Some(nebula_common::resolve_engine_type(
                spec.engine_type.as_deref(),
            ))
        }
        _ => None,
    }
}

/// Look up model spec `engine_type` from the in-memory snapshot (hot-path Rule #8).
/// Once `/models/` has completed initial sync, a miss is authoritative and never queries etcd.
pub async fn resolve_engine_type_from_snapshot(
    snapshot: &crate::sync::GatewayMetaSnapshot,
    store: &dyn MetaStore,
    model: Option<&str>,
) -> Option<String> {
    let model = model?.trim();
    if model.is_empty() {
        return None;
    }
    if let Some(engine) = snapshot.get_engine_type(model).await {
        return Some(engine);
    }
    if snapshot.is_models_synced() {
        return None;
    }
    resolve_engine_type_for_model(store, Some(model)).await
}

/// If the body requests tools, enforce `tool_calling` capability.
/// Returns `Some(Response)` to short-circuit; `None` to continue.
pub async fn check_tooling_gate(
    snapshot: &crate::sync::GatewayMetaSnapshot,
    store: &dyn MetaStore,
    payload: &Value,
    model: Option<&str>,
) -> Option<Response> {
    if !request_has_tools(payload) {
        return None;
    }
    let engine_type = resolve_engine_type_from_snapshot(snapshot, store, model).await;
    let level = tool_calling_for_engine(engine_type.as_deref());
    match tooling_gate_decision(level) {
        ToolingGate::Allow => None,
        ToolingGate::AllowWithWarn => {
            tracing::warn!(
                model = model.unwrap_or(""),
                engine_type = engine_type.as_deref().unwrap_or(""),
                "tool_calling capability unknown; allowing request"
            );
            None
        }
        ToolingGate::Deny => Some(unsupported_tooling_response(engine_type.as_deref())),
    }
}

/// Enforce `score_api` capability on `POST /v1/score` (Batch 5.4).
pub async fn check_score_gate(
    snapshot: &crate::sync::GatewayMetaSnapshot,
    store: &dyn MetaStore,
    model: Option<&str>,
) -> Option<Response> {
    let engine_type = resolve_engine_type_from_snapshot(snapshot, store, model).await;
    let level = score_api_for_engine(engine_type.as_deref());
    match tooling_gate_decision(level) {
        ToolingGate::Allow => None,
        ToolingGate::AllowWithWarn => {
            tracing::warn!(
                model = model.unwrap_or(""),
                engine_type = engine_type.as_deref().unwrap_or(""),
                "score_api capability unknown; allowing request"
            );
            None
        }
        ToolingGate::Deny => Some(unsupported_score_response(engine_type.as_deref())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn tooling_gate_decision_three_states() {
        assert_eq!(
            tooling_gate_decision(SupportLevel::Supported),
            ToolingGate::Allow
        );
        assert_eq!(
            tooling_gate_decision(SupportLevel::Unknown),
            ToolingGate::AllowWithWarn
        );
        assert_eq!(
            tooling_gate_decision(SupportLevel::Unsupported),
            ToolingGate::Deny
        );
    }

    #[test]
    fn unsupported_response_type_stable() {
        let resp = unsupported_tooling_response(Some("mock"));
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let score_resp = unsupported_score_response(Some("mock"));
        assert_eq!(score_resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn synced_snapshot_never_falls_back_to_store_on_miss_or_alias_hit() {
        use nebula_common::{ModelConfig, ModelSource};
        use nebula_meta::MemoryMetaStore;

        // Seed a model ONLY in the MetaStore, NOT in the synced snapshot.
        let store = MemoryMetaStore::new();
        let store_only_spec = ModelSpec {
            model_uid: "store-only".into(),
            model_name: "store-only".into(),
            model_source: ModelSource::Local,
            model_path: None,
            engine_type: Some("mock".into()),
            docker_image: None,
            config: None,
            labels: Default::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: None,
        };
        store
            .put(
                "/models/store-only/spec",
                serde_json::to_vec(&store_only_spec).unwrap(),
                None,
            )
            .await
            .unwrap();

        let snap = crate::sync::GatewayMetaSnapshot::new();
        // Before initial sync, cold-start fallback reads from store.
        assert_eq!(
            resolve_engine_type_from_snapshot(&snap, &store, Some("store-only"))
                .await
                .as_deref(),
            Some("mock")
        );

        // Once synced, snapshot is authoritative: `store-only` is NOT in snapshot so it
        // must NOT query store (returns None), while `served-alias` resolves purely from snapshot.
        let snap_spec = ModelSpec {
            model_uid: "uid-1".into(),
            model_name: "name-1".into(),
            model_source: ModelSource::Local,
            model_path: None,
            engine_type: Some("sglang".into()),
            docker_image: None,
            config: Some(ModelConfig {
                served_model_name: Some("served-alias".into()),
                ..Default::default()
            }),
            labels: Default::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: None,
        };
        snap.replace_all_model_specs(vec![snap_spec]).await;

        assert_eq!(
            resolve_engine_type_from_snapshot(&snap, &store, Some("store-only")).await,
            None,
            "synced snapshot miss must not fall back to etcd"
        );
        assert_eq!(
            resolve_engine_type_from_snapshot(&snap, &store, Some("served-alias"))
                .await
                .as_deref(),
            Some("sglang")
        );
    }
}
