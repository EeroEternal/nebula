use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;

use nebula_common::EndpointStats;

use super::{
    configure_process_group, container_name, find_available_port, kill_process_group,
    parse_yaml_defaults, stop_docker_container_by_name, wait_engine_ready, Engine, EngineHandle,
    EngineProcess, EngineStartContext,
};
use crate::util::now_ms;

/// Resolve host mount path and in-container model path for Docker mode.
fn resolve_docker_model_mount(model_tag: &str, model_dir: &str) -> (String, String) {
    let path = std::path::Path::new(model_tag);
    if path.is_absolute() {
        return (model_tag.to_string(), "/model".to_string());
    }
    if model_tag.starts_with(model_dir) {
        return (
            model_dir.to_string(),
            model_tag.replacen(model_dir, "/model", 1),
        );
    }
    (model_dir.to_string(), model_tag.to_string())
}

/// DeepSeek V4 requires flashinfer and a bash entrypoint when run in Docker.
fn needs_deepseek_v4_docker_wrapper(model_tag: &str, vllm_args: &[String]) -> bool {
    if vllm_args
        .windows(2)
        .any(|w| w[0] == "--kv-cache-dtype" && w[1].starts_with("fp8"))
    {
        return true;
    }
    let config_path = std::path::Path::new(model_tag).join("config.json");
    if let Ok(text) = std::fs::read_to_string(config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            return v.get("model_type").and_then(|t| t.as_str()) == Some("deepseek_v4");
        }
    }
    false
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Inspect a named Docker container; returns (base_url, healthy) when running.
async fn inspect_running_container(name: &str) -> Option<(String, bool)> {
    let output = Command::new("docker")
        .args([
            "inspect",
            "-f",
            "{{.State.Running}} {{range .NetworkSettings.Ports}}{{(index . 0).HostPort}}{{end}}",
            name,
        ])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim();
    if !stdout.starts_with("true ") {
        return None;
    }

    let port_str = stdout.strip_prefix("true ")?.trim();
    let _port: u16 = port_str.parse().ok()?;
    let base_url = format!("http://127.0.0.1:{_port}");

    let http = nebula_common::health_http_client().ok()?;
    let health_url = format!("{base_url}/health");
    let healthy = matches!(
        http.get(&health_url).send().await,
        Ok(resp) if resp.status().is_success()
    );

    Some((base_url, healthy))
}

/// vLLM-specific configuration extracted from Node CLI args.
/// These are the vllm_* parameters that used to live directly in Args.
#[derive(Debug, Clone)]
pub struct VllmConfig {
    pub bin: String,
    pub cwd: String,
    pub host: String,
    pub docker_image: Option<String>,
    pub model_dir: String,
    pub use_modelscope: bool,
    pub hf_endpoint: Option<String>,
    pub gpu_memory_utilization: Option<f32>,
    pub max_model_len: Option<u32>,
    pub swap_space: Option<u32>,
    pub max_num_batched_tokens: Option<u32>,
    pub max_num_seqs: Option<u32>,
    pub tensor_parallel_size: Option<u32>,
}

pub struct VllmEngine {
    pub config: VllmConfig,
}

impl VllmEngine {
    pub fn new(args: &crate::args::Args) -> Self {
        Self {
            config: VllmConfig {
                bin: args.vllm_bin.clone(),
                cwd: args.vllm_cwd.clone(),
                host: args.vllm_host.clone(),
                docker_image: args.vllm_docker_image.clone(),
                model_dir: args.vllm_model_dir.clone(),
                use_modelscope: args.vllm_use_modelscope,
                hf_endpoint: args.vllm_hf_endpoint.clone(),
                gpu_memory_utilization: args.vllm_gpu_memory_utilization,
                max_model_len: args.vllm_max_model_len,
                swap_space: args.vllm_swap_space,
                max_num_batched_tokens: args.vllm_max_num_batched_tokens,
                max_num_seqs: args.vllm_max_num_seqs,
                tensor_parallel_size: args.vllm_tensor_parallel_size,
            },
        }
    }

