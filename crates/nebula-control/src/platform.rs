//! Read helpers and Control Operation records for `/platform/v1`.

use nebula_common::{EndpointInfo, ModelSpec, NodeStatus};
use nebula_meta::MetaStore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ServiceError;
use crate::store::{get_model_spec, is_valid_model_uid, now_ms, put_model_spec};

/// Async control operation tracked for machine clients (I1.4 minimal).
///
/// etcd: `/operations/{operation_id}` — small, latest-value only; no lease.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlOperation {
    pub operation_id: String,
    pub model_uid: String,
    /// `deploy` | `scale` | `stop` | `upsert_spec` | `upsert_deployment`
    pub kind: String,
    /// `accepted` | `succeeded` | `failed`
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub created_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertModelSpecRequest {
    pub model_uid: String,
    pub model_name: String,
    #[serde(default)]
    pub model_path: Option<String>,
    #[serde(default)]
    pub engine_type: Option<String>,
    #[serde(default)]
    pub docker_image: Option<String>,
    #[serde(default)]
    pub config: Option<nebula_common::ModelConfig>,
    #[serde(default)]
    pub labels: Option<std::collections::HashMap<String, String>>,
}

pub async fn list_model_specs(store: &dyn MetaStore) -> Result<Vec<ModelSpec>, ServiceError> {
    let entries = store.list_prefix("/models/").await?;
    let mut out = Vec::new();
    for (key, v, _) in entries {
        if !key.ends_with("/spec") {
            continue;
        }
        if let Ok(spec) = serde_json::from_slice::<ModelSpec>(&v) {
            out.push(spec);
        }
    }
    out.sort_by(|a, b| a.model_uid.cmp(&b.model_uid));
    Ok(out)
}

