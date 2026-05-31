pub mod sglang;
pub mod r#virtual;
pub mod vllm;

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use nebula_common::EndpointStats;
use tokio::fs;
use tokio::net::TcpListener;
use tokio::process::{Child, Command};

use crate::args::Args;

/// Context needed to start an engine instance (engine-agnostic).
pub struct EngineStartContext {
    pub model_uid: String,
    pub model_name: String,
    pub replica_id: u32,
    pub port: u16,
    pub gpu_indices: Option<Vec<u32>>,
    pub engine_config_path: String,
    pub extra_args: Option<Vec<String>>,
    pub ready_timeout: Duration,
}

/// Handle to a running engine instance.
pub struct EngineHandle {
    pub base_url: String,
    pub engine_model: String,
    pub process: EngineProcess,
}

/// How the engine process is managed.
pub enum EngineProcess {
    /// A locally spawned child process.
    Child(Child),
    /// A Docker container identified by name.
    DockerContainer { name: String, wait_child: Child },
    /// Externally managed — Node does not control the lifecycle.
    External,
}

#[async_trait]
pub trait Engine: Send + Sync {
    /// Engine type identifier, e.g. "vllm", "sglang".
    fn engine_type(&self) -> &str;

    /// Start a new engine instance.
    async fn start(&self, ctx: EngineStartContext) -> anyhow::Result<EngineHandle>;

    /// Try to reuse an existing instance (e.g. a running Docker container).
    /// Returns None if reuse is not possible; caller will fall back to start().
    async fn try_reuse(&self, ctx: &EngineStartContext) -> Option<EngineHandle> {
        let _ = ctx;
        None
    }

    /// Stop a running engine instance.
    async fn stop(&self, handle: &mut EngineHandle) -> anyhow::Result<()>;

    /// Health check. Returns true if the engine is healthy.
    async fn health_check(&self, handle: &EngineHandle) -> bool;

    /// Scrape runtime metrics and return unified EndpointStats.
    async fn scrape_stats(
        &self,
        http: &reqwest::Client,
        handle: &EngineHandle,
        model_uid: &str,
        replica_id: u32,
    ) -> Option<EndpointStats>;

    /// Attempt to restart the engine (e.g. docker restart). Default: no-op.
    async fn try_restart(&self, handle: &EngineHandle) {
        let _ = handle;
    }
}

/// Build the standard container name for a model replica.
pub fn container_name(model_uid: &str, replica_id: u32) -> String {
    format!("nebula-{}-{}", model_uid, replica_id)
}

pub async fn write_engine_env(path: &str, base_url: &str, model: &str) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).await?;
    }
    let content = format!(
        "NEBULA_ENGINE_BASE_URL={}\nNEBULA_ENGINE_MODEL={}\n",
        base_url, model
    );
    fs::write(path, content).await?;
    Ok(())
}

pub(crate) async fn find_available_port(start_port: u16, max_tries: u16) -> anyhow::Result<u16> {
    let mut port = start_port;
    for _ in 0..max_tries {
        match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => {
                drop(listener);
                return Ok(port);
            }
            Err(_) => {
                port = port.saturating_add(1);
            }
        }
    }
    anyhow::bail!(
        "no available port found in range [{}, {}]",
        start_port,
        start_port.saturating_add(max_tries)
    );
}

pub(crate) async fn parse_yaml_defaults(path: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    match fs::read_to_string(path).await {
        Ok(content) => {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once(':') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if !k.is_empty() && !v.is_empty() {
                        out.insert(k.to_string(), v.to_string());
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("failed to read config file {}: {}", path, e);
        }
    }
    out
}

/// Wait for an engine to become ready by polling /health and /v1/models.
pub(crate) async fn wait_engine_ready(base_url: &str, timeout: Duration) -> anyhow::Result<String> {
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))
        .build()?;

    let start = tokio::time::Instant::now();
    let health_url = format!("{}/health", base_url.trim_end_matches('/'));
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    loop {
        if start.elapsed() > timeout {
            anyhow::bail!("engine not ready within timeout");
        }

        if let Ok(resp) = http.get(&health_url).send().await {
            if resp.status().is_success() {
                return Ok("healthy".to_string());
            }
        }

        match http.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let v: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
                let model_id = v
                    .get("data")
                    .and_then(|d| d.get(0))
                    .and_then(|m| m.get("id"))
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if !model_id.is_empty() {
                    return Ok(model_id);
                }
            }
            _ => {}
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Stop and remove a Docker container by name (shared across engine implementations).
pub(crate) async fn stop_docker_container_by_name(name: &str) {
    tracing::info!(%name, "stopping docker container");
    let _ = Command::new("docker")
        .args(["stop", "-t", "10", name])
        .output()
        .await;
    let _ = Command::new("docker")
        .args(["rm", "-f", name])
        .output()
        .await;
}

/// Create the appropriate Engine implementation based on engine_type string.
/// Defaults to "vllm" if engine_type is None or unrecognized.
/// If `docker_image_override` is Some, it takes precedence over the node-level CLI arg.
pub fn create_engine(
    args: &Args,
    engine_type: Option<&str>,
    docker_image_override: Option<&str>,
) -> Box<dyn Engine> {
    match engine_type.unwrap_or("vllm") {
        "vllm" => {
            let mut engine = vllm::VllmEngine::new(args);
            if let Some(img) = docker_image_override {
                engine.config.docker_image = Some(img.to_string());
            }
            Box::new(engine)
        }
        "sglang" => {
            let mut engine = sglang::SglangEngine::new(args);
            if let Some(img) = docker_image_override {
                engine.config.docker_image = Some(img.to_string());
            }
            Box::new(engine)
        }
        "virtual" => Box::new(r#virtual::VirtualEngine::new(args)),
        other => {
            tracing::warn!(engine_type=%other, "unknown engine type, falling back to vllm");
            let mut engine = vllm::VllmEngine::new(args);
            if let Some(img) = docker_image_override {
                engine.config.docker_image = Some(img.to_string());
            }
            Box::new(engine)
        }
    }
}
