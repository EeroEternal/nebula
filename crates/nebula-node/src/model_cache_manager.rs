use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use nebula_common::{
    AlertType, DiskAlert, DownloadPhase, DownloadProgress, ModelCacheEntry, ModelSource,
    NodeDiskStatus,
};
use nebula_meta::{EtcdMetaStore, MetaStore};

use crate::util::now_ms;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CACHE_SCAN_INTERVAL: Duration = Duration::from_secs(60);
const DOWNLOAD_PROGRESS_TTL_MS: u64 = 30_000;
const DOWNLOAD_PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_secs(3);
const DISK_WARNING_THRESHOLD: f64 = 85.0;
const DISK_CRITICAL_THRESHOLD: f64 = 95.0;
const MAX_DOWNLOAD_RETRIES: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelGcRequest {
    model_uid: String,
    model_name: String,
    model_path: Option<String>,
    requested_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Cache scan loop (spawned at startup)
// ---------------------------------------------------------------------------

/// Background loop that periodically scans the local model directory and
/// reports cache entries + disk status to etcd.
pub async fn model_cache_scan_loop(store: EtcdMetaStore, node_id: String, model_dir: String) {
    loop {
        if let Err(e) = scan_and_report(&store, &node_id, &model_dir).await {
            tracing::warn!(error=%e, "model cache scan failed");
        }
        tokio::time::sleep(CACHE_SCAN_INTERVAL).await;
    }
}

/// Scan the model directory for cached models and report to etcd.
async fn scan_and_report(
    store: &EtcdMetaStore,
    node_id: &str,
    model_dir: &str,
) -> anyhow::Result<()> {
    process_gc_requests(store, node_id, model_dir).await;

    let base = Path::new(model_dir);
    let mut found_keys: HashSet<String> = HashSet::new();
    let mut total_cache_bytes: u64 = 0;
    let mut model_count: u32 = 0;
    let ts = now_ms();

    // 1. Scan HuggingFace Hub cache layout
    scan_hf_cache(
        store,
        node_id,
        base,
        ts,
        &mut found_keys,
        &mut total_cache_bytes,
        &mut model_count,
    )
    .await;

    // 2. Scan ModelScope cache layout
    scan_modelscope_cache(
        store,
        node_id,
        base,
        ts,
        &mut found_keys,
        &mut total_cache_bytes,
        &mut model_count,
    )
    .await;

    // 3. Scan direct paths
    scan_direct_paths(
        store,
        node_id,
        base,
        ts,
        &mut found_keys,
        &mut total_cache_bytes,
        &mut model_count,
    )
    .await;

    // 4. Report disk status
    report_disk_status(
        store,
        node_id,
        model_dir,
        total_cache_bytes,
        model_count,
        ts,
    )
    .await;

    // 5. Clean stale entries
    clean_stale_entries(store, node_id, &found_keys).await;

    tracing::debug!(model_count, total_cache_bytes, "model cache scan complete");
    Ok(())
}

async fn process_gc_requests(store: &EtcdMetaStore, node_id: &str, model_dir: &str) {
    let prefix = format!("/model_gc_requests/{node_id}/");
    let requests = match store.list_prefix(&prefix).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error=%e, %node_id, "failed to list model GC requests");
            return;
        }
    };

    for (key, val, _) in requests {
        let req = match serde_json::from_slice::<ModelGcRequest>(&val) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error=%e, %key, "invalid model GC request payload");
                continue;
            }
        };

        match purge_model_cache_files(
            store,
            node_id,
            model_dir,
            &req.model_name,
            req.model_path.as_deref(),
        )
        .await
        {
            Ok(removed) => {
                tracing::info!(
                    %node_id,
                    %req.model_uid,
                    model_name=%req.model_name,
                    removed,
                    "model GC request processed"
                );
                let _ = store.delete(&key).await;
            }
            Err(e) => {
                tracing::warn!(
                    error=%e,
                    %node_id,
                    %req.model_uid,
                    model_name=%req.model_name,
                    "failed to process model GC request"
                );
            }
        }
    }
}

