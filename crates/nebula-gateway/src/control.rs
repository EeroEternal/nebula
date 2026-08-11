//! Map shared control-plane errors to HTTP responses (C3 envelope).

use axum::response::{IntoResponse, Response};
use nebula_control::ServiceError;

pub fn control_error(err: ServiceError) -> Response {
    err.into_response()
}

pub fn control_error_with_request_id(err: ServiceError, request_id: &str) -> Response {
    use axum::{http::StatusCode, Json};
    let status = err.status_code();
    (status, Json(err.into_json_body(Some(request_id)))).into_response()
}
