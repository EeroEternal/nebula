use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;

use super::{
    configure_process_group, container_name, find_available_port, kill_process_group,
    parse_yaml_defaults, stop_docker_container_by_name, wait_engine_ready, Engine, EngineHandle,
    EngineProcess, EngineStartContext,
};
use crate::util::now_ms;

/// TokenSpeed-specific configuration extracted from Node CLI args.
#[derive(Debug, Clone)]
pub struct TokenspeedConfig {
    pub bin: String,
    pub cwd: String,
    pub host: String,
    pub docker_image: Option<String>,
    pub model_dir: String,
    pub tensor_parallel_size: Option<u32>,
    pub data_parallel_size: Option<u32>,
    pub gpu_memory_utilization: Option<f32>,
    pub max_num_seqs: Option<u32>,
}

pub struct TokenspeedEngine {
    pub config: TokenspeedConfig,
}

impl TokenspeedEngine {
    pub fn new(args: &crate::args::Args) -> Self {
        Self {
            config: TokenspeedConfig {
                bin: args.tokenspeed_bin.clone(),
                cwd: args.tokenspeed_cwd.clone(),
                host: args.tokenspeed_host.clone(),
                docker_image: args.tokenspeed_docker_image.clone(),
                model_dir: args.tokenspeed_model_dir.clone(),
                tensor_parallel_size: args.tokenspeed_tensor_parallel_size,
                data_parallel_size: args.tokenspeed_data_parallel_size,
                gpu_memory_utilization: args.tokenspeed_gpu_memory_utilization,
                max_num_seqs: args.tokenspeed_max_num_seqs,
            },
        }
    }

    fn is_docker(&self) -> bool {
        self.config.docker_image.is_some()
    }

    /// Split `--tokenspeed-bin` into executable + prefix args (default: `tokenspeed serve`).
    fn split_bin(&self) -> (String, Vec<String>) {
        let parts: Vec<&str> = self.config.bin.split_whitespace().collect();
        if parts.is_empty() {
            return ("tokenspeed".into(), vec!["serve".into()]);
        }
        let program = parts[0].to_string();
        let prefix = parts[1..].iter().map(|s| (*s).to_string()).collect();
        (program, prefix)
    }
}

#[async_trait]
impl Engine for TokenspeedEngine {
    fn engine_type(&self) -> &str {
        "tokenspeed"
    }

    fn capabilities(&self) -> nebula_common::EngineCapability {
        nebula_common::static_capability_tokenspeed()
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
            "starting tokenspeed engine"
        );

