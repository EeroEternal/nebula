use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;

use super::{
    configure_process_group, container_name, find_available_port, kill_process_group,
    parse_yaml_defaults, stop_docker_container_by_name, wait_engine_ready, Engine, EngineHandle,
    EngineProcess, EngineStartContext,
};
use crate::util::now_ms;

/// zstep-specific configuration extracted from Node CLI args.
#[derive(Debug, Clone)]
pub struct ZstepConfig {
    pub bin: String,
    pub cwd: String,
    pub host: String,
    pub docker_image: Option<String>,
    pub model_dir: String,
    pub tensor_parallel_size: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub max_num_seqs: Option<u32>,
    pub worker_root: Option<String>,
}

pub struct ZstepEngine {
    pub config: ZstepConfig,
}

impl ZstepEngine {
    pub fn new(args: &crate::args::Args) -> Self {
        Self {
            config: ZstepConfig {
                bin: args.zstep_bin.clone(),
                cwd: args.zstep_cwd.clone(),
                host: args.zstep_host.clone(),
                docker_image: args.zstep_docker_image.clone(),
                model_dir: args.zstep_model_dir.clone(),
                tensor_parallel_size: args.zstep_tensor_parallel_size,
                gpu_memory_utilization: args.zstep_gpu_memory_utilization,
                max_num_seqs: args.zstep_max_num_seqs,
                worker_root: args.zstep_worker_root.clone(),
            },
        }
    }

    fn is_docker(&self) -> bool {
        self.config.docker_image.is_some()
    }

    /// Split `--zstep-bin` into executable + prefix args (default: `zs-server serve`).
    fn split_bin(&self) -> (String, Vec<String>) {
        let parts: Vec<&str> = self.config.bin.split_whitespace().collect();
        if parts.is_empty() {
            return ("zs-server".into(), vec!["serve".into()]);
        }
        let program = parts[0].to_string();
        let prefix = parts[1..].iter().map(|s| (*s).to_string()).collect();
        (program, prefix)
    }
}

/// Build `zs-server serve` CLI arguments from `EngineStartContext` and node defaults.
fn build_launch_args(
    cfg: &ZstepConfig,
    ctx: &EngineStartContext,
    yaml_served_model_name: Option<&str>,
    yaml_gpu_util: Option<f32>,
) -> Vec<String> {
    build_launch_args_with_kv_events(
        cfg,
        ctx,
        yaml_served_model_name,
        yaml_gpu_util,
        std::env::var("NEBULA_KV_EVENTS_ENDPOINT").ok().as_deref(),
    )
}

fn build_launch_args_with_kv_events(
    cfg: &ZstepConfig,
    ctx: &EngineStartContext,
    yaml_served_model_name: Option<&str>,
    yaml_gpu_util: Option<f32>,
    kv_events_endpoint: Option<&str>,
) -> Vec<String> {
    let mut zs_args: Vec<String> = Vec::new();
    if let Some(extra) = ctx.extra_args.as_ref() {
        zs_args.extend(extra.clone());
    }
    if !zs_args.iter().any(|a| a == "--served-model-name") {
        let served = yaml_served_model_name.unwrap_or(&ctx.model_uid);
        if !served.is_empty() {
            zs_args.push("--served-model-name".into());
            zs_args.push(served.to_string());
        }
    }
    // `zs-server serve` defaults `--world-size` to 8; always pin to the assigned
    // GPU slice count when `extra_args` did not already set `--world-size`.
    let tp_size = match ctx.gpu_indices.as_ref() {
        Some(indices) if !indices.is_empty() => Some(indices.len() as u32),
        _ => cfg.tensor_parallel_size,
    };
    if let Some(tp) = tp_size {
        if !zs_args.iter().any(|a| a == "--world-size") {
            zs_args.push("--world-size".into());
            zs_args.push(tp.to_string());
        }
    }
    let gpu_util = cfg.gpu_memory_utilization.or(yaml_gpu_util);
    if let Some(v) = gpu_util {
        if !zs_args.iter().any(|a| a == "--gpu-memory-utilization") {
            zs_args.push("--gpu-memory-utilization".into());
            zs_args.push(v.to_string());
        }
    }
    if let Some(v) = cfg.max_num_seqs {
        if !zs_args.iter().any(|a| a == "--max-seqs") {
            zs_args.push("--max-seqs".into());
            zs_args.push(v.to_string());
        }
    }
    if let Some(root) = cfg.worker_root.as_deref() {
        if !zs_args.iter().any(|a| a == "--worker-root") {
            zs_args.push("--worker-root".into());
            zs_args.push(root.to_string());
        }
    }
    if let Some(endpoint) = kv_events_endpoint.map(str::trim).filter(|s| !s.is_empty()) {
        if !zs_args.iter().any(|a| a == "--kv-events-endpoint") {
            zs_args.push("--kv-events-endpoint".into());
            zs_args.push(endpoint.to_string());
        }
        if !zs_args.iter().any(|a| a == "--kv-events-topic") {
            zs_args.push("--kv-events-topic".into());
            zs_args.push(format!("{}/{}", ctx.model_uid, ctx.replica_id));
        }
    }
    zs_args
}

