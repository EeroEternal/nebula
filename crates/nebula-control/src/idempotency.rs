use nebula_meta::MetaStore;
use serde::{Deserialize, Serialize};

use crate::error::ServiceError;

pub const IDEMPOTENCY_PREFIX: &str = "/idempotency/";
pub const IDEMPOTENCY_TTL_MS: u64 = 86_400_000; // 24h

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub operation_id: String,
    pub model_uid: String,
    pub path: String,
    pub body_hash: String,
    pub created_at_ms: u64,
}

fn store_key(principal: &str, key: &str) -> String {
    format!("{IDEMPOTENCY_PREFIX}{principal}/{key}")
}

pub async fn get_idempotency(
    store: &dyn MetaStore,
    principal: &str,
    idempotency_key: &str,
) -> Result<Option<IdempotencyRecord>, ServiceError> {
    let key = store_key(principal, idempotency_key);
    match store.get(&key).await? {
        Some((data, _)) => Ok(Some(serde_json::from_slice(&data)?)),
        None => Ok(None),
    }
}

pub async fn put_idempotency(
    store: &dyn MetaStore,
    principal: &str,
    idempotency_key: &str,
    record: &IdempotencyRecord,
) -> Result<(), ServiceError> {
    let key = store_key(principal, idempotency_key);
    let val = serde_json::to_vec(record)?;
    store.put(&key, val, Some(IDEMPOTENCY_TTL_MS)).await?;
    Ok(())
}

pub fn hash_body(body: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(body))
}
