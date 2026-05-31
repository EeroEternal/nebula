use clap::Parser;

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(long, default_value = "node_1")]
    pub node_id: String,

    #[arg(long, default_value = "/home/ai/miniconda3/envs/Lvllm/bin/vllm")]
    pub vllm_bin: String,

    #[arg(long, default_value = "Lvllm/m21.yaml")]
    pub vllm_config: String,

    #[arg(long, default_value = "/home/ai")]
    pub vllm_cwd: String,

    #[arg(long, default_value_t = 10814)]
    pub vllm_port: u16,

    #[arg(long, default_value = "0.0.0.0")]
    pub vllm_host: String,

    #[arg(long, default_value = "/tmp/nebula/engine.env")]
    pub engine_env_path: String,

    #[arg(long, default_value_t = 60_000)]
    pub heartbeat_ttl_ms: u64,

    #[arg(long, default_value_t = 3_000)]
    pub heartbeat_interval_ms: u64,

    #[arg(long, default_value_t = 180)]
    pub ready_timeout_secs: u64,

    #[arg(long)]
    pub vllm_gpu_memory_utilization: Option<f32>,

    #[arg(long)]
    pub vllm_max_model_len: Option<u32>,

    #[arg(long)]
    pub vllm_swap_space: Option<u32>,

    #[arg(long)]
    pub vllm_max_num_batched_tokens: Option<u32>,

    #[arg(long)]
    pub vllm_max_num_seqs: Option<u32>,

    #[arg(long)]
    pub vllm_tensor_parallel_size: Option<u32>,

    /// If set, launch vLLM via `docker run` using this image instead of a local binary.
    #[arg(long)]
    pub vllm_docker_image: Option<String>,

    /// Host path to mount as /model inside the container (e.g. /DATA/Model).
    #[arg(long, default_value = "/DATA/Model")]
    pub vllm_model_dir: String,

    /// Use ModelScope as the model source for vLLM (sets VLLM_USE_MODELSCOPE=True).
    /// Default: false (do not force ModelScope).
    #[arg(long, default_value_t = false)]
    pub vllm_use_modelscope: bool,

    /// Optional HuggingFace endpoint mirror for downloading models (sets HF_ENDPOINT).
    #[arg(long)]
    pub vllm_hf_endpoint: Option<String>,

    // ---- SGLang engine parameters ----
    /// SGLang binary path (used in local binary mode).
    #[arg(long, default_value = "python3 -m sglang.launch_server")]
    pub sglang_bin: String,

    /// Working directory for SGLang local binary mode.
    #[arg(long, default_value = "/home/ai")]
    pub sglang_cwd: String,

    /// Host for SGLang to bind to.
    #[arg(long, default_value = "0.0.0.0")]
    pub sglang_host: String,

    /// If set, launch SGLang via `docker run` using this image.
    #[arg(long)]
    pub sglang_docker_image: Option<String>,

    /// Host path to mount as /model inside the SGLang container.
    #[arg(long, default_value = "/DATA/Model")]
    pub sglang_model_dir: String,

    /// Tensor parallel size for SGLang.
    #[arg(long)]
    pub sglang_tensor_parallel_size: Option<u32>,

    /// Data parallel size for SGLang.
    #[arg(long)]
    pub sglang_data_parallel_size: Option<u32>,

    /// Memory fraction for SGLang (equivalent to --mem-fraction-static).
    #[arg(long)]
    pub sglang_mem_fraction: Option<f32>,

    /// Max number of running requests for SGLang.
    #[arg(long)]
    pub sglang_max_running_requests: Option<u32>,

    /// Port for the Node HTTP API (containers, images, etc.).
    #[arg(long, default_value_t = 9090)]
    pub api_port: u16,

    #[command(flatten)]
    pub common: nebula_common::CommonArgs,
}