#[async_trait]
impl Engine for ZstepEngine {
    fn engine_type(&self) -> &str {
        "zstep"
    }

    fn capabilities(&self) -> nebula_common::EngineCapability {
        nebula_common::static_capability_zstep()
    }

    async fn start(&self, ctx: EngineStartContext) -> anyhow::Result<EngineHandle> {
        let cfg = parse_yaml_defaults(&ctx.engine_config_path).await;
        let model_tag = cfg
            .get("model")
            .cloned()
            .unwrap_or_else(|| ctx.model_name.clone());
        let served_model_name = cfg
            .get("served-model-name")
            .cloned()
            .or_else(|| cfg.get("served_model_name").cloned());
        let cfg_gpu_util: Option<f32> = cfg
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
            "starting zstep engine"
        );

        let zs_args = build_launch_args(
            &self.config,
            &ctx,
            served_model_name.as_deref(),
            cfg_gpu_util,
        );

        let kv_hash_seed = std::env::var("NEBULA_KV_EVENTS_ENDPOINT")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(|_| std::env::var("NEBULA_KV_HASH_SEED").unwrap_or_else(|_| "42".to_string()));

        let mut cmd;
        let process_kind;

        if let Some(image) = self.config.docker_image.as_deref() {
            let cname = container_name(&ctx.model_uid, ctx.replica_id);
            stop_docker_container_by_name(&cname).await;
            tokio::time::sleep(Duration::from_millis(500)).await;

            let gpu_device = if let Some(ref indices) = ctx.gpu_indices {
                let devs: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                format!("\"device={}\"", devs.join(","))
            } else {
                "all".to_string()
            };

            let container_model = if model_tag.starts_with(&self.config.model_dir) {
                model_tag.replacen(&self.config.model_dir, "/model", 1)
            } else {
                model_tag.clone()
            };

            tracing::info!(
                image=%image,
                container=%cname,
                gpu=%gpu_device,
                model=%container_model,
                "launching zstep via docker"
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
                .arg(format!("{}:/model", self.config.model_dir))
                .arg("--ipc=host");

            cmd.arg("-e").arg("HF_HOME=/model/.cache/huggingface");
            cmd.arg("-e")
                .arg("TRANSFORMERS_CACHE=/model/.cache/huggingface");
            cmd.arg("-e").arg("XDG_CACHE_HOME=/model/.cache");
            cmd.arg("-e").arg("TRITON_CACHE_DIR=/model/.cache/triton");
            cmd.arg("-e")
                .arg("TORCHINDUCTOR_CACHE_DIR=/model/.cache/torchinductor");
            cmd.arg("-e")
                .arg("FLASHINFER_WORKSPACE_BASE=/model/.cache/flashinfer");
            if let Some(ref seed) = kv_hash_seed {
                cmd.arg("-e").arg(format!("NEBULA_KV_HASH_SEED={seed}"));
            }

            let (program, prefix_args) = self.split_bin();
            cmd.arg("--entrypoint").arg(&program).arg(image);
            for a in &prefix_args {
                cmd.arg(a);
            }
            cmd.arg("--model-dir")
                .arg(&container_model)
                .arg("--host")
                .arg("0.0.0.0")
                .arg("--port")
                .arg(selected_port.to_string());

            for a in &zs_args {
                cmd.arg(a);
            }

            process_kind = "docker";
        } else {
            let (program, prefix_args) = self.split_bin();

            tracing::info!(bin=%self.config.bin, "using zstep binary");
            cmd = Command::new(&program);
            for a in &prefix_args {
                cmd.arg(a);
            }
            if let Some(ref indices) = ctx.gpu_indices {
                let devs: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                cmd.env("CUDA_VISIBLE_DEVICES", devs.join(","));
            }
            if let Some(ref seed) = kv_hash_seed {
                cmd.env("NEBULA_KV_HASH_SEED", seed);
            }
            cmd.current_dir(&self.config.cwd);
            cmd.arg("--model-dir")
                .arg(&model_tag)
                .arg("--host")
                .arg(&self.config.host)
                .arg("--port")
                .arg(selected_port.to_string());

            for a in &zs_args {
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
                anyhow::bail!("zstep exited early: {status}");
            }
        }?;

        let process = if process_kind == "docker" {
            let cname = container_name(&ctx.model_uid, ctx.replica_id);
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

        let output = Command::new("docker")
            .args([
                "inspect",
                "-f",
                "{{.State.Running}} {{range .NetworkSettings.Ports}}{{(index . 0).HostPort}}{{end}}",
                &name,
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
            tracing::info!(%name, "container exists but not running, will recreate");
            return None;
        }

        let port_str = stdout.strip_prefix("true ")?.trim();
        let port: u16 = port_str.parse().ok()?;
        let base_url = format!("http://127.0.0.1:{}", port);

        let http = nebula_common::health_http_client().ok()?;

        // Both `/health` AND `/v1/models` must return 2xx: during `Draining`,
        // zstep keeps `/health == 200` (so heartbeat continues scraping pending_requests)
        // while `/v1/models` returns `503` specifically to prevent `try_reuse` from
        // attaching to an exiting container.
        let health_url = format!("{}/health", base_url);
        let models_url = format!("{}/v1/models", base_url);
        let health_ok = http
            .get(&health_url)
            .send()
            .await
            .is_ok_and(|resp| resp.status().is_success());
        if !health_ok {
            tracing::info!(%name, "container running but /health not ready, will recreate");
            return None;
        }
        let engine_model = match http.get(&models_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let v: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
                v.get("data")
                    .and_then(|d| d.get(0))
                    .and_then(|m| m.get("id"))
                    .and_then(|id| id.as_str())
                    .unwrap_or_default()
                    .to_string()
            }
            _ => {
                tracing::info!(%name, "container draining or /v1/models unavailable, will recreate");
                return None;
            }
        };
        if engine_model.is_empty() {
            return None;
        }

        tracing::info!(%name, %base_url, "reusing existing healthy zstep container");
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
                tracing::info!(%name, "stopping zstep docker container");
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
        scrape_zstep_stats(http, &handle.base_url, model_uid, replica_id).await
    }

    async fn try_restart(
        &self,
        handle: &mut EngineHandle,
        ctx: &EngineStartContext,
    ) -> anyhow::Result<()> {
        match &handle.process {
            EngineProcess::DockerContainer { name, .. } => {
                if super::container::docker_container_running(name).await {
                    tracing::warn!(%name, "attempting zstep docker restart");
                    let status = Command::new("docker")
                        .args(["restart", "-t", "10", name])
                        .status()
                        .await?;
                    if !status.success() {
                        anyhow::bail!("docker restart failed for {name}");
                    }
                    Ok(())
                } else {
                    tracing::warn!(
                        %name,
                        model_uid=%ctx.model_uid,
                        replica_id=ctx.replica_id,
                        "container missing or stopped; recreating via stop+start"
                    );
                    self.stop(handle).await?;
                    *handle = self.start(ctx.clone()).await?;
                    Ok(())
                }
            }
            EngineProcess::Child(_) => {
                tracing::warn!(
                    model_uid=%ctx.model_uid,
                    replica_id=ctx.replica_id,
                    "restarting local zstep via stop+start (process group)"
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

pub use nebula_common::engine_metrics::parse_zstep_metrics_text;

pub async fn scrape_zstep_stats(
    http: &reqwest::Client,
    base_url: &str,
    model_uid: &str,
    replica_id: u32,
) -> super::ScrapeResult {
    let url = format!("{}/metrics", base_url.trim_end_matches('/'));
    let text = match http.get(&url).send().await {
        Ok(resp) => {
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                tracing::debug!(%base_url, "zstep /metrics not exposed");
                return Err(super::ScrapeError::NotAvailable);
            }
            if !resp.status().is_success() {
                tracing::debug!(status=%resp.status(), %base_url, "zstep metrics HTTP error");
                return Err(super::ScrapeError::Unreachable);
            }
            match resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    tracing::debug!(error=%e, %base_url, "failed to read zstep metrics body");
                    return Err(super::ScrapeError::ParseFailed);
                }
            }
        }
        Err(e) => {
            tracing::debug!(error=%e, %base_url, "failed to scrape zstep metrics");
            if e.is_timeout() {
                return Err(super::ScrapeError::Timeout);
            }
            return Err(super::ScrapeError::Unreachable);
        }
    };

    let ts_ms = now_ms();
    let stats = parse_zstep_metrics_text(&text, model_uid, replica_id, ts_ms);
    let diag = nebula_common::parse_engine_scrape_diagnostics(Some("zstep"), &text);
    if let Some(tau) = diag.spec_decode_mean_acceptance_length {
        tracing::debug!(
            model_uid = %model_uid,
            replica_id,
            spec_decode_mean_acceptance_length = tau,
            "scraped speculative decoding acceptance length"
        );
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_launch_args_pins_single_gpu_world_size_and_served_model() {
        let cfg = ZstepConfig {
            bin: "zs-server serve".into(),
            cwd: ".".into(),
            host: "0.0.0.0".into(),
            docker_image: None,
            model_dir: "/DATA/Model".into(),
            tensor_parallel_size: None,
            gpu_memory_utilization: Some(0.9),
            max_num_seqs: Some(64),
            worker_root: Some("/model/zstep/worker".into()),
        };
        let ctx = EngineStartContext {
            model_uid: "qwen3.5-35b".into(),
            model_name: "/models/Qwen3.5-35B-A3B".into(),
            replica_id: 0,
            port: 18080,
            gpu_indices: Some(vec![3]),
            engine_config_path: "/nonexistent.yaml".into(),
            extra_args: None,
            ready_timeout: Duration::from_secs(60),
            role: nebula_common::ReplicaRole::Mixed,
            kv_sharing: None,
        };
        let args = build_launch_args(&cfg, &ctx, None, None);
        assert!(args
            .windows(2)
            .any(|w| w == ["--served-model-name", "qwen3.5-35b"]));
        assert!(args.windows(2).any(|w| w == ["--world-size", "1"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["--gpu-memory-utilization", "0.9"]));
        assert!(args.windows(2).any(|w| w == ["--max-seqs", "64"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["--worker-root", "/model/zstep/worker"]));
        assert!(!args.iter().any(|a| a == "--enable-metrics"));
    }

    #[test]
    fn build_launch_args_injects_kv_events_endpoint_and_topic() {
        let cfg = ZstepConfig {
            bin: "zs-server serve".into(),
            cwd: ".".into(),
            host: "0.0.0.0".into(),
            docker_image: None,
            model_dir: "/DATA/Model".into(),
            tensor_parallel_size: None,
            gpu_memory_utilization: None,
            max_num_seqs: None,
            worker_root: None,
        };
        let ctx = EngineStartContext {
            model_uid: "dsv41".into(),
            model_name: "/models/DeepSeek-V4.1".into(),
            replica_id: 2,
            port: 18080,
            gpu_indices: Some(vec![0, 1]),
            engine_config_path: "/nonexistent.yaml".into(),
            extra_args: None,
            ready_timeout: Duration::from_secs(60),
            role: nebula_common::ReplicaRole::Mixed,
            kv_sharing: None,
        };
        let args =
            build_launch_args_with_kv_events(&cfg, &ctx, None, None, Some("tcp://127.0.0.1:5557"));
        assert!(args
            .windows(2)
            .any(|w| w == ["--kv-events-endpoint", "tcp://127.0.0.1:5557"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["--kv-events-topic", "dsv41/2"]));
    }

    #[test]
    fn zstep_worker_root_cli_flag_parses_cleanly() {
        use clap::Parser;
        let parsed = crate::args::Args::try_parse_from([
            "nebula-node",
            "--zstep-worker-root",
            "/opt/worker",
        ])
        .unwrap();
        assert_eq!(parsed.zstep_worker_root.as_deref(), Some("/opt/worker"));
    }
}
