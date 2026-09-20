//! Map shared control-plane errors to HTTP responses (C3 envelope).

use axum::response::{IntoResponse, Response};
use nebula_control::ServiceError;

pub fn control_error(err: ServiceError) -> Response {
    err.into_response()
}