    fn is_docker(&self) -> bool {
        self.config.docker_image.is_some()
    }
}

#[async_trait]
impl Engine for VllmEngine {
    fn engine_type(&self) -> &str {
        "vllm"
    }

    fn capabilities(&self) -> nebula_common::EngineCapability {
        nebula_common::static_capability_vllm()
    }

    async fn start(&self, ctx: EngineStartContext) -> anyhow::Result<EngineHandle> {
        let cfg = parse_yaml_defaults(&ctx.engine_config_path).await;
        let model_tag = cfg
            .get("model")
            .cloned()
            .unwrap_or_else(|| ctx.model_name.clone());
        let runner = cfg.get("runner").cloned();
        let served_model_name = cfg
            .get("served-model-name")
            .cloned()
            .or_else(|| cfg.get("served_model_name").cloned());
        let cfg_gpu_memory_utilization: Option<f32> = cfg
            .get("gpu-memory-utilization")
            .or_else(|| cfg.get("gpu_memory_utilization"))
            .and_then(|s| s.parse::<f32>().ok());

        let selected_port = find_available_port(ctx.port, 64).await?;
        if selected_port != ctx.port {
            tracing::warn!(
                requested_port = ctx.port,
                selected_port,
                "requested port is busy, using another port"
            );
        }
        let base_url = format!("http://127.0.0.1:{}", selected_port);

        tracing::info!(
            replica_id=ctx.replica_id,
            port=selected_port,
            config=%ctx.engine_config_path,
            model=%model_tag,
            "starting vllm engine"
        );

        // Collect common vLLM args
        let mut vllm_args: Vec<String> = Vec::new();
        if let Some(extra) = ctx.extra_args.as_ref() {
            vllm_args.extend(extra.clone());
        }
        if let Some(runner) = runner.as_deref() {
            vllm_args.push("--runner".into());
            vllm_args.push(runner.into());
        }
        if let Some(name) = served_model_name.as_deref() {
            vllm_args.push("--served-model-name".into());
            vllm_args.push(name.into());
        }
        // Determine tensor parallel size: from gpu_indices, or from config
        let tp_size = if let Some(ref indices) = ctx.gpu_indices {
            if indices.len() > 1 {
                Some(indices.len() as u32)
            } else {
                self.config.tensor_parallel_size
            }
        } else {
            self.config.tensor_parallel_size
        };
        if let Some(tp) = tp_size {
            vllm_args.push("--tensor-parallel-size".into());
            vllm_args.push(tp.to_string());
        }
        let gpu_memory_utilization = self
            .config
            .gpu_memory_utilization
            .or(cfg_gpu_memory_utilization);
        if let Some(v) = gpu_memory_utilization {
            vllm_args.push("--gpu-memory-utilization".into());
            vllm_args.push(v.to_string());
        }
        if let Some(v) = self.config.max_num_batched_tokens {
            vllm_args.push("--max-num-batched-tokens".into());
            vllm_args.push(v.to_string());
        }
        if let Some(v) = self.config.max_num_seqs {
            vllm_args.push("--max-num-seqs".into());
            vllm_args.push(v.to_string());
        }
        if let Some(v) = self.config.swap_space {
            vllm_args.push("--swap-space".into());
            vllm_args.push(v.to_string());
        }

        let mut cmd;
        let process_kind;

        if let Some(image) = self.config.docker_image.as_deref() {
            // Docker mode
            let cname = container_name(&ctx.model_uid, ctx.replica_id);
            // Stop & remove any previous container with the same name
            stop_docker_container_by_name(&cname).await;
            // Wait briefly for port release after container removal
            tokio::time::sleep(Duration::from_millis(500)).await;

            let gpu_device = if let Some(ref indices) = ctx.gpu_indices {
                let devs: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                format!("\"device={}\"", devs.join(","))
            } else {
                "all".to_string()
            };

            let (host_mount, container_model) =
                resolve_docker_model_mount(&model_tag, &self.config.model_dir);
            let use_wrapper = needs_deepseek_v4_docker_wrapper(&model_tag, &vllm_args);

            tracing::info!(
                image=%image,
                container=%cname,
                gpu=%gpu_device,
                model=%container_model,
                host_mount=%host_mount,
                deepseek_wrapper=%use_wrapper,
                "launching vLLM via docker"
            );

            cmd = Command::new("docker");
            cmd.arg("run")
                .arg("--name")
                .arg(&cname)
                .arg("--gpus")
                .arg(&gpu_device)
                .arg("-p")
                .arg(format!("{}:{}", selected_port, selected_port))
                .arg("-v")
                .arg(format!("{host_mount}:/model"));

            if self.config.use_modelscope {
                cmd.arg("-e").arg("VLLM_USE_MODELSCOPE=True");
            }
            if let Some(ep) = self.config.hf_endpoint.as_deref() {
                cmd.arg("-e").arg(format!("HF_ENDPOINT={ep}"));
            }

            cmd.arg("-e").arg("HF_HOME=/model/.cache/huggingface");
            cmd.arg("-e")
                .arg("TRANSFORMERS_CACHE=/model/.cache/huggingface");
            cmd.arg("-e").arg("XDG_CACHE_HOME=/model/.cache");
            cmd.arg("-e").arg("HF_HUB_DISABLE_XET=1");

            if use_wrapper {
                cmd.arg("-e").arg("FLASHINFER_DISABLE_VERSION_CHECK=1");
            }

            if use_wrapper {
                cmd.arg("--entrypoint").arg("bash");
            }

            cmd.arg(image);

            if use_wrapper {
                let mut serve_args = vec![
                    "serve".to_string(),
                    container_model.clone(),
                    "--host".to_string(),
                    "0.0.0.0".to_string(),
                    "--port".to_string(),
                    selected_port.to_string(),
                ];
                serve_args.extend(vllm_args.clone());
                let args_str = serve_args
                    .iter()
                    .map(|a| shell_escape(a))
                    .collect::<Vec<_>>()
                    .join(" ");
                let script = format!(
                    "set -e\npip install --no-deps -q flashinfer-python==0.6.14 2>/dev/null || true\nexport FLASHINFER_DISABLE_VERSION_CHECK=1\nexec vllm {args_str}"
                );
                cmd.arg("-lc").arg(script);
            } else {
                cmd.arg("--model")
                    .arg(&container_model)
                    .arg("--host")
                    .arg("0.0.0.0")
                    .arg("--port")
                    .arg(selected_port.to_string());

                for a in &vllm_args {
                    cmd.arg(a);
                }
            }

            process_kind = "docker";
        } else {
            // Local binary mode
            tracing::info!("Using vllm binary: {}", self.config.bin);
            cmd = Command::new(&self.config.bin);
            if self.config.use_modelscope {
                cmd.env("VLLM_USE_MODELSCOPE", "True");
            }
            if let Some(ep) = self.config.hf_endpoint.as_deref() {
                cmd.env("HF_ENDPOINT", ep);
            }
            cmd.env("HF_HUB_DISABLE_XET", "1");
            if let Some(ref indices) = ctx.gpu_indices {
                let devs: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                cmd.env("CUDA_VISIBLE_DEVICES", devs.join(","));
            }
            cmd.current_dir(&self.config.cwd);
            cmd.arg("serve")
                .arg(&model_tag)
                .arg("--host")
                .arg(&self.config.host)
                .arg("--port")
                .arg(selected_port.to_string());

            for a in &vllm_args {
                cmd.arg(a);
            }

            configure_process_group(&mut cmd);
            process_kind = "local";
        }

        let mut child = cmd.spawn()?;

        let ready = tokio::select! {
            r = wait_engine_ready(&base_url, ctx.ready_timeout) => r,
            status = child.wait() => {
                let status = status?;
                anyhow::bail!("vllm exited early: {status}");
            }
        }?;

        let process = if process_kind == "docker" {
            let cname = container_name(&ctx.model_uid, ctx.replica_id);
            // The original child is `docker run` which stays alive.
            // We keep it as a DockerContainer variant.
            EngineProcess::DockerContainer {
                name: cname,
                wait_child: child,
            }
        } else {
            EngineProcess::Child(child)
        };

        Ok(EngineHandle {
            base_url,
            engine_model: ready,
            process,
        })
    }

