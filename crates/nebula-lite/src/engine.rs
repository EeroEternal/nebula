use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context};
use tokio::net::TcpListener;
use tokio::process::{Child, Command};

use crate::args::{Args, EngineKind};

pub struct EngineHandle {
    pub base_url: String,
    child: Child,
}

impl EngineHandle {
    pub async fn shutdown(mut self) {
        if let Err(e) = kill_process_group(&mut self.child, Duration::from_secs(10)).await {
            tracing::warn!(error=%e, "failed to stop engine process cleanly");
        }
    }
}

pub async fn start_engine(args: &Args) -> anyhow::Result<EngineHandle> {
    let cuda = args.cuda_visible_devices()?;
    let tp = args.tensor_parallel_size()?;
    let engine_port = if args.engine_port == 0 {
        find_available_port(args.port.saturating_add(10000).max(19000), 64).await?
    } else {
        find_available_port(args.engine_port, 64).await?
    };
    let base_url = format!("http://127.0.0.1:{engine_port}");

    let mut cmd = match args.engine {
        EngineKind::Vllm => build_vllm_command(args, engine_port, tp)?,
        EngineKind::Sglang => build_sglang_command(args, engine_port, tp)?,
    };
    cmd.env("CUDA_VISIBLE_DEVICES", &cuda);
    cmd.current_dir(&args.cwd);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    configure_process_group(&mut cmd);

    let program = command_program_label(args);
    ensure_program_resolvable(&program)?;

    tracing::info!(
        engine=?args.engine,
        model=%args.model,
        gpus=%cuda,
        tp,
        engine_port,
        bin=%program,
        "starting engine"
    );

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn engine: {program}"))?;

    let ready_timeout = Duration::from_secs(args.ready_timeout_secs);
    tokio::select! {
        r = wait_engine_ready(&base_url, ready_timeout) => {
            r.with_context(|| format!("engine at {base_url} did not become ready"))?;
        }
        status = child.wait() => {
            let status = status.context("waiting for engine process")?;
            bail!("engine exited before ready: {status}");
        }
    }

    tracing::info!(%base_url, "engine ready");
    Ok(EngineHandle { base_url, child })
}

fn command_program_label(args: &Args) -> String {
    match args.engine {
        EngineKind::Vllm => args.vllm_bin.clone(),
        EngineKind::Sglang => args.sglang_bin.clone(),
    }
}

fn ensure_program_resolvable(bin: &str) -> anyhow::Result<()> {
    let program = bin.split_whitespace().next().unwrap_or(bin);
    if program.contains('/') {
        if !Path::new(program).exists() {
            bail!("engine binary not found: {program}");
        }
        return Ok(());
    }
    // Best-effort: `which` on Unix; if missing, still try spawn (PATH may work).
    #[cfg(unix)]
    {
        let status = std::process::Command::new("which")
            .arg(program)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            _ => bail!(
                "engine binary not found on PATH: {program}\n\
                 install the engine or pass --vllm-bin / --sglang-bin"
            ),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = program;
        Ok(())
    }
}

fn build_vllm_command(args: &Args, port: u16, tp: u32) -> anyhow::Result<Command> {
    let mut cmd = Command::new(&args.vllm_bin);
    cmd.arg("serve")
        .arg(&args.model)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--tensor-parallel-size")
        .arg(tp.to_string());
    if let Some(v) = args.gpu_memory_utilization {
        cmd.arg("--gpu-memory-utilization").arg(v.to_string());
    }
    if let Some(v) = args.max_model_len {
        cmd.arg("--max-model-len").arg(v.to_string());
    }
    cmd.env("HF_HUB_DISABLE_XET", "1");
    Ok(cmd)
}

fn build_sglang_command(args: &Args, port: u16, tp: u32) -> anyhow::Result<Command> {
    let parts: Vec<&str> = args.sglang_bin.split_whitespace().collect();
    if parts.is_empty() {
        bail!("--sglang-bin is empty");
    }
    let (program, prefix) = (parts[0], &parts[1..]);
    let mut cmd = Command::new(program);
    for a in prefix {
        cmd.arg(a);
    }
    cmd.arg("--model-path")
        .arg(&args.model)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("--tp")
        .arg(tp.to_string());
    if let Some(v) = args.mem_fraction_static {
        cmd.arg("--mem-fraction-static").arg(v.to_string());
    }
    Ok(cmd)
}

#[cfg(unix)]
fn configure_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_cmd: &mut Command) {}

async fn signal_process_group(pid: u32, signal: &str) {
    let arg = format!("-{pid}");
    let _ = Command::new("kill")
        .args([signal, &arg])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;
}

async fn kill_process_group(child: &mut Child, grace: Duration) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            tracing::info!(pid, "sending SIGTERM to engine process group");
            signal_process_group(pid, "-TERM").await;
            tokio::select! {
                _ = child.wait() => return Ok(()),
                _ = tokio::time::sleep(grace) => {}
            }
            tracing::warn!(pid, "engine still alive after grace; SIGKILL");
            signal_process_group(pid, "-KILL").await;
            let _ = child.wait().await;
            return Ok(());
        }
    }
    let _ = child.kill().await;
    Ok(())
}

async fn find_available_port(start_port: u16, max_tries: u16) -> anyhow::Result<u16> {
    let mut port = start_port;
    for _ in 0..max_tries {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                drop(listener);
                return Ok(port);
            }
            Err(_) => {
                port = port.saturating_add(1);
            }
        }
    }
    bail!(
        "no available engine port in range [{}, {}]",
        start_port,
        start_port.saturating_add(max_tries)
    );
}

async fn wait_engine_ready(base_url: &str, timeout: Duration) -> anyhow::Result<()> {
    let http = nebula_common::health_http_client()?;
    let start = tokio::time::Instant::now();
    let health_url = format!("{}/health", base_url.trim_end_matches('/'));
    let models_url = format!("{}/v1/models", base_url.trim_end_matches('/'));

    loop {
        if start.elapsed() > timeout {
            bail!("engine not ready within {timeout:?}");
        }

        if let Ok(resp) = http.get(&health_url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if let Ok(resp) = http.get(&models_url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
