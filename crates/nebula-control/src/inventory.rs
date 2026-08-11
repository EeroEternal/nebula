use nebula_common::{EndpointInfo, EndpointStatus, NodeStatus, resolve_node_platform};
use nebula_meta::MetaStore;
use serde::Serialize;

use crate::error::ServiceError;

#[derive(Debug, Clone, Serialize)]
pub struct NodeInventory {
    #[serde(flatten)]
    pub status: NodeStatus,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplicaView {
    pub model_uid: String,
    pub replica_id: u32,
    pub node_id: String,
    pub status: EndpointStatus,
    pub last_heartbeat_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

impl From<EndpointInfo> for ReplicaView {
    fn from(ep: EndpointInfo) -> Self {
        Self {
            model_uid: ep.model_uid,
            replica_id: ep.replica_id,
            node_id: ep.node_id,
            status: ep.status,
            last_heartbeat_ms: ep.last_heartbeat_ms,
            status_detail: ep.status_detail,
            base_url: ep.base_url,
        }
    }
}

pub async fn list_nodes(store: &dyn MetaStore) -> Result<Vec<NodeInventory>, ServiceError> {
    let raw = store.list_prefix("/nodes/").await?;
    let mut nodes = Vec::new();
    for (key, value, _) in raw {
        if !key.ends_with("/status") {
            continue;
        }
        if let Ok(status) = serde_json::from_slice::<NodeStatus>(&value) {
            let platform = resolve_node_platform(&status);
            nodes.push(NodeInventory { status, platform });
        }
    }
    nodes.sort_by(|a, b| a.status.node_id.cmp(&b.status.node_id));
    Ok(nodes)
}

pub async fn list_endpoints(store: &dyn MetaStore) -> Result<Vec<EndpointInfo>, ServiceError> {
    let raw = store.list_prefix("/endpoints/").await?;
    let mut endpoints = Vec::new();
    for (_, value, _) in raw {
        if let Ok(ep) = serde_json::from_slice::<EndpointInfo>(&value) {
            endpoints.push(ep);
        }
    }
    Ok(endpoints)
}

pub async fn list_replicas(
    store: &dyn MetaStore,
    model_uid: &str,
) -> Result<Vec<ReplicaView>, ServiceError> {
    let endpoints = list_endpoints(store).await?;
    let mut replicas: Vec<ReplicaView> = endpoints
        .into_iter()
        .filter(|ep| ep.model_uid == model_uid)
        .map(ReplicaView::from)
        .collect();
    replicas.sort_by_key(|r| r.replica_id);
    Ok(replicas)
}

pub fn count_ready_replicas(replicas: &[ReplicaView]) -> u32 {
    replicas
        .iter()
        .filter(|r| r.status == EndpointStatus::Ready)
        .count() as u32
}
