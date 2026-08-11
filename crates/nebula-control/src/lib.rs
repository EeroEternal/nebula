pub mod compat;
pub mod deploy;
pub mod error;
pub mod health;
pub mod idempotency;
pub mod inventory;
pub mod models;
pub mod operation;
pub mod store;

pub use compat::{list_compat_rules, validate_deploy_compat};
pub use deploy::{
    load_model, scale_model, start_model, stop_model, ScaleDeploymentRequest,
    StartDeploymentRequest,
};
pub use error::ServiceError;
pub use health::{cluster_counts, etcd_health, ClusterCounts, ComponentHealth, ComponentStatus, HealthSummary};
pub use idempotency::{get_idempotency, hash_body, put_idempotency, IdempotencyRecord};
pub use inventory::{list_endpoints, list_nodes, list_replicas, NodeInventory, ReplicaView};
pub use models::{create_model, get_model, list_models, CreateModelRequest};
pub use operation::{
    create_operation, get_operation, Operation, OperationKind, OperationResponse, OperationStatus,
};
pub use store::{
    get_model_deployment, get_model_spec, infer_model_source, is_valid_model_uid, now_ms,
    put_model_deployment, put_model_spec,
};
