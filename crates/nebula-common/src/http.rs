//! Shared HTTP client construction for Nebula control-plane and proxy paths.
//!
//! Prefer these helpers over ad-hoc `reqwest::Client::builder()` so connect/request
//! timeouts stay consistent across Gateway, Router, BFF, and Node.

use std::time::Duration;

use reqwest::Client;

/// Options for [`build_http_client`].
#[derive(Debug, Clone)]
pub struct HttpClientOptions {
    pub connect_timeout: Duration,
    /// Overall request timeout. `None` disables the reqwest timeout (streaming proxies
    /// that set their own deadlines may still pass `Some`).
    pub request_timeout: Option<Duration>,
}

impl Default for HttpClientOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(3),
            request_timeout: Some(Duration::from_secs(30)),
        }
    }
}

/// Build a `reqwest::Client` with the given timeouts.
pub fn build_http_client(opts: HttpClientOptions) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder().connect_timeout(opts.connect_timeout);
    if let Some(timeout) = opts.request_timeout {
        builder = builder.timeout(timeout);
    }
    builder.build()
}

/// Long-lived proxy client (Gateway / Router → engine HTTP).
///
/// Connect: 3s; request: 300s (covers slow LLM generations / SSE).
pub fn proxy_http_client() -> Result<Client, reqwest::Error> {
    build_http_client(HttpClientOptions {
        connect_timeout: Duration::from_secs(3),
        request_timeout: Some(Duration::from_secs(300)),
    })
}

/// Control-plane client (BFF → router / external APIs).
///
/// Connect: 3s; request: 30s.
pub fn control_plane_http_client() -> Result<Client, reqwest::Error> {
    build_http_client(HttpClientOptions {
        connect_timeout: Duration::from_secs(3),
        request_timeout: Some(Duration::from_secs(30)),
    })
}

/// Short-timeout client for health checks and engine readiness probes.
///
/// Connect: 3s; request: 5s.
pub fn health_http_client() -> Result<Client, reqwest::Error> {
    build_http_client(HttpClientOptions {
        connect_timeout: Duration::from_secs(3),
        request_timeout: Some(Duration::from_secs(5)),
    })
}

/// Fire-and-forget style client (audit / metrics push).
///
/// Connect: 3s; request: 10s.
pub fn audit_http_client() -> Result<Client, reqwest::Error> {
    build_http_client(HttpClientOptions {
        connect_timeout: Duration::from_secs(3),
        request_timeout: Some(Duration::from_secs(10)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_default_client() {
        let client = build_http_client(HttpClientOptions::default()).expect("client");
        // Client is clone-cheap; just ensure construction succeeds.
        let _ = client.clone();
    }

    #[test]
    fn presets_build() {
        proxy_http_client().expect("proxy");
        control_plane_http_client().expect("control");
        health_http_client().expect("health");
        audit_http_client().expect("audit");
    }
}