async fn purge_model_cache_files(
    store: &EtcdMetaStore,
    node_id: &str,
    model_dir: &str,
    model_name: &str,
    model_path: Option<&str>,
) -> anyhow::Result<usize> {
    let prefix = format!("/model_cache/{node_id}/");
    let entries = store.list_prefix(&prefix).await?;
    let model_dir_canon =
        std::fs::canonicalize(model_dir).unwrap_or_else(|_| Path::new(model_dir).to_path_buf());

    let mut removed = 0usize;
    for (key, val, _) in entries {
        let entry = match serde_json::from_slice::<ModelCacheEntry>(&val) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let matches_name = entry.model_name == model_name
            || model_name
                .rsplit('/')
                .next()
                .map(|tail| entry.model_name == tail)
                .unwrap_or(false)
            || entry
                .model_name
                .rsplit('/')
                .next()
                .map(|tail| tail == model_name)
                .unwrap_or(false);
        let matches_path = model_path.map(|p| p == entry.cache_path).unwrap_or(false);
        if !matches_name && !matches_path {
            continue;
        }

        let cache_path = Path::new(&entry.cache_path);
        if cache_path.exists() {
            let target_canon = match std::fs::canonicalize(cache_path) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error=%e, path=%entry.cache_path, "failed to canonicalize cache path");
                    continue;
                }
            };
            if !target_canon.starts_with(&model_dir_canon) {
                tracing::warn!(
                    path=%entry.cache_path,
                    model_dir=%model_dir,
                    "refusing to delete cache path outside model_dir"
                );
                continue;
            }

            if target_canon.is_dir() {
                std::fs::remove_dir_all(&target_canon)?;
            } else if target_canon.is_file() {
                std::fs::remove_file(&target_canon)?;
            }
            removed += 1;
        }

        let _ = store.delete(&key).await;
    }

    Ok(removed)
}

// ---------------------------------------------------------------------------
// Scan helpers
// ---------------------------------------------------------------------------

/// Scan HuggingFace Hub cache: {model_dir}/.cache/huggingface/hub/models--{org}--{name}/
async fn scan_hf_cache(
    store: &EtcdMetaStore,
    node_id: &str,
    base: &Path,
    ts: u64,
    found_keys: &mut HashSet<String>,
    total_cache_bytes: &mut u64,
    model_count: &mut u32,
) {
    let hf_hub = base.join(".cache/huggingface/hub");
    if !hf_hub.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(&hf_hub) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("models--") {
            continue;
        }
        let model_name = name
            .strip_prefix("models--")
            .unwrap_or(&name)
            .replacen("--", "/", 1);
        let model_path = entry.path();
        let (size, count) = dir_size_and_count(&model_path);
        let complete =
            model_path.join("snapshots").is_dir() && has_config_json_recursive(&model_path);
        let key = cache_etcd_key(node_id, &model_name);
        write_cache_entry(
            store,
            &key,
            node_id,
            &model_name,
            model_path.to_string_lossy().as_ref(),
            size,
            count,
            complete,
            ts,
        )
        .await;
        found_keys.insert(key);
        *total_cache_bytes += size;
        *model_count += 1;
    }
}

/// Scan ModelScope cache: {model_dir}/.cache/modelscope/hub/{org}/{name}/
async fn scan_modelscope_cache(
    store: &EtcdMetaStore,
    node_id: &str,
    base: &Path,
    ts: u64,
    found_keys: &mut HashSet<String>,
    total_cache_bytes: &mut u64,
    model_count: &mut u32,
) {
    let ms_hub = base.join(".cache/modelscope/hub");
    if !ms_hub.is_dir() {
        return;
    }
    let orgs = match std::fs::read_dir(&ms_hub) {
        Ok(e) => e,
        Err(_) => return,
    };
    for org_entry in orgs.flatten() {
        if !org_entry.path().is_dir() {
            continue;
        }
        let org_name = org_entry.file_name().to_string_lossy().to_string();
        let models = match std::fs::read_dir(org_entry.path()) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for model_entry in models.flatten() {
            if !model_entry.path().is_dir() {
                continue;
            }
            let m_name = model_entry.file_name().to_string_lossy().to_string();
            let model_name = format!("{}/{}", org_name, m_name);
            let model_path = model_entry.path();
            let (size, count) = dir_size_and_count(&model_path);
            let complete = has_config_json_recursive(&model_path);
            let key = cache_etcd_key(node_id, &model_name);
            write_cache_entry(
                store,
                &key,
                node_id,
                &model_name,
                model_path.to_string_lossy().as_ref(),
                size,
                count,
                complete,
                ts,
            )
            .await;
            found_keys.insert(key);
            *total_cache_bytes += size;
            *model_count += 1;
        }
    }
}

