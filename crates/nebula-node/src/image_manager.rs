use std::collections::HashSet;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::process::Command;

use nebula_common::{EngineImage, ImagePullStatus, NodeImageStatus, VersionPolicy};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::util::now_ms;

/// Interval between image garbage-collection sweeps.
const GC_INTERVAL_SECS: u64 = 3600; // 1 hour

/// Run the image manager loop:
/// 1. On startup, scan all registered images and pull any that are missing locally.
/// 2. Watch `/images/` for new/updated registrations and pull as needed.
/// 3. Periodically clean up local images that are no longer in the registry.
pub async fn image_manager_loop(store: EtcdMetaStore, node_id: String) {
    // Initial scan: pull all registered images that are missing locally
    let mut start_rev: u64 = 0;
    if let Ok(kvs) = store.list_prefix("/images/").await {
        for (_key, val, rev) in kvs {
            if rev > start_rev {
                start_rev = rev;
            }
            if let Ok(img) = serde_json::from_slice::<EngineImage>(&val) {
                if img.pre_pull {
                    tokio::spawn(pull_if_missing(store.clone(), node_id.clone(), img));
                }
            }
        }
    }

    // Spawn periodic GC task
    let gc_store = store.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(GC_INTERVAL_SECS)).await;
            run_image_gc(&gc_store).await;
        }
    });

    // Watch for new/updated image registrations
    loop {
        tracing::info!(start_rev, "watching /images/ for image registry changes");
        let mut watch = match store.watch_prefix("/images/", Some(start_rev)).await {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error=%e, "failed to watch /images/, retrying");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        while let Some(ev) = watch.next().await {
            if ev.revision > start_rev {
                start_rev = ev.revision;
            }

            match ev.value {
                Some(val) => {
                    if let Ok(img) = serde_json::from_slice::<EngineImage>(&val) {
                        tracing::info!(image_id=%img.id, image=%img.image, "image registry updated");
                        if img.pre_pull {
                            tokio::spawn(pull_if_missing(store.clone(), node_id.clone(), img));
                        }
                    }
                }
                None => {
                    // Image deleted from registry — GC will handle cleanup
                    tracing::info!(key=%ev.key, "image registry entry deleted");
                }
            }
        }

        tracing::warn!("image watch stream ended, reconnecting");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Pull an image if it is not already present locally.
/// Reports status to etcd under `/image_status/{node_id}/{image_id}`.
async fn pull_if_missing(store: EtcdMetaStore, node_id: String, img: EngineImage) {
    let image_ref = &img.image;

    // For rolling images, always re-pull to get latest digest
    let should_pull = match img.version_policy {
        VersionPolicy::Rolling => true,
        VersionPolicy::Pin => !image_exists_locally(image_ref).await,
    };

    if !should_pull {
        tracing::debug!(image=%image_ref, "image already present locally, skipping pull");
        report_status(&store, &node_id, &img, ImagePullStatus::Ready, None).await;
        return;
    }

    tracing::info!(image=%image_ref, policy=?img.version_policy, "pulling image");
    report_status(&store, &node_id, &img, ImagePullStatus::Pulling, None).await;

    match docker_pull(image_ref).await {
        Ok(()) => {
            tracing::info!(image=%image_ref, "image pulled successfully");
            report_status(&store, &node_id, &img, ImagePullStatus::Ready, None).await;
        }
        Err(e) => {
            tracing::error!(image=%image_ref, error=%e, "failed to pull image");
            report_status(
                &store,
                &node_id,
                &img,
                ImagePullStatus::Failed,
                Some(e.to_string()),
            )
            .await;
        }
    }
}

/// Check if a Docker image exists locally.
async fn image_exists_locally(image: &str) -> bool {
    let output = Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;
    matches!(output, Ok(status) if status.success())
}

/// Pull a Docker image.
async fn docker_pull(image: &str) -> anyhow::Result<()> {
    let output = Command::new("docker")
        .args(["pull", image])
        .output()
        .await?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker pull failed: {}", stderr.trim())
    }
}

/// Report image pull status to etcd.
async fn report_status(
    store: &EtcdMetaStore,
    node_id: &str,
    img: &EngineImage,
    status: ImagePullStatus,
    error: Option<String>,
) {
    let key = format!("/image_status/{}/{}", node_id, img.id);
    let record = NodeImageStatus {
        node_id: node_id.to_string(),
        image_id: img.id.clone(),
        image: img.image.clone(),
        status,
        error,
        updated_at_ms: now_ms(),
    };
    match serde_json::to_vec(&record) {
        Ok(bytes) => {
            if let Err(e) = store.put(&key, bytes, None).await {
                tracing::warn!(error=%e, %key, "failed to report image status");
            }
        }
        Err(e) => {
            tracing::warn!(error=%e, "failed to serialize image status");
        }
    }
}

/// Garbage-collect local nebula-related images that are no longer in the registry
/// and not used by any running container.
async fn run_image_gc(store: &EtcdMetaStore) {
    // Collect registered image references
    let registered: HashSet<String> = match store.list_prefix("/images/").await {
        Ok(kvs) => kvs
            .into_iter()
            .filter_map(|(_, val, _)| {
                serde_json::from_slice::<EngineImage>(&val)
                    .ok()
                    .map(|img| img.image)
            })
            .collect(),
        Err(e) => {
            tracing::warn!(error=%e, "image GC: failed to list registry, skipping");
            return;
        }
    };

    if registered.is_empty() {
        tracing::debug!("image GC: no registered images, skipping cleanup");
        return;
    }

    // List local images
    let local_images = list_local_images().await;
    // List images used by running containers
    let in_use = images_in_use().await;

    let mut removed = 0u32;
    for local_ref in &local_images {
        if registered.contains(local_ref) {
            continue; // still registered
        }
        if in_use.contains(local_ref) {
            continue; // in use by a running container
        }
        // Only clean up images that look like engine images (contain "vllm" or "sglang" in name)
        if !is_engine_image(local_ref) {
            continue;
        }

        tracing::info!(image=%local_ref, "image GC: removing unregistered image");
        let _ = Command::new("docker")
            .args(["rmi", local_ref])
            .output()
            .await;
        removed += 1;
    }

    if removed > 0 {
        tracing::info!(removed, "image GC: cleanup complete");
    }
}

/// Heuristic: only GC images that look like inference engine images.
fn is_engine_image(image_ref: &str) -> bool {
    let lower = image_ref.to_lowercase();
    lower.contains("vllm") || lower.contains("sglang") || lower.contains("xllm")
}

/// List all local Docker image references (repository:tag).
async fn list_local_images() -> Vec<String> {
    let output = Command::new("docker")
        .args([
            "images",
            "--format",
            "{{.Repository}}:{{.Tag}}",
            "--filter",
            "dangling=false",
        ])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty() && !l.contains("<none>"))
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// List images currently used by running containers.
async fn images_in_use() -> HashSet<String> {
    let output = Command::new("docker")
        .args(["ps", "--format", "{{.Image}}"])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|s| s.to_string())
            .collect(),
        _ => HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_engine_image() {
        assert!(is_engine_image("vllm/vllm-openai:v0.8.3"));
        assert!(is_engine_image("registry.example.com/sglang:latest"));
        assert!(is_engine_image("my-xllm-image:v1"));
        assert!(!is_engine_image("nginx:latest"));
        assert!(!is_engine_image("postgres:14"));
    }
}