        let mut ts_args: Vec<String> = Vec::new();
        if let Some(extra) = ctx.extra_args.as_ref() {
            ts_args.extend(extra.clone());
        }
        if let Some(name) = served_model_name.as_deref() {
            if !ts_args.iter().any(|a| a == "--served-model-name") {
                ts_args.push("--served-model-name".into());
                ts_args.push(name.into());
            }
        }
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
            if !ts_args
                .iter()
                .any(|a| a == "--tensor-parallel-size" || a == "--tp")
            {
                ts_args.push("--tensor-parallel-size".into());
                ts_args.push(tp.to_string());
            }
        }
        if let Some(dp) = self.config.data_parallel_size {
            if !ts_args
                .iter()
                .any(|a| a == "--data-parallel-size" || a == "--dp")
            {
                ts_args.push("--data-parallel-size".into());
                ts_args.push(dp.to_string());
            }
        }
        let gpu_util = self.config.gpu_memory_utilization.or(cfg_gpu_util);
        if let Some(v) = gpu_util {
            if !ts_args.iter().any(|a| a == "--gpu-memory-utilization") {
                ts_args.push("--gpu-memory-utilization".into());
                ts_args.push(v.to_string());
            }
        }
        if let Some(v) = self.config.max_num_seqs {
            if !ts_args.iter().any(|a| a == "--max-num-seqs") {
                ts_args.push("--max-num-seqs".into());
                ts_args.push(v.to_string());
            }
        }
        if !ts_args.iter().any(|a| a == "--enable-metrics") {
            ts_args.push("--enable-metrics".into());
        }

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

            tracing::info!(image=%image, container=%cname, gpu=%gpu_device, model=%container_model, "launching TokenSpeed via docker");

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

            cmd.arg(image);

            let (program, prefix_args) = self.split_bin();
            cmd.arg(&program);
            for a in &prefix_args {
                cmd.arg(a);
            }
            cmd.arg("--model")
                .arg(&container_model)
                .arg("--host")
                .arg("0.0.0.0")
                .arg("--port")
                .arg(selected_port.to_string());

            for a in &ts_args {
                cmd.arg(a);
            }

            process_kind = "docker";
        } else {
            let (program, prefix_args) = self.split_bin();

            tracing::info!(bin=%self.config.bin, "using tokenspeed binary");
            cmd = Command::new(&program);
            for a in &prefix_args {
                cmd.arg(a);
            }
            if let Some(ref indices) = ctx.gpu_indices {
                let devs: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                cmd.env("CUDA_VISIBLE_DEVICES", devs.join(","));
            }
            cmd.current_dir(&self.config.cwd);
            cmd.arg("--model")
                .arg(&model_tag)
                .arg("--host")
                .arg(&self.config.host)
                .arg("--port")
                .arg(selected_port.to_string());

            for a in &ts_args {
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
                anyhow::bail!("tokenspeed exited early: {status}");
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

        let health_url = format!("{}/health", base_url);
        match http.get(&health_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(%name, %base_url, "reusing existing healthy tokenspeed container");

                let models_url = format!("{}/v1/models", base_url);
                let engine_model = match http.get(&models_url).send().await {
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
            _ => {
                tracing::info!(%name, "container running but not healthy, will recreate");
                None
            }
        }
    }

    async fn stop(&self, handle: &mut EngineHandle) -> anyhow::Result<()> {
        match &mut handle.process {
            EngineProcess::Child(child) => {
                kill_process_group(child, Duration::from_secs(10)).await?;
            }
            EngineProcess::DockerContainer { name, wait_child } => {
                tracing::info!(%name, "stopping tokenspeed docker container");
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
        scrape_tokenspeed_stats(http, &handle.base_url, model_uid, replica_id).await
    }

    async fn try_restart(
        &self,
        handle: &mut EngineHandle,
        ctx: &EngineStartContext,
    ) -> anyhow::Result<()> {
        match &handle.process {
            EngineProcess::DockerContainer { name, .. } => {
                if super::container::docker_container_running(name).await {
                    tracing::warn!(%name, "attempting tokenspeed docker restart");
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
                    "restarting local tokenspeed via stop+start (process group)"
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

pub use nebula_common::engine_metrics::parse_tokenspeed_metrics_text;

pub async fn scrape_tokenspeed_stats(
    http: &reqwest::Client,
    base_url: &str,
    model_uid: &str,
    replica_id: u32,
) -> super::ScrapeResult {
    let url = format!("{}/metrics", base_url.trim_end_matches('/'));
    let text = match http.get(&url).send().await {
        Ok(resp) => {
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                tracing::debug!(%base_url, "tokenspeed /metrics not exposed");
                return Err(super::ScrapeError::NotAvailable);
            }
            if !resp.status().is_success() {
                tracing::debug!(status=%resp.status(), %base_url, "tokenspeed metrics HTTP error");
                return Err(super::ScrapeError::Unreachable);
            }
            match resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    tracing::debug!(error=%e, %base_url, "failed to read tokenspeed metrics body");
                    return Err(super::ScrapeError::ParseFailed);
                }
            }
        }
        Err(e) => {
            tracing::debug!(error=%e, %base_url, "failed to scrape tokenspeed metrics");
            if e.is_timeout() {
                return Err(super::ScrapeError::Timeout);
            }
            return Err(super::ScrapeError::Unreachable);
        }
    };

    Ok(parse_tokenspeed_metrics_text(
        &text,
        model_uid,
        replica_id,
        now_ms(),
    ))
}
