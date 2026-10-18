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

/// Default `NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS`: budget for the engine to produce the
/// first streamed chunk (prefill + first decode step). The idle budget is the wrong
/// deadline for that phase: 120 s of tolerance for a stalled mid-stream chunk is far
/// too generous for "the engine has not started answering at all".
pub const DEFAULT_PROXY_FIRST_BYTE_TIMEOUT_MS: u64 = 60_000;

/// First-chunk budget for streamed proxying (`NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS`).
///
/// `0` opts out (`None`): the idle budget then covers the first chunk too. That opt-out
/// matters — a legitimately slow prefill (very long prompt on slow hardware) must be
/// configurable, which is the same mistake the flat 300 s cap made. Unlike the idle
/// budget, this one *can* be disabled, because the idle deadline still bounds the wait.
pub fn first_byte_timeout() -> Option<Duration> {
    match std::env::var("NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        Some(0) => None,
        Some(ms) => Some(Duration::from_millis(ms)),
        None => Some(Duration::from_millis(DEFAULT_PROXY_FIRST_BYTE_TIMEOUT_MS)),
    }
}

/// Default `NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS`: total wall-clock budget a single
/// client request may spend waiting for a first byte across *all* replica attempts.
///
/// Per-attempt retries without a shared budget multiply latency by the attempt count,
/// so the two knobs have to be read together: per attempt `first_byte_timeout()`,
/// across attempts this budget.
pub const DEFAULT_FIRST_BYTE_TOTAL_BUDGET_MS: u64 = 90_000;

/// Total first-byte budget across retries (`NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS`).
pub fn first_byte_total_budget() -> Duration {
    Duration::from_millis(
        std::env::var("NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|ms| *ms > 0)
            .unwrap_or(DEFAULT_FIRST_BYTE_TOTAL_BUDGET_MS),
    )
}

/// Per-chunk deadline for a streamed response: the first-byte budget for the first
/// chunk, then the idle budget for every later one.
///
/// Exists so the deadline choice is one testable unit instead of an `if first_chunk`
/// buried in each proxy loop (there are two).
#[derive(Debug, Clone, Copy)]
pub struct ChunkDeadline {
    first: Duration,
    idle: Duration,
    seen_first: bool,
}

impl ChunkDeadline {
    pub fn new(first: Option<Duration>, idle: Duration) -> Self {
        Self {
            first: first.unwrap_or(idle),
            idle,
            seen_first: false,
        }
    }

    /// Deadline for the next chunk plus whether that chunk is the first one.
    /// Call exactly once per await; it advances the state machine.
    pub fn next_deadline(&mut self) -> (Duration, bool) {
        if self.seen_first {
            (self.idle, false)
        } else {
            self.seen_first = true;
            (self.first, true)
        }
    }
}

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

    #[tokio::test(start_paused = true)]
    async fn first_chunk_uses_the_first_byte_budget_then_idle() {
        // 500 ms first-byte vs 120 s idle: a stalled engine must be reported after the
        // short budget, and only later chunks get the generous one.
        let mut dl = ChunkDeadline::new(Some(Duration::from_millis(500)), Duration::from_secs(120));

        let t0 = tokio::time::Instant::now();
        let (d1, first1) = dl.next_deadline();
        assert!(first1);
        assert_eq!(d1, Duration::from_millis(500));
        assert!(
            tokio::time::timeout(d1, std::future::pending::<()>())
                .await
                .is_err(),
            "first chunk must hit its deadline"
        );
        assert!(
            tokio::time::Instant::now() >= t0 + Duration::from_millis(500),
            "the first-chunk deadline, not the idle budget, must have elapsed"
        );

        // The first chunk arrived: the next deadline is the idle budget.
        let (d2, first2) = dl.next_deadline();
        assert!(!first2);
        assert_eq!(d2, Duration::from_secs(120));
        let before = tokio::time::Instant::now();
        assert!(tokio::time::timeout(d2, std::future::pending::<()>())
            .await
            .is_err());
        assert!(tokio::time::Instant::now() >= before + Duration::from_secs(120));
    }

    #[tokio::test(start_paused = true)]
    async fn without_first_byte_budget_the_idle_budget_applies_from_the_start() {
        let mut dl = ChunkDeadline::new(None, Duration::from_secs(30));
        let (d, first) = dl.next_deadline();
        assert!(first);
        assert_eq!(d, Duration::from_secs(30));
    }

    #[test]
    fn first_byte_total_budget_default_and_override() {
        std::env::remove_var("NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS");
        assert_eq!(
            first_byte_total_budget(),
            Duration::from_millis(DEFAULT_FIRST_BYTE_TOTAL_BUDGET_MS)
        );
        std::env::set_var("NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS", "0");
        assert_eq!(
            first_byte_total_budget(),
            Duration::from_millis(DEFAULT_FIRST_BYTE_TOTAL_BUDGET_MS),
            "0 must not remove the cap: unbounded retry latency is the bug, not the fix"
        );
        std::env::set_var("NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS", "5000");
        assert_eq!(first_byte_total_budget(), Duration::from_millis(5000));
        std::env::remove_var("NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS");
    }

    #[test]
    fn first_byte_timeout_default_and_opt_out() {
        std::env::remove_var("NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS");
        assert_eq!(
            first_byte_timeout(),
            Some(Duration::from_millis(DEFAULT_PROXY_FIRST_BYTE_TIMEOUT_MS))
        );
        std::env::set_var("NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS", "0");
        assert_eq!(
            first_byte_timeout(),
            None,
            "0 is the documented opt-out (slow prefill must be configurable)"
        );
        std::env::set_var("NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS", "5000");
        assert_eq!(first_byte_timeout(), Some(Duration::from_millis(5000)));
        std::env::remove_var("NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS");
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
