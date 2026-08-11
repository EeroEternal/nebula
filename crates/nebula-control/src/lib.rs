pub mod compat;
pub mod deploy;
pub mod error;
pub mod governance;
pub mod health;
pub mod idempotency;
pub mod inventory;
pub mod models;
pub mod operation;
pub mod router_metrics;
pub mod store;

pub use compat::{list_compat_rules, validate_deploy_compat};
pub use deploy::{
    callback_url_from_scale, callback_url_from_start, load_model, scale_model, start_model,
    stop_model, validate_callback_url, ScaleDeploymentRequest, StartDeploymentRequest,
};
pub use error::ServiceError;
pub use governance::{
    filter_canaries_by_model, get_canary, get_slo, list_canaries, list_slos,
};
pub use health::{cluster_counts, etcd_health, ClusterCounts, ComponentHealth, ComponentStatus, HealthSummary};
pub use idempotency::{get_idempotency, hash_body, put_idempotency, IdempotencyRecord};
pub use inventory::{
    count_ready_replicas, drain_replica, get_cluster_status, list_endpoints, list_nodes,
    list_replicas, DrainReplicaRequest, DrainReplicaResponse, NodeInventory, ReplicaView,
};
pub use models::{create_model, get_model, list_models, CreateModelRequest};
pub use operation::{
    create_operation, get_operation, Operation, OperationKind, OperationOptions, OperationResponse,
    OperationStatus,
};
pub use router_metrics::evaluate_slo_from_router_metrics;
pub use store::{
    get_model_deployment, get_model_spec, infer_model_source, is_valid_model_uid, now_ms,
    put_model_deployment, put_model_spec,
};
