use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Args {
    #[arg(long, env = "NEBULA_GATEWAY_ADDR", default_value = "0.0.0.0:8081")]
    pub listen_addr: String,

    #[arg(long, env = "NEBULA_ROUTER_URL", default_value = "http://127.0.0.1:18081")]
    pub router_url: String,

    #[arg(long, env = "NEBULA_GATEWAY_LOG_PATH", default_value = "/tmp/nebula-gateway.log")]
    pub log_path: String,

    #[arg(long, env = "NEBULA_ENGINE_MODEL")]
    pub engine_model: Option<String>,

    /// BFF service URL for v2 API proxy.
    #[arg(long, env = "NEBULA_BFF_URL", default_value = "http://127.0.0.1:18090")]
    pub bff_url: String,

    #[command(flatten)]
    pub common: nebula_common::CommonArgs,
}
