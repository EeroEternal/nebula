//! C3 stable OpenAI-shaped error envelopes for Gateway-originated failures.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};

/// Build the stable error JSON object (without HTTP wrapper).
pub fn openai_error_json(error_type: &str, code: &str, message: impl Into<String>) -> Value {
    json!({
        "error": {
            "message": message.into(),
            "type": error_type,
            "code": code,
        }
    })
}

/// Build `{ "error": { "message", "type", "code" } }` with the given HTTP status.
pub fn openai_error_response(
    status: StatusCode,
    error_type: &str,
    code: &str,
    message: impl Into<String>,
) -> Response {
    (status, Json(openai_error_json(error_type, code, message))).into_response()
}

/// Map a classified reqwest failure kind to a stable upstream envelope.
pub fn upstream_transport_error(kind: &str, detail: impl Into<String>) -> Response {
    let detail = detail.into();
    match kind {
        "timeout" => openai_error_response(
            StatusCode::GATEWAY_TIMEOUT,
            "upstream_error",
            "upstream_timeout",
            detail,
        ),
        "connect" => openai_error_response(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            "upstream_connect",
            detail,
        ),
        _ => openai_error_response(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            "upstream_other",
            detail,
        ),
    }
}

pub fn payload_too_large_response() -> Response {
    openai_error_response(
        StatusCode::PAYLOAD_TOO_LARGE,
        "invalid_request_error",
        "payload_too_large",
        "request body too large",
    )
}

/// If body looks like JSON object/array, return true.
pub fn body_looks_like_json(bytes: &[u8]) -> bool {
    let trimmed = bytes.iter().find(|b| !b.is_ascii_whitespace()).copied();
    matches!(trimmed, Some(b'{') | Some(b'['))
}

/// Normalize Router plain-text 429/503 into stable envelopes; leave JSON as-is.
pub fn maybe_normalize_router_error(status: StatusCode, body: &[u8]) -> Option<Response> {
    if status != StatusCode::TOO_MANY_REQUESTS && status != StatusCode::SERVICE_UNAVAILABLE {
        return None;
    }
    if body_looks_like_json(body) {
        return None;
    }
    let message = String::from_utf8_lossy(body).trim().to_string();
    let message = if message.is_empty() {
        status
            .canonical_reason()
            .unwrap_or("upstream error")
            .to_string()
    } else {
        message
    };
    Some(match status {
        StatusCode::TOO_MANY_REQUESTS => openai_error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit",
            "endpoints_overloaded",
            message,
        ),
        StatusCode::SERVICE_UNAVAILABLE => openai_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "no_ready_endpoint",
            message,
        ),
        _ => unreachable!(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_kinds_map_to_stable_codes() {
        let timeout = openai_error_json("upstream_error", "upstream_timeout", "t");
        assert_eq!(timeout["error"]["code"], "upstream_timeout");
        assert_eq!(
            upstream_transport_error("timeout", "t").status(),
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(
            upstream_transport_error("connect", "c").status(),
            StatusCode::BAD_GATEWAY
        );
        assert_eq!(
            upstream_transport_error("other", "o").status(),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn payload_too_large_envelope() {
        let resp = payload_too_large_response();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let v = openai_error_json("invalid_request_error", "payload_too_large", "request body too large");
        assert_eq!(v["error"]["type"], "invalid_request_error");
        assert_eq!(v["error"]["code"], "payload_too_large");
    }

    #[test]
    fn normalizes_plain_429_and_leaves_json() {
        let plain = maybe_normalize_router_error(
            StatusCode::TOO_MANY_REQUESTS,
            b"all endpoints overloaded for model 'm'",
        )
        .expect("normalize");
        assert_eq!(plain.status(), StatusCode::TOO_MANY_REQUESTS);

        let json_body = br#"{"error":{"message":"x","type":"rate_limit_error"}}"#;
        assert!(
            maybe_normalize_router_error(StatusCode::TOO_MANY_REQUESTS, json_body).is_none()
        );
    }

    #[test]
    fn normalizes_plain_503() {
        let plain = maybe_normalize_router_error(
            StatusCode::SERVICE_UNAVAILABLE,
            b"no ready endpoint for model 'm'",
        )
        .expect("normalize");
        assert_eq!(plain.status(), StatusCode::SERVICE_UNAVAILABLE);
        let v = openai_error_json("server_error", "no_ready_endpoint", "no ready endpoint");
        assert_eq!(v["error"]["code"], "no_ready_endpoint");
    }
}
