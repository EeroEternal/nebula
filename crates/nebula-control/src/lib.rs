pub mod compat;
pub mod deploy;
pub mod error;
pub mod platform;
pub mod store;

pub use compat::{list_compat_rules, validate_deploy_compat};
pub use deploy::{
    load_model, scale_model, start_model, stop_model, ScaleDeploymentRequest,
    StartDeploymentRequest,
};
pub use error::ServiceError;
pub use platform::{
    get_operation, list_endpoints_for_model, list_model_specs, list_node_statuses, put_operation,
    record_succeeded_operation, require_model_spec, upsert_model_spec, ControlOperation,
    UpsertModelSpecRequest,
};
pub use store::{
    get_model_deployment, get_model_spec, infer_model_source, is_valid_model_uid, now_ms,
    put_model_deployment, put_model_spec,
};
