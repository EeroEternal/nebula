use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;

use nebula_common::{EndpointInfo, EndpointStats, PlacementPlan};
use nebula_meta::{EtcdMetaStore, MetaStore};

pub async fn endpoints_sync_loop(
    store: EtcdMetaStore,
    router: Arc<nebula_router::Router>,
) -> anyhow::Result<()> {
    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/endpoints/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "failed to list endpoints, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut snapshot: Vec<EndpointInfo> = Vec::new();
        for (_k, v, _rev) in items {
            if let Ok(info) = serde_json::from_slice::<EndpointInfo>(&v) {
                snapshot.push(info);
            }
        }
        router.replace_all_endpoints(snapshot);

        let mut stream = match store.watch_prefix("/endpoints/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch endpoints, will resync");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        while let Some(ev) = stream.next().await {
            if let Some(v) = ev.value {
                if let Ok(info) = serde_json::from_slice::<EndpointInfo>(&v) {
                    router.upsert_endpoint(info);
                }
            } else {
                let parts: Vec<&str> = ev.key.split('/').collect();
                if parts.len() >= 4 {
                    if let Ok(replica_id) = parts[3].parse::<u32>() {
                        router.remove_endpoint(parts[2], replica_id);
                    }
                }
            }
        }

        tracing::warn!("endpoints watch stream ended, full resync");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn placement_sync_loop(
    store: EtcdMetaStore,
    router: Arc<nebula_router::Router>,
) -> anyhow::Result<()> {
    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/placements/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "failed to list placements, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut versions = Vec::new();
        for (_k, v, _rev) in items {
            if let Ok(plan) = serde_json::from_slice::<PlacementPlan>(&v) {
                router.set_model_mapping(&plan.model_uid, &plan.model_name);
                versions.push((plan.model_uid, plan.version));
            }
        }
        router.replace_all_plan_versions(versions);

        let mut stream = match store.watch_prefix("/placements/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch placements, will resync");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        while let Some(ev) = stream.next().await {
            match ev.value {
                Some(v) => {
                    let Ok(plan) = serde_json::from_slice::<PlacementPlan>(&v) else {
                        continue;
                    };
                    router.set_model_mapping(&plan.model_uid, &plan.model_name);
                    router.set_plan_version(&plan.model_uid, plan.version);
                }
                None => {
                    // /placements/{model_uid}
                    let model_uid = ev
                        .key
                        .strip_prefix("/placements/")
                        .unwrap_or(&ev.key)
                        .to_string();
                    if !model_uid.is_empty() {
                        router.clear_plan_version(&model_uid);
                    }
                }
            }
        }

        tracing::warn!("placements watch stream ended, full resync");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// P0-2: control-plane hot path — watch etcd `/stats/` (not xtrace).
pub async fn stats_sync_loop(
    store: EtcdMetaStore,
    router: Arc<nebula_router::Router>,
) -> anyhow::Result<()> {
    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/stats/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "failed to list stats, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut snapshot: Vec<EndpointStats> = Vec::new();
        for (_k, v, _rev) in items {
            if let Ok(stats) = serde_json::from_slice::<EndpointStats>(&v) {
                snapshot.push(stats);
            }
        }
        router.replace_all_stats(snapshot);

        let mut stream = match store.watch_prefix("/stats/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch stats, will resync");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        while let Some(ev) = stream.next().await {
            if let Some(v) = ev.value {
                if let Ok(stats) = serde_json::from_slice::<EndpointStats>(&v) {
                    router.upsert_stats(stats);
                }
            } else {
                // /stats/{model_uid}/{replica_id}
                let parts: Vec<&str> = ev.key.split('/').collect();
                if parts.len() >= 4 {
                    if let Ok(replica_id) = parts[3].parse::<u32>() {
                        router.remove_stats(parts[2], replica_id);
                    }
                }
            }
        }

        tracing::warn!("stats watch stream ended, full resync");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn models_sync_loop(
    store: EtcdMetaStore,
    router: Arc<nebula_router::Router>,
) -> anyhow::Result<()> {
    use nebula_common::ModelSpec;

    loop {
        let (items, snap_rev) = match store.list_prefix_snapshot("/models/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, "failed to list model specs, will retry");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        for (key, v, _rev) in &items {
            if !key.ends_with("/spec") {
                continue;
            }
            if let Ok(spec) = serde_json::from_slice::<ModelSpec>(v) {
                router.register_model_spec(&spec);
            }
        }

        let mut stream = match store.watch_prefix("/models/", Some(snap_rev)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch model specs, will resync");
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
                .and_then(|rest| rest.strip_suffix("/spec"))
                .unwrap_or("")
                .to_string();
            match ev.value {
                Some(v) => {
                    if let Ok(spec) = serde_json::from_slice::<ModelSpec>(&v) {
                        router.register_model_spec(&spec);
                    }
                }
                None => {
                    if !model_uid.is_empty() {
                        router.clear_model_mappings(&model_uid);
                    }
                }
            }
        }

        tracing::warn!("model specs watch stream ended, full resync");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
