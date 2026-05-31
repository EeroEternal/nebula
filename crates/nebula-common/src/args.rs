use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct CommonArgs {
    /// etcd endpoint for cluster coordination
    #[arg(long = "etcd-endpoint", env = "ETCD_ENDPOINT", default_value = "http://127.0.0.1:2379")]
    pub etcd_endpoint: String,

    /// OTLP/xtrace endpoint for exporting traces/metrics (e.g. "http://127.0.0.1:8742").
    #[arg(long = "xtrace-url", env = "OBSERVE_URL")]
    pub xtrace_url: Option<String>,

    /// Bearer token for xtrace authentication.
    #[arg(long = "xtrace-token", env = "OBSERVE_TOKEN")]
    pub xtrace_token: Option<String>,

    /// Log output format: "text" (human-readable) or "json" (structured).
    #[arg(long = "log-format", env = "NEBULA_LOG_FORMAT", default_value = "text")]
    pub log_format: String,
}
