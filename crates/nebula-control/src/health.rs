use nebula_common::{DesiredState, EndpointStatus};
use nebula_meta::MetaStore;
use serde::Serialize;

use crate::error::ServiceError;
use crate::inventory::{list_endpoints, list_nodes};
use crate::store::now_ms;

const NODE_STALE_MS: u64 = 90_000;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    Ok,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub status: ComponentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterCounts {
    pub nodes_online: u32,
    pub total_replicas: u32,
    pub ready_replicas: u32,
    pub running_deployments: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthSummary {
    pub gateway: ComponentHealth,
    pub etcd: ComponentHealth,
    pub router: ComponentHealth,
    #[serde(flatten)]
    pub cluster: ClusterCounts,
}

pub async fn cluster_counts(store: &dyn MetaStore) -> Result<ClusterCounts, ServiceError> {
    let now = now_ms();
    let nodes = list_nodes(store).await?;
    let nodes_online = nodes
        .iter()
        .filter(|n| now.saturating_sub(n.status.last_heartbeat_ms) <= NODE_STALE_MS)
        .count() as u32;

    let endpoints = list_endpoints(store).await?;
    let total_replicas = endpoints.len() as u32;
    let ready_replicas = endpoints
        .iter()
        .filter(|ep| ep.status == EndpointStatus::Ready)
        .count() as u32;

    let deployments = store.list_prefix("/deployments/").await?;
    let mut running_deployments = 0u32;
    for (_, value, _) in deployments {
        if let Ok(dep) = serde_json::from_slice::<nebula_common::ModelDeployment>(&value) {
            if dep.desired_state == DesiredState::Running {
                running_deployments += 1;
            }
        }
    }

    Ok(ClusterCounts {
        nodes_online,
        total_replicas,
        ready_replicas,
        running_deployments,
    })
}

pub async fn etcd_health(store: &dyn MetaStore) -> ComponentHealth {
    match store.list_prefix("/nodes/").await {
        Ok(_) => ComponentHealth {
            status: ComponentStatus::Ok,
            message: None,
        },
        Err(e) => ComponentHealth {
            status: ComponentStatus::Unavailable,
            message: Some(format!("etcd unreachable: {e}")),
        },
    }
}