/// Scan direct paths: {model_dir}/{name}/ (top-level directories that look like models)
async fn scan_direct_paths(
    store: &EtcdMetaStore,
    node_id: &str,
    base: &Path,
    ts: u64,
    found_keys: &mut HashSet<String>,
    total_cache_bytes: &mut u64,
    model_count: &mut u32,
) {
    if !base.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs and the .cache directory
        if dir_name.starts_with('.') {
            continue;
        }
        // Only consider directories that look like model dirs
        if !has_config_json_recursive(&path) && !has_safetensors(&path) {
            continue;
        }
        let model_name = dir_name;
        let (size, count) = dir_size_and_count(&path);
        let complete = has_config_json_recursive(&path);
        let key = cache_etcd_key(node_id, &model_name);
        write_cache_entry(
            store,
            &key,
            node_id,
            &model_name,
            path.to_string_lossy().as_ref(),
            size,
            count,
            complete,
            ts,
        )
        .await;
        found_keys.insert(key);
        *total_cache_bytes += size;
        *model_count += 1;
    }
}

// ---------------------------------------------------------------------------
// etcd key helpers
// ---------------------------------------------------------------------------

/// Build the etcd key for a model cache entry.
fn cache_etcd_key(node_id: &str, model_name: &str) -> String {
    let sanitized = model_name.replace('/', "--");
    format!("/model_cache/{}/{}", node_id, sanitized)
}

/// Write a ModelCacheEntry to etcd.
async fn write_cache_entry(
    store: &EtcdMetaStore,
    key: &str,
    node_id: &str,
    model_name: &str,
    cache_path: &str,
    size_bytes: u64,
    file_count: u32,
    complete: bool,
    ts: u64,
) {
    let entry = ModelCacheEntry {
        node_id: node_id.to_string(),
        model_name: model_name.to_string(),
        cache_path: cache_path.to_string(),
        size_bytes,
        file_count,
        complete,
        last_accessed_ms: ts,
        discovered_at_ms: ts,
    };
    match serde_json::to_vec(&entry) {
        Ok(bytes) => {
            if let Err(e) = store.put(key, bytes, None).await {
                tracing::warn!(error=%e, %key, "failed to write model cache entry");
            }
        }
        Err(e) => tracing::warn!(error=%e, "failed to serialize model cache entry"),
    }
}

