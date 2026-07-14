use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EngineKind {
    Vllm,
    Sglang,
}

#[derive(Debug, Parser)]
#[command(name = "nebula-lite")]
#[command(about = "Single-process local inference: spawn vLLM/SGLang and proxy OpenAI API")]
pub struct Args {
    /// Model id or local path
    #[arg(long)]
    pub model: String,

    /// Inference engine
    #[arg(long, value_enum, default_value_t = EngineKind::Vllm)]
    pub engine: EngineKind,

    /// GPU indices, e.g. 0 or 0,1,2,3 (sets CUDA_VISIBLE_DEVICES and TP)
    #[arg(long, default_value = "0")]
    pub gpus: String,

    /// Public listen host
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Public listen port (OpenAI-compatible API)
    #[arg(long, default_value_t = 8081)]
    pub port: u16,

    /// Internal engine listen port (0 = auto-pick near public port)
    #[arg(long, default_value_t = 0)]
    pub engine_port: u16,

    /// vLLM binary (used when --engine=vllm)
    #[arg(long, default_value = "vllm", env = "NEBULA_LITE_VLLM_BIN")]
    pub vllm_bin: String,

    /// SGLang launch command (used when --engine=sglang); may be multi-word
    #[arg(
        long,
        default_value = "python3 -m sglang.launch_server",
        env = "NEBULA_LITE_SGLANG_BIN"
    )]
    pub sglang_bin: String,

    /// vLLM --max-model-len
    #[arg(long)]
    pub max_model_len: Option<u32>,

    /// vLLM --gpu-memory-utilization
    #[arg(long)]
    pub gpu_memory_utilization: Option<f32>,

    /// SGLang --mem-fraction-static
    #[arg(long)]
    pub mem_fraction_static: Option<f32>,

    /// Seconds to wait for engine /v1/models readiness
    #[arg(long, default_value_t = 1200)]
    pub ready_timeout_secs: u64,

    /// Working directory for the engine process
    #[arg(long, default_value = ".")]
    pub cwd: String,
}

impl Args {
    pub fn gpu_indices(&self) -> anyhow::Result<Vec<u32>> {
        let mut out = Vec::new();
        for part in self.gpus.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let idx: u32 = part
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid --gpus entry: {part}"))?;
            out.push(idx);
        }
        if out.is_empty() {
            anyhow::bail!("--gpus must list at least one GPU index");
        }
        Ok(out)
    }

    pub fn cuda_visible_devices(&self) -> anyhow::Result<String> {
        Ok(self
            .gpu_indices()?
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(","))
    }

    pub fn tensor_parallel_size(&self) -> anyhow::Result<u32> {
        Ok(self.gpu_indices()?.len() as u32)
    }
}
