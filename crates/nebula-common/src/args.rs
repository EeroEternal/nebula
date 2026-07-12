use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct CommonArgs {
    /// etcd endpoint(s) for cluster coordination.
    /// Comma-separated list is supported for HA (e.g. `http://e1:2379,http://e2:2379,http://e3:2379`).
    #[arg(
        long = "etcd-endpoint",
        env = "ETCD_ENDPOINT",
        default_value = "http://127.0.0.1:2379"
    )]
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

impl CommonArgs {
    /// Parse `ETCD_ENDPOINT` into one or more client URLs.
    pub fn etcd_endpoints(&self) -> Vec<String> {
        parse_etcd_endpoints(&self.etcd_endpoint)
    }
}

/// Split a comma-separated etcd endpoint list.
pub fn parse_etcd_endpoints(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_and_multi_endpoints() {
        assert_eq!(
            parse_etcd_endpoints("http://127.0.0.1:2379"),
            vec!["http://127.0.0.1:2379".to_string()]
        );
        assert_eq!(
            parse_etcd_endpoints("http://e1:2379, http://e2:2379 ,http://e3:2379"),
            vec![
                "http://e1:2379".to_string(),
                "http://e2:2379".to_string(),
                "http://e3:2379".to_string()
            ]
        );
        assert!(parse_etcd_endpoints(" , , ").is_empty());
    }
}