    async fn try_reuse(&self, ctx: &EngineStartContext) -> Option<EngineHandle> {
        if !self.is_docker() {
            return None;
        }

        let name = container_name(&ctx.model_uid, ctx.replica_id);
        let (base_url, healthy) = inspect_running_container(&name).await?;

        if healthy {
            tracing::info!(%name, %base_url, "reusing existing healthy container");
        } else {
            tracing::info!(
                %name,
                %base_url,
                "container running but not healthy yet; waiting instead of recreating"
            );
        }

        let http = nebula_common::health_http_client().ok()?;
        let models_url = format!("{base_url}/v1/models");
        let engine_model = if healthy {
            match http.get(&models_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let v: serde_json::Value =
                        resp.json().await.unwrap_or(serde_json::Value::Null);
                    v.get("data")
                        .and_then(|d| d.get(0))
                        .and_then(|m| m.get("id"))
                        .and_then(|id| id.as_str())
                        .unwrap_or_default()
                        .to_string()
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };

        let wait_child = Command::new("docker")
            .args(["wait", &name])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()?;

        Some(EngineHandle {
            base_url,
            engine_model,
            process: EngineProcess::DockerContainer {
                name: name.clone(),
                wait_child,
            },
        })
    }

    async fn stop(&self, handle: &mut EngineHandle) -> anyhow::Result<()> {
        match &mut handle.process {
            EngineProcess::Child(child) => {
                kill_process_group(child, Duration::from_secs(10)).await?;
            }
            EngineProcess::DockerContainer { name, wait_child } => {
                tracing::info!(%name, "stopping docker container");
                let _ = Command::new("docker")
                    .args(["stop", "-t", "10", name])
                    .output()
                    .await;
                let _ = Command::new("docker")
                    .args(["rm", "-f", name])
                    .output()
                    .await;
                let _ = wait_child.kill().await;
            }
            EngineProcess::External => {}
        }
        Ok(())
    }

    async fn health_check(&self, handle: &EngineHandle) -> bool {
        let http = nebula_common::health_http_client().unwrap_or_default();
        let health_url = format!("{}/health", handle.base_url);
        match http.get(&health_url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    async fn scrape_stats(
        &self,
        http: &reqwest::Client,
        handle: &EngineHandle,
        model_uid: &str,
        replica_id: u32,
    ) -> super::ScrapeResult {
        scrape_vllm_stats(http, &handle.base_url, model_uid, replica_id).await
    }

    async fn try_restart(
        &self,
        handle: &mut EngineHandle,
        ctx: &EngineStartContext,
    ) -> anyhow::Result<()> {
        match &handle.process {
            EngineProcess::DockerContainer { name, .. } => {
                tracing::warn!(%name, "attempting docker restart");
                let status = Command::new("docker")
                    .args(["restart", "-t", "10", name])
                    .status()
                    .await?;
                if !status.success() {
                    anyhow::bail!("docker restart failed for {name}");
                }
                Ok(())
            }
            EngineProcess::Child(_) => {
                tracing::warn!(
                    model_uid=%ctx.model_uid,
                    replica_id=ctx.replica_id,
                    "restarting local vllm via stop+start (process group)"
                );
                self.stop(handle).await?;
                *handle = self.start(ctx.clone()).await?;
                Ok(())
            }
            EngineProcess::External => {
                anyhow::bail!("cannot restart external engine")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// vLLM-specific metrics scraping (moved from scrape.rs)
// ---------------------------------------------------------------------------

/// Parse a Prometheus `/metrics` body into `EndpointStats`.
pub fn parse_vllm_metrics_text(
    text: &str,
    model_uid: &str,
    replica_id: u32,
    last_updated_ms: u64,
) -> EndpointStats {
    let mut pending_requests: u64 = 0;
    let mut running_requests: u64 = 0;
    let mut kv_cache_usage: Option<f64> = None;
    let mut prefix_cache_hit_rate: Option<f64> = None;
    let mut prefix_cache_hits: Option<f64> = None;
    let mut prefix_cache_queries: Option<f64> = None;

    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }

        // vLLM metric formats (v0.11+):
        //   vllm:num_requests_waiting{...} 3
        //   vllm:num_requests_running{...} 1
        //   vllm:kv_cache_usage_perc{...} 0.45
        //   vllm:prefix_cache_hits_total{...} 100
        //   vllm:prefix_cache_queries_total{...} 200
        //
        // Older versions may use:
        //   vllm:gpu_cache_usage_perc{...} 0.45
        //   vllm:gpu_prefix_cache_hit_rate{...} 0.8
        //
        // Also handle underscore variants without colon:
        //   vllm_num_requests_waiting{...} 3

        if let Some(val) = extract_metric(line, "num_requests_waiting") {
            pending_requests = val as u64;
        } else if let Some(val) = extract_metric(line, "num_requests_running") {
            running_requests = val as u64;
        } else if let Some(val) = extract_metric(line, "kv_cache_usage_perc") {
            kv_cache_usage = Some(val);
        } else if kv_cache_usage.is_none() {
            // Fallback for older vLLM versions
            if let Some(val) = extract_metric(line, "gpu_cache_usage_perc") {
                kv_cache_usage = Some(val);
            }
        }

        // prefix cache: prefer direct hit_rate gauge, else compute from counters
        if let Some(val) = extract_metric(line, "gpu_prefix_cache_hit_rate") {
            prefix_cache_hit_rate = Some(val);
        } else if let Some(val) = extract_metric(line, "cpu_prefix_cache_hit_rate") {
            if prefix_cache_hit_rate.is_none() {
                prefix_cache_hit_rate = Some(val);
            }
        }
        if let Some(val) = extract_metric(line, "prefix_cache_hits_total") {
            prefix_cache_hits = Some(val);
        } else if let Some(val) = extract_metric(line, "prefix_cache_queries_total") {
            prefix_cache_queries = Some(val);
        }
    }

    // Compute prefix cache hit rate from counters if no direct gauge was found
    if prefix_cache_hit_rate.is_none() {
        if let (Some(hits), Some(queries)) = (prefix_cache_hits, prefix_cache_queries) {
            if queries > 0.0 {
                prefix_cache_hit_rate = Some(hits / queries);
            }
        }
    }

    EndpointStats {
        model_uid: model_uid.to_string(),
        replica_id,
        last_updated_ms,
        pending_requests: pending_requests + running_requests,
        prefix_cache_hit_rate,
        prompt_cache_hit_rate: None,
        kv_cache_usage,
    }
}

/// Scrape vLLM `/metrics` and parse into EndpointStats.
pub async fn scrape_vllm_stats(
    http: &reqwest::Client,
    base_url: &str,
    model_uid: &str,
    replica_id: u32,
) -> super::ScrapeResult {
    let url = format!("{}/metrics", base_url.trim_end_matches('/'));
    let text = match http.get(&url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                tracing::debug!(error=%e, %base_url, "failed to read metrics body");
                return Err(super::ScrapeError::ParseFailed);
            }
        },
        Err(e) => {
            tracing::debug!(error=%e, %base_url, "failed to scrape engine metrics");
            if e.is_timeout() {
                return Err(super::ScrapeError::Timeout);
            }
            return Err(super::ScrapeError::Unreachable);
        }
    };

    Ok(parse_vllm_metrics_text(
        &text,
        model_uid,
        replica_id,
        now_ms(),
    ))
}

/// Extract a numeric value from a Prometheus metric line.
/// Matches lines like:
///   vllm:metric_name{labels...} 123.45
///   vllm_metric_name{labels...} 123.45
///   vllm:metric_name 123.45
fn extract_metric(line: &str, metric_suffix: &str) -> Option<f64> {
    // Check if line contains the metric name
    let has_metric =
        line.contains(&format!(":{metric_suffix}")) || line.contains(&format!("_{metric_suffix}"));

    if !has_metric {
        return None;
    }

    // Value is the last whitespace-separated token
    let value_str = line.rsplit_once(|c: char| c.is_whitespace())?.1;
    value_str.parse::<f64>().ok()
}

#[cfg(test)]
mod docker_mount_tests {
    use super::{needs_deepseek_v4_docker_wrapper, resolve_docker_model_mount};

    #[test]
    fn absolute_local_path_mounts_directly() {
        let (host, container) =
            resolve_docker_model_mount("/data/models/DeepSeek-V4-Flash-0731", "/data/models");
        assert_eq!(host, "/data/models/DeepSeek-V4-Flash-0731");
        assert_eq!(container, "/model");
    }

    #[test]
    fn model_dir_prefix_remaps_container_path() {
        let (host, container) =
            resolve_docker_model_mount("/data/models/foo/bar", "/data/models");
        assert_eq!(host, "/data/models/foo/bar");
        assert_eq!(container, "/model");
    }

    #[test]
    fn fp8_kv_cache_triggers_deepseek_wrapper() {
        assert!(needs_deepseek_v4_docker_wrapper(
            "/tmp/x",
            &["--kv-cache-dtype".into(), "fp8".into()]
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_metric() {
        assert_eq!(
            extract_metric(
                "vllm:num_requests_waiting{model=\"m\"} 3",
                "num_requests_waiting"
            ),
            Some(3.0)
        );
        assert_eq!(
            extract_metric(
                "vllm:kv_cache_usage_perc{engine=\"0\"} 0.45",
                "kv_cache_usage_perc"
            ),
            Some(0.45)
        );
        assert_eq!(
            extract_metric("vllm_gpu_cache_usage_perc{} 0.45", "gpu_cache_usage_perc"),
            Some(0.45)
        );
        assert_eq!(
            extract_metric(
                "vllm:prefix_cache_hits_total{engine=\"0\"} 100.0",
                "prefix_cache_hits_total"
            ),
            Some(100.0)
        );
        assert_eq!(
            extract_metric(
                "vllm:prefix_cache_queries_total{engine=\"0\"} 200.0",
                "prefix_cache_queries_total"
            ),
            Some(200.0)
        );
        assert_eq!(
            extract_metric(
                "# HELP vllm:num_requests_waiting help text",
                "num_requests_waiting"
            ),
            None, // comment lines are skipped before calling this
        );
        assert_eq!(
            extract_metric("unrelated_metric{} 1.0", "num_requests_waiting"),
            None,
        );
    }

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/vllm/{}",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    }

    #[test]
    fn parse_v0_11_basic_fixture() {
        let stats = parse_vllm_metrics_text(&fixture("v0.11_basic.prom"), "m", 0, 1);
        assert_eq!(stats.pending_requests, 3);
        assert!((stats.kv_cache_usage.unwrap() - 0.45).abs() < 1e-9);
        assert!((stats.prefix_cache_hit_rate.unwrap() - 0.5).abs() < 1e-9);
        assert!(stats.prompt_cache_hit_rate.is_none());
    }

    #[test]
    fn parse_legacy_gpu_cache_fixture() {
        let stats = parse_vllm_metrics_text(&fixture("legacy_gpu_cache.prom"), "m", 1, 1);
        assert_eq!(stats.pending_requests, 4);
        assert!((stats.kv_cache_usage.unwrap() - 0.30).abs() < 1e-9);
        assert!((stats.prefix_cache_hit_rate.unwrap() - 0.75).abs() < 1e-9);
    }
}
