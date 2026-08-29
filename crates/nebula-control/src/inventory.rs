use nebula_common::{
    ClusterStatus, EndpointInfo, EndpointStatus, ModelRequest, NodeStatus, PlacementPlan,
    resolve_node_platform,
};
use nebula_meta::MetaStore;
use serde::{Deserialize, Serialize};

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

/// Full cluster snapshot (nodes, endpoints, placements, legacy model_requests).
pub async fn get_cluster_status(store: &dyn MetaStore) -> Result<ClusterStatus, ServiceError> {
    let nodes_raw = store.list_prefix("/nodes/").await?;
    let mut nodes = Vec::new();
    for (_, v, _) in nodes_raw {
        if let Ok(n) = serde_json::from_slice::<NodeStatus>(&v) {
            nodes.push(n);
        }
    }

    let endpoints = list_endpoints(store).await?;

    let placements_raw = store.list_prefix("/placements/").await?;
    let mut placements = Vec::new();
    for (_, v, _) in placements_raw {
        if let Ok(p) = serde_json::from_slice::<PlacementPlan>(&v) {
            placements.push(p);
        }
    }

    let requests_raw = store.list_prefix("/model_requests/").await?;
    let mut model_requests = Vec::new();
    for (_, v, _) in requests_raw {
        if let Ok(r) = serde_json::from_slice::<ModelRequest>(&v) {
            model_requests.push(r);
        }
    }

    Ok(ClusterStatus {
        nodes,
        endpoints,
        placements,
        model_requests,
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct DrainReplicaRequest {
    pub model_uid: String,
    pub replica_id: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DrainReplicaResponse {
    pub model_uid: String,
    pub replica_id: u32,
    pub status: String,
}

pub async fn drain_replica(
    store: &dyn MetaStore,
    model_uid: &str,
    replica_id: u32,
) -> Result<DrainReplicaResponse, ServiceError> {
    let key = format!("/endpoints/{model_uid}/{replica_id}");
    let (data, _) = store
        .get(&key)
        .await?
        .ok_or_else(|| ServiceError::NotFound("endpoint not found".to_string()))?;

    let mut ep: EndpointInfo = serde_json::from_slice(&data)?;
    if ep.status == EndpointStatus::Draining {
        return Ok(DrainReplicaResponse {
            model_uid: model_uid.to_string(),
            replica_id,
            status: "already_draining".to_string(),
        });
    }
    ep.status = EndpointStatus::Draining;
    let val = serde_json::to_vec(&ep)?;
    store.put(&key, val, None).await?;
    Ok(DrainReplicaResponse {
        model_uid: model_uid.to_string(),
        replica_id,
        status: "draining".to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct DrainNodeResponse {
    pub node_id: String,
    pub drained_replicas: Vec<DrainReplicaResponse>,
}

pub async fn drain_node(
    store: &dyn MetaStore,
    node_id: &str,
) -> Result<DrainNodeResponse, ServiceError> {
    let raw = store.list_prefix("/endpoints/").await?;
    let mut drained = Vec::new();

    for (key, val, _) in raw {
        if let Ok(mut ep) = serde_json::from_slice::<EndpointInfo>(&val) {
            if ep.node_id == node_id {
                let status_str = if ep.status == EndpointStatus::Draining {
                    "already_draining".to_string()
                } else {
                    ep.status = EndpointStatus::Draining;
                    if let Ok(data) = serde_json::to_vec(&ep) {
                        let _ = store.put(&key, data, None).await;
                    }
                    "draining".to_string()
                };
                drained.push(DrainReplicaResponse {
                    model_uid: ep.model_uid,
                    replica_id: ep.replica_id,
                    status: status_str,
                });
            }
        }
    }

    Ok(DrainNodeResponse {
        node_id: node_id.to_string(),
        drained_replicas: drained,
    })
}
