//! Idempotency-Key handling for `/platform/v1` write endpoints (I3.3).

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use nebula_control::{
    get_idempotency, hash_body, put_idempotency, IdempotencyRecord, Operation, ServiceError,
};
use nebula_meta::EtcdMetaStore;

pub fn extract_idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("Idempotency-Key")
        .or_else(|| headers.get("idempotency-key"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() <= 128)
}

pub async fn check_idempotency(
    store: &EtcdMetaStore,
    principal: &str,
    headers: &HeaderMap,
    path: &str,
    body: &[u8],
) -> Result<Option<Response>, ServiceError> {
    let Some(key) = extract_idempotency_key(headers) else {
        return Ok(None);
    };
    let body_hash = hash_body(body);
    match get_idempotency(store, principal, &key).await? {
        None => Ok(None),
        Some(record) if record.path == path && record.body_hash == body_hash => {
            Ok(Some(idempotent_accepted(&record)))
        }
        Some(_) => Err(ServiceError::Conflict(
            "Idempotency-Key reused with different request body or path".to_string(),
        )),
    }
}

pub async fn record_idempotency(
    store: &EtcdMetaStore,
    principal: &str,
    headers: &HeaderMap,
    path: &str,
    body: &[u8],
    op: &Operation,
) -> Result<(), ServiceError> {
    let Some(key) = extract_idempotency_key(headers) else {
        return Ok(());
    };
    let record = IdempotencyRecord {
        operation_id: op.operation_id.clone(),
        model_uid: op.model_uid.clone(),
        path: path.to_string(),
        body_hash: hash_body(body),
        created_at_ms: op.created_at_ms,
    };
    put_idempotency(store, principal, &key, &record).await
}

fn idempotent_accepted(record: &IdempotencyRecord) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "operation_id": record.operation_id,
            "model_uid": record.model_uid,
            "idempotent_replay": true,
        })),
    )
        .into_response()
}

pub fn idempotency_conflict(err: ServiceError) -> Response {
    use crate::control::control_error;
    control_error(err)
}
