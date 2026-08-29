use nebula_common::pool::{HardwarePool, PoolRole};
use nebula_meta::MetaStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::ServiceError;
use crate::store::now_ms;

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePoolRequest {
    pub pool_id: String,
    pub display_name: String,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub role: PoolRole,
    #[serde(default)]
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub schedulable: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePoolRequest {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub role: Option<PoolRole>,
    #[serde(default)]
    pub node_ids: Option<Vec<String>>,
    #[serde(default)]
    pub labels: Option<HashMap<String, String>>,
    #[serde(default)]
    pub schedulable: Option<bool>,
}

fn pool_key(pool_id: &str) -> String {
    format!("/pools/{pool_id}")
}

pub async fn list_pools(store: &dyn MetaStore) -> Result<Vec<HardwarePool>, ServiceError> {
    let kvs = store.list_prefix("/pools/").await?;
    let mut pools = Vec::new();
    for (_, val, _) in kvs {
        if let Ok(pool) = serde_json::from_slice::<HardwarePool>(&val) {
            pools.push(pool);
        }
    }
    pools.sort_by(|a, b| a.pool_id.cmp(&b.pool_id));
    Ok(pools)
}

pub async fn get_pool(store: &dyn MetaStore, pool_id: &str) -> Result<HardwarePool, ServiceError> {
    match store.get(&pool_key(pool_id)).await? {
        Some((val, _)) => Ok(serde_json::from_slice(&val)?),
        None => Err(ServiceError::NotFound(format!("pool '{pool_id}' not found"))),
    }
}

pub async fn create_pool(
    store: &dyn MetaStore,
    req: CreatePoolRequest,
) -> Result<HardwarePool, ServiceError> {
    let pool_id = req.pool_id.trim();
    if pool_id.is_empty() {
        return Err(ServiceError::BadRequest("pool_id is required".into()));
    }
    let key = pool_key(pool_id);
    if store.get(&key).await?.is_some() {
        return Err(ServiceError::Conflict(format!("pool '{pool_id}' already exists")));
    }

    let pool = HardwarePool {
        pool_id: pool_id.to_string(),
        display_name: if req.display_name.trim().is_empty() {
            pool_id.to_string()
        } else {
            req.display_name
        },
        platform: req.platform,
        role: req.role,
        node_ids: req.node_ids,
        labels: req.labels,
        schedulable: req.schedulable,
        updated_at_ms: now_ms(),
    };

    store.put(&key, serde_json::to_vec(&pool)?, None).await?;
    Ok(pool)
}

pub async fn update_pool(
    store: &dyn MetaStore,
    pool_id: &str,
    req: UpdatePoolRequest,
) -> Result<HardwarePool, ServiceError> {
    let mut pool = get_pool(store, pool_id).await?;
    if let Some(dn) = req.display_name {
        pool.display_name = dn;
    }
    if let Some(p) = req.platform {
        pool.platform = Some(p);
    }
    if let Some(r) = req.role {
        pool.role = r;
    }
    if let Some(nodes) = req.node_ids {
        pool.node_ids = nodes;
    }
    if let Some(labels) = req.labels {
        pool.labels = labels;
    }
    if let Some(s) = req.schedulable {
        pool.schedulable = s;
    }
    pool.updated_at_ms = now_ms();

    let key = pool_key(pool_id);
    store.put(&key, serde_json::to_vec(&pool)?, None).await?;
    Ok(pool)
}

pub async fn delete_pool(store: &dyn MetaStore, pool_id: &str) -> Result<(), ServiceError> {
    let key = pool_key(pool_id);
    if store.get(&key).await?.is_none() {
        return Err(ServiceError::NotFound(format!("pool '{pool_id}' not found")));
    }
    store.delete(&key).await?;
    Ok(())
}