/// Clean stale cache entries from etcd that no longer exist on disk.
async fn clean_stale_entries(store: &EtcdMetaStore, node_id: &str, found_keys: &HashSet<String>) {
    let prefix = format!("/model_cache/{}/", node_id);
    let existing = match store.list_prefix(&prefix).await {
        Ok(kvs) => kvs,
        Err(e) => {
            tracing::warn!(error=%e, "failed to list model cache entries for cleanup");
            return;
        }
    };
    for (key, _, _) in existing {
        if !found_keys.contains(&key) {
            tracing::info!(%key, "removing stale model cache entry");
            if let Err(e) = store.delete(&key).await {
                tracing::warn!(error=%e, %key, "failed to delete stale cache entry");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Filesystem utilities
// ---------------------------------------------------------------------------

/// Compute total size and file count of a directory recursively.
fn dir_size_and_count(path: &Path) -> (u64, u32) {
    let mut total_size: u64 = 0;
    let mut count: u32 = 0;
    let _ = dir_size_inner(path, &mut total_size, &mut count);
    (total_size, count)
}

fn dir_size_inner(path: &Path, total_size: &mut u64, count: &mut u32) -> std::io::Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            dir_size_inner(&entry.path(), total_size, count)?;
        } else if ft.is_file() {
            *total_size += entry.metadata()?.len();
            *count += 1;
        }
    }
    Ok(())
}

/// Check if a directory (recursively up to 3 levels) contains a `config.json` file.
fn has_config_json_recursive(path: &Path) -> bool {
    if path.join("config.json").is_file() {
        return true;
    }
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if entry.path().join("config.json").is_file() {
                    return true;
                }
                // Check one more level (e.g. snapshots/{hash}/)
                if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                    for sub in sub_entries.flatten() {
                        if sub.path().is_dir() && sub.path().join("config.json").is_file() {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Check if a directory contains any .safetensors files (top-level only).
fn has_safetensors(path: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".safetensors") || name.ends_with(".safetensors.index.json") {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Disk status reporting
// ---------------------------------------------------------------------------

/// Report disk status to etcd and emit alerts if thresholds are exceeded.
async fn report_disk_status(
    store: &EtcdMetaStore,
    node_id: &str,
    model_dir: &str,
    model_cache_bytes: u64,
    model_count: u32,
    ts: u64,
) {
    let (total_bytes, used_bytes, available_bytes) = match get_disk_usage(model_dir).await {
        Some(v) => v,
        None => {
            tracing::warn!(%model_dir, "failed to get disk usage");
            return;
        }
    };

    let usage_pct = if total_bytes > 0 {
        (used_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    let status = NodeDiskStatus {
        node_id: node_id.to_string(),
        model_dir: model_dir.to_string(),
        total_bytes,
        used_bytes,
        available_bytes,
        usage_pct,
        model_cache_bytes,
        model_count,
        updated_at_ms: ts,
    };

    let key = format!("/node_disk/{}", node_id);
    match serde_json::to_vec(&status) {
        Ok(bytes) => {
            if let Err(e) = store.put(&key, bytes, None).await {
                tracing::warn!(error=%e, %key, "failed to write node disk status");
            }
        }
        Err(e) => tracing::warn!(error=%e, "failed to serialize node disk status"),
    }

    // Check thresholds and emit alerts
    if usage_pct > DISK_CRITICAL_THRESHOLD {
        emit_disk_alert(
            store,
            node_id,
            model_dir,
            AlertType::DiskCritical,
            usage_pct,
            available_bytes,
            ts,
        )
        .await;
    } else if usage_pct > DISK_WARNING_THRESHOLD {
        emit_disk_alert(
            store,
            node_id,
            model_dir,
            AlertType::DiskWarning,
            usage_pct,
            available_bytes,
            ts,
        )
        .await;
    }
}

/// Emit a disk alert to etcd.
async fn emit_disk_alert(
    store: &EtcdMetaStore,
    node_id: &str,
    model_dir: &str,
    alert_type: AlertType,
    usage_pct: f64,
    available_bytes: u64,
    ts: u64,
) {
    let alert_suffix = match alert_type {
        AlertType::DiskWarning => "disk_warning",
        AlertType::DiskCritical => "disk_critical",
    };
    let key = format!("/alerts/{}/{}", node_id, alert_suffix);
    let avail_gb = available_bytes / (1024 * 1024 * 1024);
    let message = format!(
        "Node {} model directory usage at {:.1}% ({} GB available)",
        node_id, usage_pct, avail_gb
    );
    let alert = DiskAlert {
        node_id: node_id.to_string(),
        alert_type,
        message,
        model_dir: model_dir.to_string(),
        usage_pct,
        available_bytes,
        created_at_ms: ts,
    };
    match serde_json::to_vec(&alert) {
        Ok(bytes) => {
            if let Err(e) = store.put(&key, bytes, None).await {
                tracing::warn!(error=%e, %key, "failed to write disk alert");
            }
        }
        Err(e) => tracing::warn!(error=%e, "failed to serialize disk alert"),
    }
}

/// Get disk usage for a path using `df -B1`.
/// Returns (total_bytes, used_bytes, available_bytes) or None on failure.
async fn get_disk_usage(path: &str) -> Option<(u64, u64, u64)> {
    let output = Command::new("df")
        .args(["-B1", path])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        // Try macOS-compatible df (no -B1 flag)
        return get_disk_usage_macos(path).await;
    }

    parse_df_output(&String::from_utf8_lossy(&output.stdout))
}

/// macOS fallback: `df -k` (1K blocks)
async fn get_disk_usage_macos(path: &str) -> Option<(u64, u64, u64)> {
    let output = Command::new("df")
        .args(["-k", path])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().nth(1)?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    // df -k output: Filesystem 1K-blocks Used Available ...
    let total = parts[1].parse::<u64>().ok()? * 1024;
    let used = parts[2].parse::<u64>().ok()? * 1024;
    let available = parts[3].parse::<u64>().ok()? * 1024;
    Some((total, used, available))
}

/// Parse `df -B1` output to extract total, used, available bytes.
fn parse_df_output(output: &str) -> Option<(u64, u64, u64)> {
    // df -B1 output format:
    // Filesystem     1B-blocks         Used    Available Use% Mounted on
    // /dev/sda1      2000000000000 1500000000000 500000000000  75% /DATA
    let line = output.lines().nth(1)?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 {
        return None;
    }
    let total = parts[1].parse::<u64>().ok()?;
    let used = parts[2].parse::<u64>().ok()?;
    let available = parts[3].parse::<u64>().ok()?;
    Some((total, used, available))
}

// ---------------------------------------------------------------------------
// Model downloader (called from reconcile before engine start)
// ---------------------------------------------------------------------------

/// Download model files if they are not already cached locally.
///
/// Returns the path to the model files on success.
#[allow(clippy::too_many_arguments)]
pub async fn download_model_if_needed(
    store: &EtcdMetaStore,
    node_id: &str,
    model_uid: &str,
    model_name: &str,
    model_source: &ModelSource,
    model_path: Option<&str>,
    model_dir: &str,
    replica_id: u32,
    hf_endpoint: Option<&str>,
    use_modelscope: bool,
) -> anyhow::Result<String> {
    let _ = use_modelscope; // reserved for future per-call override

    let effective_model_dir = if matches!(model_source, ModelSource::Local) {
        model_dir
    } else {
        model_path
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .unwrap_or(model_dir)
    };

    if !matches!(model_source, ModelSource::Local) {
        if let Err(e) = std::fs::create_dir_all(effective_model_dir) {
            anyhow::bail!(
                "failed to create model directory '{}': {}",
                effective_model_dir,
                e
            );
        }

        // Unified cache check: look in both HF and ModelScope caches regardless of source.
        if let Some(path) = find_hf_cached_model(model_name, effective_model_dir)
            .or_else(|| find_modelscope_cached_model(model_name, effective_model_dir))
        {
            tracing::info!(%model_uid, %path, source=?model_source, model_dir=%effective_model_dir, "model already cached (unified check)");
            return Ok(path);
        }
    }

    match model_source {
        ModelSource::Local => {
            // For local source, just verify the path exists
            let path = model_path.unwrap_or(model_name);
            let p = Path::new(path);
            if p.exists() {
                tracing::info!(%model_uid, %path, "local model path verified");
                Ok(path.to_string())
            } else {
                anyhow::bail!("local model path does not exist: {}", path)
            }
        }
        ModelSource::HuggingFace => {
            // Try HuggingFace first, fallback to ModelScope on failure
            match download_hf_model(
                store,
                node_id,
                model_uid,
                model_name,
                effective_model_dir,
                replica_id,
                hf_endpoint,
            )
            .await
            {
                Ok(path) => Ok(path),
                Err(hf_err) => {
                    tracing::warn!(
                        %model_uid, error=%hf_err,
                        "HuggingFace download failed, falling back to ModelScope"
                    );
                    download_modelscope_model(
                        store,
                        node_id,
                        model_uid,
                        model_name,
                        effective_model_dir,
                        replica_id,
                    )
                    .await
                    .map_err(|ms_err| {
                        anyhow::anyhow!(
                            "both download sources failed — HuggingFace: {}; ModelScope: {}",
                            hf_err,
                            ms_err
                        )
                    })
                }
            }
        }
        ModelSource::ModelScope => {
            // Try ModelScope first, fallback to HuggingFace on failure
            match download_modelscope_model(
                store,
                node_id,
                model_uid,
                model_name,
                effective_model_dir,
                replica_id,
            )
            .await
            {
                Ok(path) => Ok(path),
                Err(ms_err) => {
                    tracing::warn!(
                        %model_uid, error=%ms_err,
                        "ModelScope download failed, falling back to HuggingFace"
                    );
                    download_hf_model(
                        store,
                        node_id,
                        model_uid,
                        model_name,
                        effective_model_dir,
                        replica_id,
                        hf_endpoint,
                    )
                    .await
                    .map_err(|hf_err| {
                        anyhow::anyhow!(
                            "both download sources failed — ModelScope: {}; HuggingFace: {}",
                            ms_err,
                            hf_err
                        )
                    })
                }
            }
        }
    }
}

/// Check if a HuggingFace model is already cached.
fn find_hf_cached_model(model_name: &str, model_dir: &str) -> Option<String> {
    let base = Path::new(model_dir);
    // Check HF cache layout: .cache/huggingface/hub/models--{org}--{name}/
    let hf_dir_name = format!("models--{}", model_name.replace('/', "--"));
    let hf_path = base.join(".cache/huggingface/hub").join(&hf_dir_name);
    if hf_path.is_dir() && has_config_json_recursive(&hf_path) {
        return Some(hf_path.to_string_lossy().to_string());
    }
    // Check full org/name path: {model_dir}/{model_name} (e.g. /DATA/Model/Qwen/Qwen2.5-0.5B-Instruct)
    if model_name.contains('/') {
        let full_path = base.join(model_name);
        if full_path.is_dir() && has_config_json_recursive(&full_path) {
            return Some(full_path.to_string_lossy().to_string());
        }
    }
    // Check direct path: {model_dir}/{model_name_last_part}/
    if let Some(short_name) = model_name.split('/').last() {
        let direct = base.join(short_name);
        if direct.is_dir() && has_config_json_recursive(&direct) {
            return Some(direct.to_string_lossy().to_string());
        }
    }
    None
}

/// Check if a ModelScope model is already cached.
fn find_modelscope_cached_model(model_name: &str, model_dir: &str) -> Option<String> {
    let base = Path::new(model_dir);
    // Check ModelScope cache layout: .cache/modelscope/hub/{org}/{name}/
    let ms_path = base.join(".cache/modelscope/hub").join(model_name);
    if ms_path.is_dir() && has_config_json_recursive(&ms_path) {
        return Some(ms_path.to_string_lossy().to_string());
    }
    // Check full org/name path: {model_dir}/{model_name} (e.g. /DATA/Model/Qwen/Qwen2.5-0.5B-Instruct)
    if model_name.contains('/') {
        let full_path = base.join(model_name);
        if full_path.is_dir() && has_config_json_recursive(&full_path) {
            return Some(full_path.to_string_lossy().to_string());
        }
    }
    // Check direct path
    if let Some(short_name) = model_name.split('/').last() {
        let direct = base.join(short_name);
        if direct.is_dir() && has_config_json_recursive(&direct) {
            return Some(direct.to_string_lossy().to_string());
        }
    }
    None
}

/// Build a PATH string that prepends `$HOME/.local/bin` to the current PATH.
/// This ensures CLIs installed via `pip install --user` (e.g. huggingface-cli,
/// modelscope) are discoverable even when the system PATH does not include it.
fn augmented_path() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    format!("{}/.local/bin:{}", home, current)
}

/// Download a model from HuggingFace Hub.
async fn download_hf_model(
    store: &EtcdMetaStore,
    node_id: &str,
    model_uid: &str,
    model_name: &str,
    model_dir: &str,
    replica_id: u32,
    hf_endpoint: Option<&str>,
) -> anyhow::Result<String> {
    // Check if already cached
    if let Some(path) = find_hf_cached_model(model_name, model_dir) {
        tracing::info!(%model_uid, %path, "HuggingFace model already cached");
        return Ok(path);
    }

    // Pre-check disk space
    check_disk_space(model_dir).await?;

    tracing::info!(%model_uid, %model_name, "downloading model from HuggingFace");

    let progress_key = format!("/download_progress/{}/{}", model_uid, replica_id);

    // Write initial progress
    write_download_progress(
        store,
        &progress_key,
        model_uid,
        replica_id,
        node_id,
        model_name,
        DownloadPhase::Downloading,
        0,
        0,
        0,
        0,
    )
    .await;

    // Spawn progress monitor that polls filesystem size
    let monitor_store = store.clone();
    let monitor_key = progress_key.clone();
    let monitor_model_uid = model_uid.to_string();
    let monitor_model_name = model_name.to_string();
    let monitor_node_id = node_id.to_string();
    let monitor_model_dir = model_dir.to_string();
    let monitor_hf_name = model_name.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        let hf_dir_name = format!("models--{}", monitor_hf_name.replace('/', "--"));
        let cache_path = Path::new(&monitor_model_dir)
            .join(".cache/huggingface/hub")
            .join(&hf_dir_name);
        loop {
            tokio::time::sleep(DOWNLOAD_PROGRESS_UPDATE_INTERVAL).await;
            if cancel_clone.load(Ordering::Relaxed) {
                break;
            }
            let (downloaded, file_count) = if cache_path.exists() {
                dir_size_and_count(&cache_path)
            } else {
                (0, 0)
            };
            write_download_progress(
                &monitor_store,
                &monitor_key,
                &monitor_model_uid,
                replica_id,
                &monitor_node_id,
                &monitor_model_name,
                DownloadPhase::Downloading,
                0,
                downloaded,
                file_count,
                0,
            )
            .await;
        }
    });

    // Run download with retries
    let mut last_err = String::new();
    for attempt in 0..MAX_DOWNLOAD_RETRIES {
        if attempt > 0 {
            let backoff = Duration::from_secs(2u64.pow(attempt));
            tracing::warn!(%model_uid, attempt, "retrying HuggingFace download after {:?}", backoff);
            tokio::time::sleep(backoff).await;
        }

        let mut cmd = Command::new("huggingface-cli");
        cmd.args(["download", model_name]);
        cmd.env("PATH", augmented_path());
        // Set HF cache dir
        cmd.env("HF_HOME", format!("{}/.cache/huggingface", model_dir));
        if let Some(endpoint) = hf_endpoint {
            cmd.env("HF_ENDPOINT", endpoint);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                cancel.store(true, Ordering::Relaxed);
                // Write completion progress
                write_download_progress(
                    store,
                    &progress_key,
                    model_uid,
                    replica_id,
                    node_id,
                    model_name,
                    DownloadPhase::Complete,
                    0,
                    0,
                    0,
                    0,
                )
                .await;
                // Find the cached path
                if let Some(path) = find_hf_cached_model(model_name, model_dir) {
                    tracing::info!(%model_uid, %path, "HuggingFace model download complete");
                    return Ok(path);
                }
                // Fallback: return the HF cache dir
                let hf_dir_name = format!("models--{}", model_name.replace('/', "--"));
                let path = Path::new(model_dir)
                    .join(".cache/huggingface/hub")
                    .join(&hf_dir_name);
                return Ok(path.to_string_lossy().to_string());
            }
            Ok(output) => {
                last_err = String::from_utf8_lossy(&output.stderr).to_string();
                tracing::warn!(%model_uid, attempt, error=%last_err, "HuggingFace download failed");
            }
            Err(e) => {
                last_err = e.to_string();
                tracing::warn!(%model_uid, attempt, error=%last_err, "failed to run huggingface-cli");
            }
        }
    }

    cancel.store(true, Ordering::Relaxed);
    // Write failure progress
    write_download_progress(
        store,
        &progress_key,
        model_uid,
        replica_id,
        node_id,
        model_name,
        DownloadPhase::Failed,
        0,
        0,
        0,
        0,
    )
    .await;
    anyhow::bail!(
        "HuggingFace download failed after {} retries: {}",
        MAX_DOWNLOAD_RETRIES,
        last_err.lines().last().unwrap_or(&last_err)
    )
}

/// Download a model from ModelScope.
async fn download_modelscope_model(
    store: &EtcdMetaStore,
    node_id: &str,
    model_uid: &str,
    model_name: &str,
    model_dir: &str,
    replica_id: u32,
) -> anyhow::Result<String> {
    // Check if already cached
    if let Some(path) = find_modelscope_cached_model(model_name, model_dir) {
        tracing::info!(%model_uid, %path, "ModelScope model already cached");
        return Ok(path);
    }

    // Pre-check disk space
    check_disk_space(model_dir).await?;

    tracing::info!(%model_uid, %model_name, "downloading model from ModelScope");

    let progress_key = format!("/download_progress/{}/{}", model_uid, replica_id);

    // Write initial progress
    write_download_progress(
        store,
        &progress_key,
        model_uid,
        replica_id,
        node_id,
        model_name,
        DownloadPhase::Downloading,
        0,
        0,
        0,
        0,
    )
    .await;

    // Spawn progress monitor
    let monitor_store = store.clone();
    let monitor_key = progress_key.clone();
    let monitor_model_uid = model_uid.to_string();
    let monitor_model_name = model_name.to_string();
    let monitor_node_id = node_id.to_string();
    let monitor_model_dir = model_dir.to_string();
    let monitor_ms_name = model_name.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        let cache_path = Path::new(&monitor_model_dir)
            .join(".cache/modelscope/hub")
            .join(&monitor_ms_name);
        loop {
            tokio::time::sleep(DOWNLOAD_PROGRESS_UPDATE_INTERVAL).await;
            if cancel_clone.load(Ordering::Relaxed) {
                break;
            }
            let (downloaded, file_count) = if cache_path.exists() {
                dir_size_and_count(&cache_path)
            } else {
                (0, 0)
            };
            write_download_progress(
                &monitor_store,
                &monitor_key,
                &monitor_model_uid,
                replica_id,
                &monitor_node_id,
                &monitor_model_name,
                DownloadPhase::Downloading,
                0,
                downloaded,
                file_count,
                0,
            )
            .await;
        }
    });

    // Run download with retries
    let mut last_err = String::new();
    for attempt in 0..MAX_DOWNLOAD_RETRIES {
        if attempt > 0 {
            let backoff = Duration::from_secs(2u64.pow(attempt));
            tracing::warn!(%model_uid, attempt, "retrying ModelScope download after {:?}", backoff);
            tokio::time::sleep(backoff).await;
        }

        let mut cmd = Command::new("modelscope");
        cmd.args(["download", "--model", model_name]);
        cmd.env("PATH", augmented_path());
        cmd.env(
            "MODELSCOPE_CACHE",
            format!("{}/.cache/modelscope", model_dir),
        );
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                cancel.store(true, Ordering::Relaxed);
                write_download_progress(
                    store,
                    &progress_key,
                    model_uid,
                    replica_id,
                    node_id,
                    model_name,
                    DownloadPhase::Complete,
                    0,
                    0,
                    0,
                    0,
                )
                .await;
                if let Some(path) = find_modelscope_cached_model(model_name, model_dir) {
                    tracing::info!(%model_uid, %path, "ModelScope model download complete");
                    return Ok(path);
                }
                let path = Path::new(model_dir)
                    .join(".cache/modelscope/hub")
                    .join(model_name);
                return Ok(path.to_string_lossy().to_string());
            }
            Ok(output) => {
                last_err = String::from_utf8_lossy(&output.stderr).to_string();
                tracing::warn!(%model_uid, attempt, error=%last_err, "ModelScope download failed");
            }
            Err(e) => {
                last_err = e.to_string();
                tracing::warn!(%model_uid, attempt, error=%last_err, "failed to run modelscope CLI");
            }
        }
    }

    cancel.store(true, Ordering::Relaxed);
    write_download_progress(
        store,
        &progress_key,
        model_uid,
        replica_id,
        node_id,
        model_name,
        DownloadPhase::Failed,
        0,
        0,
        0,
        0,
    )
    .await;
    anyhow::bail!(
        "ModelScope download failed after {} retries: {}",
        MAX_DOWNLOAD_RETRIES,
        last_err.lines().last().unwrap_or(&last_err)
    )
}

