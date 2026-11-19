//! In-memory watch/snapshot synchronization for Gateway hot-path metadata.
//!
//! Enforces AGENTS.md Rule #8: "Gateway/Router hot path reads /endpoints/ only".
//! Tenancy and Model Spec metadata are loaded via snapshot on boot and updated
//! reactively via etcd watch streams so that the inference hot path never executes
//! per-request `store.get(...)` calls once initial sync has completed.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use nebula_common::{ModelSpec, Tenant};
use nebula_meta::{EtcdMetaStore, MetaStore};
use tokio::sync::{Notify, RwLock};

#[derive(Debug, Default)]
struct ModelEngineCache {
    /// Maps any known alias (`model_uid`, `model_name`, `config.served_model_name`) to resolved `engine_type`.
    by_alias: HashMap<String, String>,
    /// Tracks aliases owned by each `model_uid` so updates and deletes leave no stale entries.
    uid_aliases: HashMap<String, Vec<String>>,
}

impl ModelEngineCache {
    fn collect_aliases(spec: &ModelSpec) -> Vec<String> {
        let mut aliases = Vec::with_capacity(3);
        if !spec.model_uid.is_empty() {
            aliases.push(spec.model_uid.clone());
        }
        if !spec.model_name.is_empty() && !aliases.iter().any(|a| a == &spec.model_name) {
            aliases.push(spec.model_name.clone());
        }
        if let Some(served) = spec
            .config
            .as_ref()
            .and_then(|c| c.served_model_name.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if !aliases.iter().any(|a| a == served) {
                aliases.push(served.to_string());
            }
        }
        aliases
    }

    fn upsert(&mut self, spec: &ModelSpec) {
        self.remove(&spec.model_uid);
        let engine = nebula_common::resolve_engine_type(spec.engine_type.as_deref());
        let aliases = Self::collect_aliases(spec);
        for alias in &aliases {
            self.by_alias.insert(alias.clone(), engine.clone());
        }
        if !spec.model_uid.is_empty() {
            self.uid_aliases.insert(spec.model_uid.clone(), aliases);
        }
    }

    fn remove(&mut self, model_uid: &str) {
        if let Some(old_aliases) = self.uid_aliases.remove(model_uid) {
            for alias in old_aliases {
                self.by_alias.remove(&alias);
            }
        }
        self.by_alias.remove(model_uid);
    }

