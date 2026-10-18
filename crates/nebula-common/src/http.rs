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

/// Inference proxy client (Gateway / Router → engine, streaming).
///
/// Connect: 3s; request: **no total timeout**. `reqwest`'s `timeout()` covers the
/// whole response body, so a flat cap truncates any generation longer than it
/// (long CoT, agent turns, slow prefill + long decode). Per-phase deadlines are
/// enforced by the stream loop instead: `first byte` and `inter-chunk idle` via
/// [`stream_idle_timeout`], which bound a hung backend without capping a healthy
/// long stream.
pub fn proxy_http_client() -> Result<Client, reqwest::Error> {
    build_http_client(HttpClientOptions {
        connect_timeout: Duration::from_secs(3),
        request_timeout: None,
    })
}

/// Default `NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS`: max silence between two chunks
/// of a streamed response (and max wait for a non-streamed body).
pub const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 120_000;

/// Inter-chunk idle budget for streamed proxying (`NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS`).
///
/// Long enough for a engine-side reasoning pause or a slow prefill chunk, short
/// enough that a silently hung backend is reported instead of held open forever.
pub fn stream_idle_timeout() -> Duration {
    Duration::from_millis(
        std::env::var("NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|ms| *ms > 0)
            .unwrap_or(DEFAULT_STREAM_IDLE_TIMEOUT_MS),
    )
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
        proxy_http_client().expect("streaming proxy");
        control_plane_http_client().expect("control");
        health_http_client().expect("health");
        audit_http_client().expect("audit");
    }

    #[test]
    fn stream_idle_timeout_defaults_and_ignores_zero() {
        // Serialised env access is not needed: both cases set then restore.
        std::env::remove_var("NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS");
        assert_eq!(
            stream_idle_timeout(),
            Duration::from_millis(DEFAULT_STREAM_IDLE_TIMEOUT_MS)
        );
        std::env::set_var("NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS", "0");
        assert_eq!(
            stream_idle_timeout(),
            Duration::from_millis(DEFAULT_STREAM_IDLE_TIMEOUT_MS),
            "0 must not disable the idle budget (that would restore the hang)"
        );
        std::env::set_var("NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS", "5000");
        assert_eq!(stream_idle_timeout(), Duration::from_millis(5000));
        std::env::remove_var("NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS");
    }
}
