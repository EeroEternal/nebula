//! In-memory watch/snapshot synchronization for Gateway hot-path metadata.
//!
//! Enforces AGENTS.md Rule #8: "Gateway/Router hot path reads /endpoints/ only".
//! Tenancy and Model Spec metadata are loaded via snapshot on boot and updated
//! reactively via etcd watch streams so that the inference hot path never executes
//! blocking `store.get(...)` calls per request.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use nebula_common::{ModelSpec, Tenant};
use nebula_meta::{EtcdMetaStore, MetaStore};
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct GatewayMetaSnapshot {
    tenants: Arc<RwLock<HashMap<String, Tenant>>>,
    model_engine_types: Arc<RwLock<HashMap<String, String>>>,
}

impl GatewayMetaSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read a tenant by id purely in-memory.
    pub async fn get_tenant(&self, tenant_id: &str) -> Option<Tenant> {
        let guard = self.tenants.read().await;
        guard.get(tenant_id).cloned()
    }

    /// Read an engine type for a model spec purely in-memory.
    pub async fn get_engine_type(&self, model: &str) -> Option<String> {
        let guard = self.model_engine_types.read().await;
        guard.get(model).cloned()
    }

    /// Upsert or replace a tenant in the local snapshot.
    pub async fn upsert_tenant(&self, tenant: Tenant) {
        let mut guard = self.tenants.write().await;
        guard.insert(tenant.tenant_id.clone(), tenant);
    }

    /// Remove a tenant from the local snapshot.
    pub async fn remove_tenant(&self, tenant_id: &str) {
        let mut guard = self.tenants.write().await;
        guard.remove(tenant_id);
    }

    /// Replace all tenants atomically (e.g. on full snapshot refresh).
    pub async fn replace_all_tenants(&self, list: Vec<Tenant>) {
        let mut map = HashMap::with_capacity(list.len());
        for t in list {
            map.insert(t.tenant_id.clone(), t);
        }
        let mut guard = self.tenants.write().await;
        *guard = map;
    }

    /// Upsert or update model engine type in the local snapshot.
    pub async fn upsert_model_spec(&self, spec: &ModelSpec) {
        let engine = nebula_common::resolve_engine_type(spec.engine_type.as_deref());
        let mut guard = self.model_engine_types.write().await;
        guard.insert(spec.model_uid.clone(), engine.clone());
        if !spec.model_name.is_empty() {
            guard.insert(spec.model_name.clone(), engine);
        }
    }

    /// Remove a model spec by model identifier.
    pub async fn remove_model_spec(&self, model_id: &str) {
        let mut guard = self.model_engine_types.write().await;
        guard.remove(model_id);
    }

    /// Replace all model specs atomically.
    pub async fn replace_all_model_specs(&self, list: Vec<ModelSpec>) {
        let mut map = HashMap::with_capacity(list.len() * 2);
        for spec in list {
            let engine = nebula_common::resolve_engine_type(spec.engine_type.as_deref());
            map.insert(spec.model_uid.clone(), engine.clone());
            if !spec.model_name.is_empty() {
                map.insert(spec.model_name.clone(), engine);
            }
        }
        let mut guard = self.model_engine_types.write().await;
        *guard = map;
    }
}

/// Background loop watching `/tenants/` and keeping GatewayMetaSnapshot updated.
pub async fn gateway_tenants_sync_loop(
    store: Arc<EtcdMetaStore>,
    snapshot: GatewayMetaSnapshot,
) -> anyhow::Result<()> {
    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/tenants/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "gateway: failed to list tenants snapshot, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut tenants = Vec::new();
        for (_k, v, _rev) in items {
            if let Ok(tenant) = serde_json::from_slice::<Tenant>(&v) {
                tenants.push(tenant);
            }
        }
        snapshot.replace_all_tenants(tenants).await;

        let mut stream = match store.watch_prefix("/tenants/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "gateway: failed to watch tenants, will resync");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        while let Some(ev) = stream.next().await {
            match ev.value {
                Some(v) => {
                    if let Ok(tenant) = serde_json::from_slice::<Tenant>(&v) {
                        snapshot.upsert_tenant(tenant).await;
                    }
                }
                None => {
                    let tenant_id = ev
                        .key
                        .strip_prefix("/tenants/")
                        .unwrap_or(&ev.key)
                        .trim_matches('/');
                    if !tenant_id.is_empty() {
                        snapshot.remove_tenant(tenant_id).await;
                    }
                }
            }
        }

        tracing::warn!("gateway: tenants watch stream ended, full resyncing");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Background loop watching `/models/` specs and keeping GatewayMetaSnapshot updated.
pub async fn gateway_models_sync_loop(
    store: Arc<EtcdMetaStore>,
    snapshot: GatewayMetaSnapshot,
) -> anyhow::Result<()> {
    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/models/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "gateway: failed to list models snapshot, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut specs = Vec::new();
        for (k, v, _rev) in items {
            if k.ends_with("/spec") {
                if let Ok(spec) = serde_json::from_slice::<ModelSpec>(&v) {
                    specs.push(spec);
                }
            }
        }
        snapshot.replace_all_model_specs(specs).await;

        let mut stream = match store.watch_prefix("/models/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "gateway: failed to watch models, will resync");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        while let Some(ev) = stream.next().await {
            if !ev.key.ends_with("/spec") {
                continue;
            }
            let model_id = ev
                .key
                .strip_prefix("/models/")
                .and_then(|r| r.strip_suffix("/spec"))
                .unwrap_or("")
                .to_string();

            match ev.value {
                Some(v) => {
                    if let Ok(spec) = serde_json::from_slice::<ModelSpec>(&v) {
                        snapshot.upsert_model_spec(&spec).await;
                    }
                }
                None => {
                    if !model_id.is_empty() {
                        snapshot.remove_model_spec(&model_id).await;
                    }
                }
            }
        }

        tracing::warn!("gateway: models watch stream ended, full resyncing");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_meta_snapshot_basic() {
        let snap = GatewayMetaSnapshot::new();
        assert!(snap.get_tenant("t1").await.is_none());
        assert!(snap.get_engine_type("m1").await.is_none());

        let t = Tenant {
            tenant_id: "t1".into(),
            display_name: "Tenant 1".into(),
            enabled: true,
            quotas: Default::default(),
            api_token_principals: vec![],
            priority_default: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        snap.upsert_tenant(t.clone()).await;
        assert_eq!(snap.get_tenant("t1").await.unwrap().tenant_id, "t1");

        let spec = ModelSpec {
            model_uid: "m1".into(),
            model_name: "llama-3".into(),
            model_source: nebula_common::ModelSource::Local,
            model_path: None,
            engine_type: Some("vllm".into()),
            docker_image: None,
            config: None,
            labels: Default::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: None,
        };
        snap.upsert_model_spec(&spec).await;
        assert_eq!(snap.get_engine_type("m1").await.as_deref(), Some("vllm"));
        assert_eq!(
            snap.get_engine_type("llama-3").await.as_deref(),
            Some("vllm")
        );

        snap.remove_tenant("t1").await;
        assert!(snap.get_tenant("t1").await.is_none());
        snap.remove_model_spec("m1").await;
        assert!(snap.get_engine_type("m1").await.is_none());
    }
}