    fn replace_all(&mut self, list: Vec<ModelSpec>) {
        self.by_alias.clear();
        self.uid_aliases.clear();
        for spec in &list {
            self.upsert(spec);
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct GatewayMetaSnapshot {
    tenants: Arc<RwLock<HashMap<String, Tenant>>>,
    models: Arc<RwLock<ModelEngineCache>>,
    tenants_synced: Arc<AtomicBool>,
    models_synced: Arc<AtomicBool>,
    sync_notify: Arc<Notify>,
}

impl GatewayMetaSnapshot {
    pub fn new() -> Self {
        Self::default()
    }

    /// True once `/tenants/` has completed at least one full snapshot load.
    pub fn is_tenants_synced(&self) -> bool {
        self.tenants_synced.load(Ordering::Acquire)
    }

    /// True once `/models/` has completed at least one full snapshot load.
    pub fn is_models_synced(&self) -> bool {
        self.models_synced.load(Ordering::Acquire)
    }

    /// Wait up to `timeout` for both `/tenants/` and `/models/` initial snapshots.
    /// Returns `(tenants_synced, models_synced)`.
    pub async fn wait_initial_sync(&self, timeout: Duration) -> (bool, bool) {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let t = self.is_tenants_synced();
            let m = self.is_models_synced();
            if t && m {
                return (true, true);
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return (t, m);
            }
            let _ = tokio::time::timeout(remaining, self.sync_notify.notified()).await;
        }
    }

    /// Read a tenant by id purely in-memory.
    pub async fn get_tenant(&self, tenant_id: &str) -> Option<Tenant> {
        let guard = self.tenants.read().await;
        guard.get(tenant_id).cloned()
    }

    /// Read an engine type for a model (uid, name, or `served_model_name`) purely in-memory.
    pub async fn get_engine_type(&self, model: &str) -> Option<String> {
        let guard = self.models.read().await;
        guard.by_alias.get(model).cloned()
    }

    /// Upsert or replace a tenant in the local snapshot (sole writer: `gateway_tenants_sync_loop`).
    pub async fn upsert_tenant(&self, tenant: Tenant) {
        let mut guard = self.tenants.write().await;
        guard.insert(tenant.tenant_id.clone(), tenant);
    }

    /// Remove a tenant from the local snapshot.
    pub async fn remove_tenant(&self, tenant_id: &str) {
        let mut guard = self.tenants.write().await;
        guard.remove(tenant_id);
    }

    /// Replace all tenants atomically (e.g. on full snapshot refresh) and mark synced.
    pub async fn replace_all_tenants(&self, list: Vec<Tenant>) {
        let mut map = HashMap::with_capacity(list.len());
        for t in list {
            map.insert(t.tenant_id.clone(), t);
        }
        let mut guard = self.tenants.write().await;
        *guard = map;
        drop(guard);
        self.tenants_synced.store(true, Ordering::Release);
        self.sync_notify.notify_waiters();
    }

    /// Upsert or update model engine type and all aliases in the local snapshot.
    pub async fn upsert_model_spec(&self, spec: &ModelSpec) {
        let mut guard = self.models.write().await;
        guard.upsert(spec);
    }

    /// Remove a model spec and all of its aliases (`model_uid`, `model_name`, `served_model_name`).
    pub async fn remove_model_spec(&self, model_uid: &str) {
        let mut guard = self.models.write().await;
        guard.remove(model_uid);
    }

    /// Replace all model specs atomically and mark synced.
    pub async fn replace_all_model_specs(&self, list: Vec<ModelSpec>) {
        let mut guard = self.models.write().await;
        guard.replace_all(list);
        drop(guard);
        self.models_synced.store(true, Ordering::Release);
        self.sync_notify.notify_waiters();
    }
}

/// Background loop watching `/tenants/` and keeping `GatewayMetaSnapshot` updated.
/// This is the sole writer for tenant entries in `GatewayMetaSnapshot`.
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
        for (k, v, _rev) in items {
            match serde_json::from_slice::<Tenant>(&v) {
                Ok(tenant) => tenants.push(tenant),
                Err(e) => {
                    tracing::warn!(key=%k, error=%e, "gateway: skipping malformed tenant JSON in snapshot");
                }
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
                Some(v) => match serde_json::from_slice::<Tenant>(&v) {
                    Ok(tenant) => snapshot.upsert_tenant(tenant).await,
                    Err(e) => {
                        tracing::warn!(key=%ev.key, error=%e, "gateway: skipping malformed tenant JSON in watch");
                    }
                },
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

/// Background loop watching `/models/` specs and keeping `GatewayMetaSnapshot` updated.
/// This is the sole writer for model spec entries in `GatewayMetaSnapshot`.
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
                match serde_json::from_slice::<ModelSpec>(&v) {
                    Ok(spec) => specs.push(spec),
                    Err(e) => {
                        tracing::warn!(key=%k, error=%e, "gateway: skipping malformed model spec JSON in snapshot");
                    }
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
            let model_uid = ev
                .key
                .strip_prefix("/models/")
                .and_then(|r| r.strip_suffix("/spec"))
                .unwrap_or("")
                .to_string();

            match ev.value {
                Some(v) => match serde_json::from_slice::<ModelSpec>(&v) {
                    Ok(spec) => snapshot.upsert_model_spec(&spec).await,
                    Err(e) => {
                        tracing::warn!(key=%ev.key, error=%e, "gateway: skipping malformed model spec JSON in watch");
                    }
                },
                None => {
                    if !model_uid.is_empty() {
                        snapshot.remove_model_spec(&model_uid).await;
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
    use nebula_common::ModelConfig;

    fn sample_spec(uid: &str, name: &str, served: Option<&str>, engine: Option<&str>) -> ModelSpec {
        ModelSpec {
            model_uid: uid.into(),
            model_name: name.into(),
            model_source: nebula_common::ModelSource::Local,
            model_path: None,
            engine_type: engine.map(|s| s.into()),
            docker_image: None,
            config: served.map(|s| ModelConfig {
                served_model_name: Some(s.into()),
                ..Default::default()
            }),
            labels: Default::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
            created_by: None,
        }
    }

    fn sample_tenant(id: &str) -> Tenant {
        Tenant {
            tenant_id: id.into(),
            display_name: id.into(),
            enabled: true,
            quotas: Default::default(),
            api_token_principals: vec![],
            priority_default: None,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[tokio::test]
    async fn snapshot_tracks_synced_flags_and_aliases() {
        let snap = GatewayMetaSnapshot::new();
        assert!(!snap.is_tenants_synced());
        assert!(!snap.is_models_synced());

        snap.replace_all_tenants(vec![sample_tenant("t1")]).await;
        assert!(snap.is_tenants_synced());
        assert_eq!(snap.get_tenant("t1").await.unwrap().tenant_id, "t1");
        assert!(snap.get_tenant("missing").await.is_none());

        let spec = sample_spec("m1", "llama-3", Some("llama-3-served"), Some("vllm"));
        snap.replace_all_model_specs(vec![spec]).await;
        assert!(snap.is_models_synced());
        assert_eq!(snap.get_engine_type("m1").await.as_deref(), Some("vllm"));
        assert_eq!(
            snap.get_engine_type("llama-3").await.as_deref(),
            Some("vllm")
        );
        assert_eq!(
            snap.get_engine_type("llama-3-served").await.as_deref(),
            Some("vllm")
        );

        // Updating model_name / served_model_name must clean up old aliases.
        let updated = sample_spec("m1", "llama-3.1", Some("llama-31-served"), Some("sglang"));
        snap.upsert_model_spec(&updated).await;
        assert!(snap.get_engine_type("llama-3").await.is_none());
        assert!(snap.get_engine_type("llama-3-served").await.is_none());
        assert_eq!(snap.get_engine_type("m1").await.as_deref(), Some("sglang"));
        assert_eq!(
            snap.get_engine_type("llama-3.1").await.as_deref(),
            Some("sglang")
        );
        assert_eq!(
            snap.get_engine_type("llama-31-served").await.as_deref(),
            Some("sglang")
        );

        // Removing the model spec must clean up ALL aliases, not just model_uid.
        snap.remove_model_spec("m1").await;
        assert!(snap.get_engine_type("m1").await.is_none());
        assert!(snap.get_engine_type("llama-3.1").await.is_none());
        assert!(snap.get_engine_type("llama-31-served").await.is_none());

        snap.remove_tenant("t1").await;
        assert!(snap.get_tenant("t1").await.is_none());
    }

    #[tokio::test]
    async fn wait_initial_sync_wakes_when_both_ready() {
        let snap = GatewayMetaSnapshot::new();
        let snap_bg = snap.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(15)).await;
            snap_bg.replace_all_tenants(vec![]).await;
            snap_bg.replace_all_model_specs(vec![]).await;
        });
        let (t, m) = snap.wait_initial_sync(Duration::from_secs(1)).await;
        assert!(t && m);
    }
}
