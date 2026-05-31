use std::fmt::Write;
use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{routing::get, Json, Router};
use serde::Serialize;
use tokio::process::Command;
use tokio::sync::Mutex;

use nebula_common::GpuStatus;

// ---------------------------------------------------------------------------
// Shared metrics state (written by heartbeat_loop, read by /metrics handler)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct EngineMetricSnapshot {
    pub model_uid: String,
    pub replica_id: u32,
    pub pending_requests: u64,
    pub kv_cache_usage: Option<f64>,
    pub prefix_cache_hit_rate: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct NodeMetricsSnapshot {
    pub gpus: Vec<GpuStatus>,
    pub engines: Vec<EngineMetricSnapshot>,
}

pub type SharedNodeMetrics = Arc<Mutex<NodeMetricsSnapshot>>;

#[derive(Debug, Clone, Serialize)]
pub struct ContainerInfo {
    pub name: String,
    pub image: String,
    pub image_id: String,
    pub status: String,
    pub state: String,
    pub ports: String,
    pub created: String,
    pub model_uid: Option<String>,
    pub replica_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageInfo {
    pub repository: String,
    pub tag: String,
    pub image_id: String,
    pub size: String,
    pub created: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NodeDockerStatus {
    pub containers: Vec<ContainerInfo>,
    pub images: Vec<ImageInfo>,
    pub docker_version: Option<String>,
}

/// Parse container name like "nebula-qwen2-5-0-5b-instruct-0" into (model_uid, replica_id).
fn parse_nebula_container_name(name: &str) -> (Option<String>, Option<u32>) {
    let stripped = name.strip_prefix('/').unwrap_or(name);
    if let Some(rest) = stripped.strip_prefix("nebula-") {
        // Last segment after the final '-' is the replica_id
        if let Some(pos) = rest.rfind('-') {
            let model_uid = &rest[..pos];
            let replica_str = &rest[pos + 1..];
            if let Ok(replica_id) = replica_str.parse::<u32>() {
                return (Some(model_uid.to_string()), Some(replica_id));
            }
        }
    }
    (None, None)
}

async fn list_containers() -> Vec<ContainerInfo> {
    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--format",
            "{{.Names}}\t{{.Image}}\t{{.ID}}\t{{.Status}}\t{{.State}}\t{{.Ports}}\t{{.CreatedAt}}",
            "--filter",
            "name=nebula-",
        ])
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(7, '\t').collect();
            let name = parts.first().unwrap_or(&"").to_string();
            let (model_uid, replica_id) = parse_nebula_container_name(&name);
            ContainerInfo {
                name,
                image: parts.get(1).unwrap_or(&"").to_string(),
                image_id: parts.get(2).unwrap_or(&"").to_string(),
                status: parts.get(3).unwrap_or(&"").to_string(),
                state: parts.get(4).unwrap_or(&"").to_string(),
                ports: parts.get(5).unwrap_or(&"").to_string(),
                created: parts.get(6).unwrap_or(&"").to_string(),
                model_uid,
                replica_id,
            }
        })
        .collect()
}

async fn list_images() -> Vec<ImageInfo> {
    let output = Command::new("docker")
        .args([
            "images",
            "--format",
            "{{.Repository}}\t{{.Tag}}\t{{.ID}}\t{{.Size}}\t{{.CreatedAt}}",
            "--filter",
            "dangling=false",
        ])
        .output()
        .await;

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            ImageInfo {
                repository: parts.first().unwrap_or(&"").to_string(),
                tag: parts.get(1).unwrap_or(&"").to_string(),
                image_id: parts.get(2).unwrap_or(&"").to_string(),
                size: parts.get(3).unwrap_or(&"").to_string(),
                created: parts.get(4).unwrap_or(&"").to_string(),
            }
        })
        .collect()
}

