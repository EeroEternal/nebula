use clap::Parser;

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(long, default_value = "0.0.0.0:18081")]
    pub listen_addr: String,

    #[arg(long, default_value = "qwen2_5_0_5b")]
    pub model_uid: String,

    #[arg(long, default_value = "least_pending")]
    pub routing_strategy: String,

    #[command(flatten)]
    pub common: nebula_common::CommonArgs,
}