// ---------------------------------------------------------------------------
// Progress and disk space helpers
// ---------------------------------------------------------------------------

/// Write download progress to etcd with TTL.
#[allow(clippy::too_many_arguments)]
async fn write_download_progress(
    store: &EtcdMetaStore,
    key: &str,
    model_uid: &str,
    replica_id: u32,
    node_id: &str,
    model_name: &str,
    phase: DownloadPhase,
    total_bytes: u64,
    downloaded_bytes: u64,
    files_done: u32,
    files_total: u32,
) {
    let progress_pct = if total_bytes > 0 {
        (downloaded_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    let progress = DownloadProgress {
        model_uid: model_uid.to_string(),
        replica_id,
        node_id: node_id.to_string(),
        model_name: model_name.to_string(),
        phase,
        total_bytes,
        downloaded_bytes,
        progress_pct,
        speed_bytes_per_sec: 0,
        eta_seconds: 0,
        files_total,
        files_done,
        updated_at_ms: now_ms(),
    };

    match serde_json::to_vec(&progress) {
        Ok(bytes) => {
            if let Err(e) = store.put(key, bytes, Some(DOWNLOAD_PROGRESS_TTL_MS)).await {
                tracing::debug!(error=%e, %key, "failed to write download progress");
            }
        }
        Err(e) => tracing::debug!(error=%e, "failed to serialize download progress"),
    }
}

/// Pre-check disk space before downloading. Returns error if disk is critically full.
async fn check_disk_space(model_dir: &str) -> anyhow::Result<()> {
    if let Some((total, used, _available)) = get_disk_usage(model_dir).await {
        let usage_pct = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        if usage_pct > DISK_CRITICAL_THRESHOLD {
            anyhow::bail!(
                "disk usage at {:.1}% exceeds critical threshold ({:.0}%), refusing to download",
                usage_pct,
                DISK_CRITICAL_THRESHOLD
            );
        }
    }
    Ok(())
}
