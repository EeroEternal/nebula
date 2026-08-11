use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Etcd error: {0}")]
    Etcd(#[from] anyhow::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Upstream error: {0}")]
    Upstream(String),
}

impl ServiceError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
            ServiceError::Conflict(_) => StatusCode::CONFLICT,
            ServiceError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ServiceError::Unauthorized => StatusCode::UNAUTHORIZED,
            ServiceError::Upstream(_) => StatusCode::BAD_GATEWAY,
            ServiceError::Etcd(_) | ServiceError::Serialization(_) | ServiceError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            ServiceError::NotFound(_) => "invalid_request_error",
            ServiceError::Conflict(_) => "invalid_request_error",
            ServiceError::BadRequest(_) => "invalid_request_error",
            ServiceError::Unauthorized => "authentication_error",
            ServiceError::Upstream(_) => "upstream_error",
            ServiceError::Etcd(_) | ServiceError::Internal(_) => "server_error",
            ServiceError::Serialization(_) => "server_error",
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            ServiceError::NotFound(_) => "not_found",
            ServiceError::Conflict(_) => "conflict",
            ServiceError::BadRequest(_) => "bad_request",
            ServiceError::Unauthorized => "unauthorized",
            ServiceError::Upstream(_) => "upstream_other",
            ServiceError::Etcd(_) => "etcd_error",
            ServiceError::Serialization(_) => "serialization_error",
            ServiceError::Internal(_) => "internal_error",
        }
    }

    pub fn message(&self) -> String {
        match self {
            ServiceError::Unauthorized => "Unauthorized".to_string(),
            other => other.to_string(),
        }
    }

    pub fn into_json_body(&self, request_id: Option<&str>) -> serde_json::Value {
        let request_id = request_id
            .map(str::to_string)
            .unwrap_or_else(|| format!("req_{}", Uuid::new_v4()));
        json!({
            "error": {
                "type": self.error_type(),
                "code": self.error_code(),
                "message": self.message(),
                "request_id": request_id,
            }
        })
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        (status, Json(self.into_json_body(None))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_json_uses_c3_shape() {
        let err = ServiceError::BadRequest("engine not compatible".to_string());
        let body = err.into_json_body(Some("req_test"));
        assert_eq!(body["error"]["type"], "invalid_request_error");
        assert_eq!(body["error"]["code"], "bad_request");
        assert_eq!(body["error"]["request_id"], "req_test");
    }
}
