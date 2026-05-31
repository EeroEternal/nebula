use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;

use nebula_common::EndpointStats;

use super::{
    container_name, find_available_port, parse_yaml_defaults, stop_docker_container_by_name,
    wait_engine_ready, Engine, EngineHandle, EngineProcess, EngineStartContext,
};
use crate::util::now_ms;

/// SGLang-specific configuration extracted from Node CLI args.
#[derive(Debug, Clone)]
pub struct SglangConfig {
    pub bin: String,
    pub cwd: String,
    pub host: String,
    pub docker_image: Option<String>,
    pub model_dir: String,
    pub tensor_parallel_size: Option<u32>,
    pub data_parallel_size: Option<u32>,
    pub mem_fraction: Option<f32>,
    pub max_running_requests: Option<u32>,
}

pub struct SglangEngine {
    pub config: SglangConfig,
}

impl SglangEngine {
    pub fn new(args: &crate::args::Args) -> Self {
        Self {
            config: SglangConfig {
                bin: args.sglang_bin.clone(),
                cwd: args.sglang_cwd.clone(),
                host: args.sglang_host.clone(),
                docker_image: args.sglang_docker_image.clone(),
                model_dir: args.sglang_model_dir.clone(),
                tensor_parallel_size: args.sglang_tensor_parallel_size,
                data_parallel_size: args.sglang_data_parallel_size,
                mem_fraction: args.sglang_mem_fraction,
                max_running_requests: args.sglang_max_running_requests,
            },
        }
    }

    fn is_docker(&self) -> bool {
        self.config.docker_image.is_some()
    }
}

#[async_trait]
impl Engine for SglangEngine {
    fn engine_type(&self) -> &str {
        "sglang"
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
        let cfg_mem_fraction: Option<f32> = cfg
            .get("mem-fraction-static")
            .or_else(|| cfg.get("mem_fraction_static"))
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
            "starting sglang engine"
        );

        // Collect common SGLang args
        let mut sglang_args: Vec<String> = Vec::new();
        if let Some(extra) = ctx.extra_args.as_ref() {
            sglang_args.extend(extra.clone());
        }
        if let Some(name) = served_model_name.as_deref() {
            sglang_args.push("--served-model-name".into());
            sglang_args.push(name.into());
        }
        // Tensor parallel size
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
            sglang_args.push("--tp".into());
            sglang_args.push(tp.to_string());
        }
        if let Some(dp) = self.config.data_parallel_size {
            sglang_args.push("--dp".into());
            sglang_args.push(dp.to_string());
        }
        let mem_fraction = self.config.mem_fraction.or(cfg_mem_fraction);
        if let Some(v) = mem_fraction {
            sglang_args.push("--mem-fraction-static".into());
            sglang_args.push(v.to_string());
        }
        if let Some(v) = self.config.max_running_requests {
            sglang_args.push("--max-running-requests".into());
            sglang_args.push(v.to_string());
        }

        let mut cmd;
        let process_kind;

        if let Some(image) = self.config.docker_image.as_deref() {
            // Docker mode
            let cname = container_name(&ctx.model_uid, ctx.replica_id);
            stop_docker_container_by_name(&cname).await;
            tokio::time::sleep(Duration::from_millis(500)).await;

            let gpu_device = if let Some(ref indices) = ctx.gpu_indices {
                let devs: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                format!("\"device={}\"", devs.join(","))
            } else {
                "all".to_string()
            };

            // Remap model path
            let container_model = if model_tag.starts_with(&self.config.model_dir) {
                model_tag.replacen(&self.config.model_dir, "/model", 1)
            } else {
                model_tag.clone()
            };

            tracing::info!(image=%image, container=%cname, gpu=%gpu_device, model=%container_model, "launching SGLang via docker");

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

            cmd.arg(image);

            cmd.arg("--model-path")
                .arg(&container_model)
                .arg("--host")
                .arg("0.0.0.0")
                .arg("--port")
                .arg(selected_port.to_string());

            for a in &sglang_args {
                cmd.arg(a);
            }

            process_kind = "docker";
        } else {
            // Local binary mode
            // sglang_bin might be "python3 -m sglang.launch_server" (multi-word)
            let parts: Vec<&str> = self.config.bin.split_whitespace().collect();
            let (program, prefix_args) = if parts.len() > 1 {
                (parts[0], &parts[1..])
            } else {
                (parts[0], &[][..])
            };

            tracing::info!(bin=%self.config.bin, "using sglang binary");
            cmd = Command::new(program);
            for a in prefix_args {
                cmd.arg(a);
            }
            if let Some(ref indices) = ctx.gpu_indices {
                let devs: Vec<String> = indices.iter().map(|i| i.to_string()).collect();
                cmd.env("CUDA_VISIBLE_DEVICES", devs.join(","));
            }
            cmd.current_dir(&self.config.cwd);
            cmd.arg("--model-path")
                .arg(&model_tag)
                .arg("--host")
                .arg(&self.config.host)
                .arg("--port")
                .arg(selected_port.to_string());

            for a in &sglang_args {
                cmd.arg(a);
            }

            process_kind = "local";
        }

        let mut child = cmd.spawn()?;

        let ready = tokio::select! {
            r = wait_engine_ready(&base_url, ctx.ready_timeout) => r,
            status = child.wait() => {
                let status = status?;
                anyhow::bail!("sglang exited early: {status}");
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
            .args(["inspect", "-f", "{{.State.Running}} {{range .NetworkSettings.Ports}}{{(index . 0).HostPort}}{{end}}", &name])
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
        let _port: u16 = port_str.parse().ok()?;
        let base_url = format!("http://127.0.0.1:{}", _port);

        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5))
            .build()
            .ok()?;

        let health_url = format!("{}/health", base_url);
        match http.get(&health_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(%name, %base_url, "reusing existing healthy sglang container");

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
                let _ = child.kill().await;
            }
            EngineProcess::DockerContainer { name, wait_child } => {
                tracing::info!(%name, "stopping sglang docker container");
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
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
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
    ) -> Option<EndpointStats> {
        scrape_sglang_stats(http, &handle.base_url, model_uid, replica_id).await
    }

    async fn try_restart(&self, handle: &EngineHandle) {
        if let EngineProcess::DockerContainer { name, .. } = &handle.process {
            tracing::warn!(%name, "attempting sglang docker restart");
            let _ = Command::new("docker")
                .args(["restart", "-t", "10", name])
                .output()
                .await;
        }
    }
}

