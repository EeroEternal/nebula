use clap::Parser;

#[derive(Debug, Parser)]
pub struct Args {
    #[arg(long, default_value = "node_gpu0")]
    pub default_node_id: String,

    #[arg(long, default_value_t = 10814)]
    pub default_port: u16,

    /// Address for the metrics / health HTTP server.
    #[arg(long, default_value = "0.0.0.0:18082")]
    pub listen_addr: String,

    #[command(flatten)]
    pub common: nebula_common::CommonArgs,
}
