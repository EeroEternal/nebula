use serde::{Deserialize, Serialize};

/// A cached model on a specific node.
///
/// Stored in etcd under `/model_cache/{node_id}/{model_name_hash}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCacheEntry {
    /// Node that holds this cache.
    pub node_id: String,

    /// User-readable model name (e.g. "Qwen/Qwen2.5-7B-Instruct").
    pub model_name: String,

    /// Actual path on the node's filesystem.
    pub cache_path: String,

    /// Total size of model files in bytes.
    #[serde(default)]
    pub size_bytes: u64,

    /// Number of model files.
    #[serde(default)]
    pub file_count: u32,

    /// Whether the download is complete (checked via config.json or index file presence).
    #[serde(default)]
    pub complete: bool,

    /// Last access time (filesystem atime or last engine load time), ms since epoch.
    #[serde(default)]
    pub last_accessed_ms: u64,

    /// When this cache entry was first discovered, ms since epoch.
    #[serde(default)]
    pub discovered_at_ms: u64,
}

/// Phase of a model file download.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    Downloading,
    Verifying,
    Complete,
    Failed,
}

/// Transient download progress for a model replica.
///
/// Stored in etcd under `/download_progress/{model_uid}/{replica_id}` with a 30s TTL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub model_uid: String,
    pub replica_id: u32,
    pub node_id: String,
    pub model_name: String,

    /// Current download phase.
    pub phase: DownloadPhase,

    /// Total expected bytes.
    #[serde(default)]
    pub total_bytes: u64,

    /// Bytes downloaded so far.
    #[serde(default)]
    pub downloaded_bytes: u64,

    /// Download progress percentage (0.0 – 100.0).
    #[serde(default)]
    pub progress_pct: f64,

    /// Current download speed in bytes/sec.
    #[serde(default)]
    pub speed_bytes_per_sec: u64,

    /// Estimated time remaining in seconds.
    #[serde(default)]
    pub eta_seconds: u64,

    /// Total number of files to download.
    #[serde(default)]
    pub files_total: u32,

    /// Number of files already downloaded.
    #[serde(default)]
    pub files_done: u32,

    /// Last update timestamp (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,
}

/// Disk status for a node's model directory.
///
/// Stored in etcd under `/node_disk/{node_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDiskStatus {
    pub node_id: String,

    /// Model directory path (e.g. "/DATA/Model").
    pub model_dir: String,

    /// Total disk space in bytes.
    #[serde(default)]
    pub total_bytes: u64,

    /// Used disk space in bytes.
    #[serde(default)]
    pub used_bytes: u64,

    /// Available disk space in bytes.
    #[serde(default)]
    pub available_bytes: u64,

    /// Disk usage percentage (0.0 – 100.0).
    #[serde(default)]
    pub usage_pct: f64,

    /// Total bytes used by model caches.
    #[serde(default)]
    pub model_cache_bytes: u64,

    /// Number of cached models.
    #[serde(default)]
    pub model_count: u32,

    /// Last update timestamp (ms since epoch).
    #[serde(default)]
    pub updated_at_ms: u64,
}

/// Type of disk alert.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertType {
    DiskWarning,
    DiskCritical,
}

/// Disk space alert for a node.
///
/// Stored in etcd under `/alerts/{node_id}/disk_warning` or `/alerts/{node_id}/disk_critical`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskAlert {
    pub node_id: String,
    pub alert_type: AlertType,
    pub message: String,
    pub model_dir: String,

    /// Disk usage percentage at alert time.
    #[serde(default)]
    pub usage_pct: f64,

    /// Available bytes at alert time.
    #[serde(default)]
    pub available_bytes: u64,

    /// Alert creation timestamp (ms since epoch).
    #[serde(default)]
    pub created_at_ms: u64,
}
