use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "nebula")]
#[command(about = "Nebula CLI for cluster management", long_about = None)]
pub struct Args {
    /// Gateway URL
    #[arg(
        long,
        env = "NEBULA_GATEWAY_URL",
        default_value = "http://127.0.0.1:8081"
    )]
    pub gateway_url: String,

    /// Gateway API token (Authorization: Bearer)
    #[arg(long, env = "NEBULA_GATEWAY_TOKEN")]
    pub token: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Cluster management
    Cluster {
        #[command(subcommand)]
        subcommand: ClusterCommand,
    },
    /// Model management
    Model {
        #[command(subcommand)]
        subcommand: ModelCommand,
    },
    /// Template management
    Template {
        #[command(subcommand)]
        subcommand: TemplateCommand,
    },
    /// Cache management
    Cache {
        #[command(subcommand)]
        subcommand: CacheCommand,
    },
    /// Disk management
    Disk {
        #[command(subcommand)]
        subcommand: DiskCommand,
    },
    /// Show current auth identity
    Whoami,
    /// Fetch gateway metrics
    Metrics,
    /// Tail gateway logs
    Logs {
        /// Lines to return (default 200, max 2000)
        #[arg(long)]
        lines: Option<u32>,
        /// Follow log output (stream new lines via SSE)
        #[arg(long, short = 'f', default_value_t = false)]
        follow: bool,
    },
    /// Interactive chat via Gateway
    Chat {
        /// Model name to use
        #[arg(long, short = 'm')]
        model: Option<String>,
        /// System prompt
        #[arg(long)]
        system: Option<String>,
        /// Single-shot message (non-interactive)
        #[arg(long)]
        message: Option<String>,
        /// Max tokens for response
        #[arg(long, default_value_t = 2048)]
        max_tokens: u32,
    },
    /// Scale model replicas (legacy v1)
    Scale {
        /// Model request ID
        #[arg(long)]
        id: String,
        /// New replica count
        #[arg(long)]
        replicas: u32,
    },
    /// Drain an endpoint (graceful shutdown)
    Drain {
        /// model_uid of the endpoint
        #[arg(long)]
        model_uid: String,
        /// replica_id of the endpoint
        #[arg(long)]
        replica_id: u32,
    },
    /// Admin operations
    Admin {
        #[command(subcommand)]
        subcommand: AdminCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ClusterCommand {
    /// Show cluster status
    Status,
}

#[derive(Debug, Subcommand)]
pub enum ModelCommand {
    /// List all models with aggregated state (v2 API)
    List,
    /// Get model detail
    Get {
        /// Model UID
        model_uid: String,
    },
    /// Create a new model
    Create {
        /// Model name (e.g. "Qwen/Qwen2.5-7B-Instruct")
        #[arg(long)]
        name: String,
        /// Model UID (auto-generated if not provided)
        #[arg(long)]
        uid: Option<String>,
        /// Engine type (default: vllm)
        #[arg(long, default_value = "vllm")]
        engine: String,
        /// Model source: huggingface, modelscope, local
        #[arg(long, default_value = "huggingface")]
        source: String,
        /// Start the model immediately after creation
        #[arg(long)]
        start: bool,
        /// Number of replicas (when --start is used)
        #[arg(long, default_value_t = 1)]
        replicas: u32,
    },
    /// Start a stopped model
    Start {
        /// Model UID
        model_uid: String,
        /// Number of replicas
        #[arg(long, default_value_t = 1)]
        replicas: u32,
    },
    /// Stop a running model
    Stop {
        /// Model UID
        model_uid: String,
    },
    /// Delete a model
    Delete {
        /// Model UID
        model_uid: String,
    },
    /// Scale model replicas (v2)
    ScaleModel {
        /// Model UID
        model_uid: String,
        /// Desired replica count
        #[arg(long)]
        replicas: u32,
    },
    /// Load a new model (legacy v1 API)
    Load {
        /// User-facing model name
        #[arg(long)]
        name: String,
        /// Internal model UID
        #[arg(long)]
        uid: String,
        /// Number of replicas
        #[arg(long, default_value_t = 1)]
        replicas: u32,

        /// Required VRAM in MB (capacity check)
        #[arg(long)]
        required_vram_mb: Option<u64>,

        /// vLLM tensor parallel size
        #[arg(long)]
        tensor_parallel_size: Option<u32>,

        /// vLLM GPU memory utilization (0-1)
        #[arg(long)]
        gpu_memory_utilization: Option<f32>,

        /// vLLM max model length
        #[arg(long)]
        max_model_len: Option<u32>,

        /// LoRA modules (repeatable)
        #[arg(long, value_delimiter = ',')]
        lora: Option<Vec<String>>,
    },
    /// Unload a model by request ID (legacy v1 API)
    Unload {
        /// Request ID to unload
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum TemplateCommand {
    /// List all templates
    List,
    /// Create a new template
    Create {
        /// Template name
        #[arg(long)]
        name: String,
        /// Model name (e.g. "Qwen/Qwen2.5-7B-Instruct")
        #[arg(long)]
        model_name: String,
        /// Engine type (default: vllm)
        #[arg(long, default_value = "vllm")]
        engine: String,
        /// Model source: huggingface, modelscope, local
        #[arg(long, default_value = "huggingface")]
        source: String,
    },
    /// Deploy a template as a running model
    Deploy {
        /// Template ID
        template_id: String,
        /// Model UID (auto-generated if not provided)
        #[arg(long)]
        uid: Option<String>,
        /// Number of replicas
        #[arg(long, default_value_t = 1)]
        replicas: u32,
    },
    /// Save a running model as a template
    Save {
        /// Model UID to save
        model_uid: String,
        /// Template name
        #[arg(long)]
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum CacheCommand {
    /// List cached models
    List {
        /// Filter by node ID
        #[arg(long)]
        node: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum DiskCommand {
    /// Show disk status
    Status,
}

#[derive(Debug, Subcommand)]
pub enum AdminCommand {
    /// Migrate v1 model_requests to v2 ModelSpec + ModelDeployment
    Migrate,
}
