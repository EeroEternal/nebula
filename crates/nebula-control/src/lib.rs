pub mod compat;
pub mod deploy;
pub mod error;
pub mod store;

pub use compat::{list_compat_rules, validate_deploy_compat};
pub use deploy::{
    load_model, scale_model, start_model, stop_model, ScaleDeploymentRequest,
    StartDeploymentRequest,
};
pub use error::ServiceError;
pub use store::{
    get_model_deployment, get_model_spec, infer_model_source, is_valid_model_uid, now_ms,
    put_model_deployment, put_model_spec,
};