pub async fn list_endpoints_for_model(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<Vec<EndpointInfo>, ServiceError> {
    let prefix = format!("/endpoints/{model_uid}/");
    let entries = store.list_prefix(&prefix).await?;
    let mut out = Vec::new();
    for (_, v, _) in entries {
        if let Ok(ep) = serde_json::from_slice::<EndpointInfo>(&v) {
            out.push(ep);
        }
    }
    out.sort_by_key(|e| e.replica_id);
    Ok(out)
}

pub async fn list_node_statuses(store: &dyn MetaStore) -> Result<Vec<NodeStatus>, ServiceError> {
    let entries = store.list_prefix("/nodes/").await?;
    let mut out = Vec::new();
    for (key, v, _) in entries {
        // Keys look like `/nodes/{id}/status`
        if !key.ends_with("/status") {
            continue;
        }
        if let Ok(n) = serde_json::from_slice::<NodeStatus>(&v) {
            out.push(n);
        }
    }
    out.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    Ok(out)
}

pub async fn upsert_model_spec(
    store: &dyn MetaStore,
    principal: &str,
    req: UpsertModelSpecRequest,
) -> Result<ModelSpec, ServiceError> {
    if !is_valid_model_uid(&req.model_uid) {
        return Err(ServiceError::BadRequest(
            "model_uid must match [a-z0-9][a-z0-9-]* and be at most 63 chars".to_string(),
        ));
    }
    let now = now_ms();
    let model_uid = req.model_uid.clone();
    let spec = match store.get(&format!("/models/{model_uid}/spec")).await? {
        Some((data, _)) => {
            let mut existing: ModelSpec = serde_json::from_slice(&data)?;
            existing.model_name = req.model_name;
            existing.engine_type = req.engine_type;
            existing.docker_image = req.docker_image;
            existing.config = req.config;
            if let Some(path) = req.model_path {
                existing.model_path = Some(path);
            }
            if let Some(labels) = req.labels {
                existing.labels = labels;
            }
            existing.updated_at_ms = now;
            existing
        }
        None => ModelSpec {
            model_uid: model_uid.clone(),
            model_name: req.model_name.clone(),
            model_source: crate::store::infer_model_source(
                &req.model_name,
                req.model_path.as_deref(),
            ),
            model_path: req.model_path,
            engine_type: req.engine_type,
            docker_image: req.docker_image,
            config: req.config,
            labels: req.labels.unwrap_or_default(),
            created_at_ms: now,
            updated_at_ms: now,
            created_by: Some(principal.to_string()),
        },
    };
    put_model_spec(store, &model_uid, &spec).await?;
    Ok(spec)
}

pub fn new_operation_id() -> String {
    format!("op_{}", Uuid::new_v4().simple())
}

pub async fn put_operation(
    store: &dyn MetaStore,
    op: &ControlOperation,
) -> Result<(), ServiceError> {
    let val = serde_json::to_vec(op)?;
    store
        .put(&format!("/operations/{}", op.operation_id), val, None)
        .await?;
    Ok(())
}

pub async fn get_operation(
    store: &dyn MetaStore,
    operation_id: &str,
) -> Result<ControlOperation, ServiceError> {
    match store.get(&format!("/operations/{operation_id}")).await? {
        Some((data, _)) => Ok(serde_json::from_slice(&data)?),
        None => Err(ServiceError::NotFound(format!(
            "operation '{operation_id}' not found"
        ))),
    }
}

/// Record a succeeded control write as an Operation (I1.4 minimal).
pub async fn record_succeeded_operation(
    store: &dyn MetaStore,
    model_uid: &str,
    kind: &str,
) -> Result<ControlOperation, ServiceError> {
    let now = now_ms();
    let op = ControlOperation {
        operation_id: new_operation_id(),
        model_uid: model_uid.to_string(),
        kind: kind.to_string(),
        status: "succeeded".into(),
        message: None,
        created_at_ms: now,
        finished_at_ms: Some(now),
    };
    put_operation(store, &op).await?;
    Ok(op)
}

/// Ensure model exists (for handlers that need 404 on missing uid).
pub async fn require_model_spec(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<ModelSpec, ServiceError> {
    get_model_spec(store, model_uid).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_common::{EndpointKind, EndpointStatus};
    use nebula_meta::MemoryMetaStore;

    #[tokio::test]
    async fn list_and_operation_roundtrip() {
        let store = MemoryMetaStore::new();
        let spec = upsert_model_spec(
            &store,
            "tester",
            UpsertModelSpecRequest {
                model_uid: "demo-model".into(),
                model_name: "Qwen/Demo".into(),
                model_path: None,
                engine_type: Some("vllm".into()),
                docker_image: None,
                config: None,
                labels: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(spec.model_uid, "demo-model");

        let listed = list_model_specs(&store).await.unwrap();
        assert_eq!(listed.len(), 1);

        let ep = EndpointInfo {
            model_uid: "demo-model".into(),
            replica_id: 0,
            plan_version: 1,
            node_id: "n1".into(),
            endpoint_kind: EndpointKind::NativeHttp,
            api_flavor: "openai".into(),
            status: EndpointStatus::Ready,
            last_heartbeat_ms: 1,
            status_detail: None,
            grpc_target: None,
            base_url: Some("http://127.0.0.1:8000".into()),
        };
        store
            .put(
                "/endpoints/demo-model/0",
                serde_json::to_vec(&ep).unwrap(),
                None,
            )
            .await
            .unwrap();
        let eps = list_endpoints_for_model(&store, "demo-model").await.unwrap();
        assert_eq!(eps.len(), 1);

        let node = NodeStatus {
            node_id: "n1".into(),
            last_heartbeat_ms: 1,
            gpus: vec![],
            api_addr: None,
            platform: Some("nvidia-cuda".into()),
        };
        store
            .put(
                "/nodes/n1/status",
                serde_json::to_vec(&node).unwrap(),
                None,
            )
            .await
            .unwrap();
        let nodes = list_node_statuses(&store).await.unwrap();
        assert_eq!(nodes.len(), 1);

        let op = record_succeeded_operation(&store, "demo-model", "deploy")
            .await
            .unwrap();
        let got = get_operation(&store, &op.operation_id).await.unwrap();
        assert_eq!(got.status, "succeeded");
        assert_eq!(got.kind, "deploy");
    }
}