// ---------------------------------------------------------------------------
// SGLang-specific metrics scraping
// ---------------------------------------------------------------------------

/// Scrape SGLang /metrics endpoint and parse into EndpointStats.
///
/// SGLang exposes Prometheus metrics with prefixes like:
///   sglang:num_requests_waiting{...} 3
///   sglang:num_requests_running{...} 1
///   sglang:token_usage{...} 0.45
///
/// It also exposes /get_model_info for model metadata.
pub async fn scrape_sglang_stats(
    http: &reqwest::Client,
    base_url: &str,
    model_uid: &str,
    replica_id: u32,
) -> Option<EndpointStats> {
    let url = format!("{}/metrics", base_url.trim_end_matches('/'));
    let text = match http.get(&url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                tracing::debug!(error=%e, %base_url, "failed to read sglang metrics body");
                return None;
            }
        },
        Err(e) => {
            tracing::debug!(error=%e, %base_url, "failed to scrape sglang metrics");
            return None;
        }
    };

    let mut pending_requests: u64 = 0;
    let mut running_requests: u64 = 0;
    let mut kv_cache_usage: Option<f64> = None;

    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }

        // SGLang metric formats:
        //   sglang:num_requests_waiting{...} 3
        //   sglang:num_requests_running{...} 1
        //   sglang:token_usage{...} 0.45          (KV cache usage ratio)
        //   sglang_num_requests_waiting{...} 3     (underscore variant)
        if let Some(val) = extract_sglang_metric(line, "num_requests_waiting") {
            pending_requests = val as u64;
        } else if let Some(val) = extract_sglang_metric(line, "num_requests_running") {
            running_requests = val as u64;
        } else if let Some(val) = extract_sglang_metric(line, "token_usage") {
            kv_cache_usage = Some(val);
        } else if kv_cache_usage.is_none() {
            if let Some(val) = extract_sglang_metric(line, "cache_usage") {
                kv_cache_usage = Some(val);
            }
        }
    }

    let (kv_cache_used, kv_cache_free) = match kv_cache_usage {
        Some(pct) => {
            let used = (pct * 1000.0) as u64;
            let free = 1000u64.saturating_sub(used);
            (Some(used), Some(free))
        }
        None => (None, None),
    };

    Some(EndpointStats {
        model_uid: model_uid.to_string(),
        replica_id,
        last_updated_ms: now_ms(),
        pending_requests: pending_requests + running_requests,
        prefix_cache_hit_rate: None,
        prompt_cache_hit_rate: None,
        kv_cache_used_bytes: kv_cache_used,
        kv_cache_free_bytes: kv_cache_free,
    })
}

/// Extract a numeric value from a SGLang Prometheus metric line.
/// Matches lines like:
///   sglang:metric_name{labels...} 123.45
///   sglang_metric_name{labels...} 123.45
fn extract_sglang_metric(line: &str, metric_suffix: &str) -> Option<f64> {
    let has_metric =
        line.contains(&format!(":{metric_suffix}")) || line.contains(&format!("_{metric_suffix}"));

    if !has_metric {
        return None;
    }

    let value_str = line.rsplit_once(|c: char| c.is_whitespace())?.1;
    value_str.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sglang_metric() {
        assert_eq!(
            extract_sglang_metric(
                "sglang:num_requests_waiting{model=\"m\"} 3",
                "num_requests_waiting"
            ),
            Some(3.0)
        );
        assert_eq!(
            extract_sglang_metric("sglang:token_usage{engine=\"0\"} 0.45", "token_usage"),
            Some(0.45)
        );
        assert_eq!(
            extract_sglang_metric("sglang_num_requests_running{} 2", "num_requests_running"),
            Some(2.0)
        );
        assert_eq!(
            extract_sglang_metric("unrelated_metric{} 1.0", "num_requests_waiting"),
            None,
        );
    }
}
