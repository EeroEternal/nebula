use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "lower")]
pub enum XtraceAuthMode {
    /// Use service-to-service bearer token (OBSERVE_TOKEN).
    Service,
    /// Trust internal network, do not send auth header to xtrace.
    Internal,
}

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Args {
    #[arg(long, env = "NEBULA_BFF_ADDR", default_value = "0.0.0.0:18090")]
    pub listen_addr: String,

    #[arg(
        long,
        env = "NEBULA_ROUTER_URL",
        default_value = "http://127.0.0.1:18081"
    )]
    pub router_url: String,

    /// PostgreSQL connection URL for user auth/profile persistence.
    #[arg(
        long,
        env = "BFF_DATABASE_URL",
        default_value = "postgresql://postgres:postgres@127.0.0.1:5432/nebula"
    )]
    pub database_url: String,

    /// Session TTL in hours for login tokens.
    #[arg(long, env = "BFF_SESSION_TTL_HOURS", default_value_t = 24)]
    pub session_ttl_hours: i64,

    /// xtrace auth mode: service (token) or internal (no token).
    #[arg(long, env = "OBSERVE_AUTH_MODE", value_enum, default_value_t = XtraceAuthMode::Service)]
    pub xtrace_auth_mode: XtraceAuthMode,

    #[command(flatten)]
    pub common: nebula_common::CommonArgs,
}