async fn docker_version() -> Option<String> {
    let output = Command::new("docker")
        .args(["version", "--format", "{{.Server.Version}}"])
        .output()
        .await
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

// -- Handlers --

async fn get_containers() -> Json<Vec<ContainerInfo>> {
    Json(list_containers().await)
}

async fn get_images() -> Json<Vec<ImageInfo>> {
    Json(list_images().await)
}

async fn get_docker_status() -> Json<NodeDockerStatus> {
    let (containers, images, version) =
        tokio::join!(list_containers(), list_images(), docker_version());
    Json(NodeDockerStatus {
        containers,
        images,
        docker_version: version,
    })
}

// -- Prometheus /metrics handler --

async fn get_metrics(State(metrics): State<SharedNodeMetrics>) -> impl IntoResponse {
    let snap = metrics.lock().await.clone();
    let mut out = String::with_capacity(1024);

    // GPU metrics
    if !snap.gpus.is_empty() {
        let _ = writeln!(
            out,
            "# HELP nebula_node_gpu_temperature GPU temperature in Celsius."
        );
        let _ = writeln!(out, "# TYPE nebula_node_gpu_temperature gauge");
        for gpu in &snap.gpus {
            if let Some(temp) = gpu.temperature_c {
                let _ = writeln!(
                    out,
                    "nebula_node_gpu_temperature{{gpu_index=\"{}\"}} {}",
                    gpu.index, temp
                );
            }
        }

        let _ = writeln!(
            out,
            "# HELP nebula_node_gpu_utilization GPU compute utilization percentage."
        );
        let _ = writeln!(out, "# TYPE nebula_node_gpu_utilization gauge");
        for gpu in &snap.gpus {
            if let Some(util) = gpu.utilization_gpu {
                let _ = writeln!(
                    out,
                    "nebula_node_gpu_utilization{{gpu_index=\"{}\"}} {}",
                    gpu.index, util
                );
            }
        }

        let _ = writeln!(
            out,
            "# HELP nebula_node_gpu_memory_used_mb GPU memory used in MiB."
        );
        let _ = writeln!(out, "# TYPE nebula_node_gpu_memory_used_mb gauge");
        for gpu in &snap.gpus {
            let _ = writeln!(
                out,
                "nebula_node_gpu_memory_used_mb{{gpu_index=\"{}\"}} {}",
                gpu.index, gpu.memory_used_mb
            );
        }

        let _ = writeln!(
            out,
            "# HELP nebula_node_gpu_memory_total_mb GPU total memory in MiB."
        );
        let _ = writeln!(out, "# TYPE nebula_node_gpu_memory_total_mb gauge");
        for gpu in &snap.gpus {
            let _ = writeln!(
                out,
                "nebula_node_gpu_memory_total_mb{{gpu_index=\"{}\"}} {}",
                gpu.index, gpu.memory_total_mb
            );
        }
    }

    // Engine metrics
    if !snap.engines.is_empty() {
        let _ = writeln!(
            out,
            "# HELP nebula_node_engine_pending_requests Number of pending requests."
        );
        let _ = writeln!(out, "# TYPE nebula_node_engine_pending_requests gauge");
        for eng in &snap.engines {
            let _ = writeln!(
                out,
                "nebula_node_engine_pending_requests{{model_uid=\"{}\",replica_id=\"{}\"}} {}",
                eng.model_uid, eng.replica_id, eng.pending_requests
            );
        }

        let _ = writeln!(
            out,
            "# HELP nebula_node_engine_kv_cache_usage KV cache usage ratio."
        );
        let _ = writeln!(out, "# TYPE nebula_node_engine_kv_cache_usage gauge");
        for eng in &snap.engines {
            if let Some(usage) = eng.kv_cache_usage {
                let _ = writeln!(
                    out,
                    "nebula_node_engine_kv_cache_usage{{model_uid=\"{}\",replica_id=\"{}\"}} {}",
                    eng.model_uid, eng.replica_id, usage
                );
            }
        }

        let _ = writeln!(
            out,
            "# HELP nebula_node_engine_prefix_cache_hit_rate Prefix cache hit rate."
        );
        let _ = writeln!(out, "# TYPE nebula_node_engine_prefix_cache_hit_rate gauge");
        for eng in &snap.engines {
            if let Some(rate) = eng.prefix_cache_hit_rate {
                let _ = writeln!(
                    out,
                    "nebula_node_engine_prefix_cache_hit_rate{{model_uid=\"{}\",replica_id=\"{}\"}} {}",
                    eng.model_uid, eng.replica_id, rate
                );
            }
        }
    }

    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    )
}

/// Build the Node API router.
pub fn node_api_router(metrics: SharedNodeMetrics) -> Router {
    Router::new()
        .route("/api/containers", get(get_containers))
        .route("/api/images", get(get_images))
        .route("/api/docker", get(get_docker_status))
        .route("/metrics", get(get_metrics))
        .with_state(metrics)
}
