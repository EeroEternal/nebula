use nebula_common::{DesiredState, ModelDeployment};
use nebula_meta::MetaStore;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::ServiceError;
use crate::inventory::{count_ready_replicas, list_replicas};
use crate::store::{get_model_deployment, now_ms};

/// Operation records live in etcd with a short TTL (see `OPERATION_TTL_MS`).
pub const OPERATION_PREFIX: &str = "/operations/";
pub const OPERATION_TTL_MS: u64 = 86_400_000; // 24h

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Deploy,
    Scale,
    Stop,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub operation_id: String,
    pub kind: OperationKind,
    pub model_uid: String,
    pub status: OperationStatus,
    pub deployment_version: u64,
    pub desired_replicas: u32,
    pub ready_replicas: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationResponse {
    pub operation_id: String,
}

pub async fn create_operation(
    store: &dyn MetaStore,
    kind: OperationKind,
    deployment: &ModelDeployment,
) -> Result<Operation, ServiceError> {
    let now = now_ms();
    let operation_id = format!("op_{}", Uuid::new_v4());
    let op = Operation {
        operation_id: operation_id.clone(),
        kind,
        model_uid: deployment.model_uid.clone(),
        status: OperationStatus::Pending,
        deployment_version: deployment.version,
        desired_replicas: if deployment.desired_state == DesiredState::Stopped {
            0
        } else {
            deployment.replicas
        },
        ready_replicas: 0,
        message: None,
        created_at_ms: now,
        updated_at_ms: now,
    };
    put_operation(store, &op).await?;
    Ok(op)
}

pub async fn get_operation(
    store: &dyn MetaStore,
    operation_id: &str,
) -> Result<Operation, ServiceError> {
    let key = format!("{OPERATION_PREFIX}{operation_id}");
    match store.get(&key).await? {
        Some((data, _)) => {
            let mut op: Operation = serde_json::from_slice(&data)?;
            refresh_operation_status(store, &mut op).await?;
            put_operation(store, &op).await?;
            Ok(op)
        }
        None => Err(ServiceError::NotFound(format!(
            "operation '{operation_id}' not found"
        ))),
    }
}

async fn put_operation(store: &dyn MetaStore, op: &Operation) -> Result<(), ServiceError> {
    let key = format!("{}{}", OPERATION_PREFIX, op.operation_id);
    let val = serde_json::to_vec(op)?;
    store.put(&key, val, Some(OPERATION_TTL_MS)).await?;
    Ok(())
}

pub async fn refresh_operation_status(
    store: &dyn MetaStore,
    op: &mut Operation,
) -> Result<(), ServiceError> {
    let deployment = get_model_deployment(store, &op.model_uid).await?;
    let replicas = list_replicas(store, &op.model_uid).await?;
    let ready = count_ready_replicas(&replicas);

    op.ready_replicas = ready;
    op.updated_at_ms = now_ms();

    let dep = match deployment {
        Some(d) if d.version >= op.deployment_version => d,
        Some(d) => {
            op.deployment_version = d.version;
            d
        }
        None if op.kind == OperationKind::Stop => {
            op.status = if replicas.is_empty() {
                OperationStatus::Succeeded
            } else {
                OperationStatus::Running
            };
            op.message = None;
            return Ok(());
        }
        None => {
            op.status = OperationStatus::Failed;
            op.message = Some("deployment record missing".to_string());
            return Ok(());
        }
    };

    op.desired_replicas = if dep.desired_state == DesiredState::Stopped {
        0
    } else {
        dep.replicas
    };

    if replicas.iter().any(|r| r.status == nebula_common::EndpointStatus::Failed) {
        op.status = OperationStatus::Failed;
        op.message = Some("one or more replicas failed".to_string());
        return Ok(());
    }

    match op.kind {
        OperationKind::Stop => {
            op.status = if replicas.is_empty() {
                OperationStatus::Succeeded
            } else {
                OperationStatus::Running
            };
        }
        OperationKind::Deploy | OperationKind::Scale => {
            if dep.desired_state == DesiredState::Stopped {
                op.status = OperationStatus::Failed;
                op.message = Some("deployment desired_state is stopped".to_string());
            } else if ready >= dep.replicas {
                op.status = OperationStatus::Succeeded;
                op.message = None;
            } else if ready > 0 || replicas.iter().any(|r| {
                r.status == nebula_common::EndpointStatus::Starting
            }) {
                op.status = OperationStatus::Running;
                op.message = None;
            } else {
                op.status = OperationStatus::Pending;
                op.message = None;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ReplicaView;
    use nebula_common::EndpointStatus;

    #[test]
    fn count_ready_ignores_starting() {
        let replicas = vec![
            ReplicaView {
                model_uid: "m".into(),
                replica_id: 0,
                node_id: "n".into(),
                status: EndpointStatus::Ready,
                last_heartbeat_ms: 0,
                status_detail: None,
                base_url: None,
            },
            ReplicaView {
                model_uid: "m".into(),
                replica_id: 1,
                node_id: "n".into(),
                status: EndpointStatus::Starting,
                last_heartbeat_ms: 0,
                status_detail: None,
                base_url: None,
            },
        ];
        assert_eq!(count_ready_replicas(&replicas), 1);
    }
}
