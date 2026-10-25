//! `xlite-transport` implementation of the engine data-plane seam (xlite#96).
//!
//! Built only with `--features xlite`, pinned to a commit rather than a branch. The
//! adapter's whole job is translation; no policy lives here:
//!
//! - **One call = one attempt** on the xlite side too. The adapter deliberately uses
//!   [`Transport::execute_redial_once`], because today's `reqwest`/hyper path already
//!   re-sends a request whose pooled connection the peer closed (hyper does it
//!   internally, invisibly). Redialing once here is *parity*, not a new retry loop — and
//!   unlike hyper it is counted (`redials`) instead of being invisible.
//! - **Deadlines stay with the caller.** `header_timeout`/`stall_timeout` are left `None`
//!   so `ChunkDeadline` (60s first byte / 120s idle / 90s total) remains the only
//!   observer; a transport-originated `HeaderTimeout`/`Stall` would be a bug in this
//!   configuration.
//! - **Header ownership**: `host` and `content-length` are dropped before handing the
//!   request over, because xlite synthesizes both when absent and rejects a
//!   `content-length` that disagrees with the body. That is the same effective behaviour
//!   as `reqwest`, which also ignores a caller-supplied length.

use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt;
use xlite_transport::{Attempt, StaleKind, Transport, TransportConfig, TransportError};

use crate::upstream::{
    UpstreamBody, UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamStats,
    UpstreamTransport,
};

fn default_connect_timeout() -> std::time::Duration {
    std::env::var("NEBULA_UPSTREAM_CONNECT_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or(std::time::Duration::from_secs(3))
}

/// xlite-transport backed engine transport.
pub struct XliteTransport {
    inner: xlite_transport::H1Transport,
    redials: AtomicU64,
}

impl XliteTransport {
    /// Label used for logs and the `transport` metric label.
    pub const NAME: &'static str = "xlite";

    /// Build from the environment knobs Nebula already owns (`NEBULA_PROXY_*`), keeping
    /// the per-attempt deadlines where they belong — with the caller.
    pub fn from_env() -> Self {
        Self::with_timeouts(default_connect_timeout(), None)
    }

    /// Same transport with explicit budgets. Used by the contract tests to prove both
    /// directions of "deadlines belong to the caller": `None` never fires, `Some` fires
    /// exactly once, from the transport that was given it.
    #[doc(hidden)]
    pub fn with_timeouts(
        connect_timeout: std::time::Duration,
        header_timeout: Option<std::time::Duration>,
    ) -> Self {
        let cfg = TransportConfig {
            connect_timeout,
            header_timeout,
            // Never: the caller's `ChunkDeadline` owns the stream-idle budget, and a
            // second watcher here would be a double-firing deadline.
            stall_timeout: None,
            ..TransportConfig::cleartext()
        };
        Self {
            inner: xlite_transport::H1Transport::new(cfg),
            redials: AtomicU64::new(0),
        }
    }
}

/// Map xlite's typed failure onto the seam's taxonomy.
///
/// `Tls`, `Overflow`, `Closed` and `Io` have no dedicated variant here yet (the seam was
/// written against `reqwest`, which cannot produce them), so they fold into `Connect` or
/// `Other`; `StaleConnection` only reaches this point when the redial itself failed, which
/// is a connection failure by then.
fn map_error(error: TransportError) -> UpstreamError {
    match error {
        TransportError::InvalidRequest(why) => {
            UpstreamError::InvalidRequest(format!("xlite: {why}"))
        }
        TransportError::Dns(e) => UpstreamError::Dns(Box::new(e)),
        TransportError::Connect(e) | TransportError::Tls(e) | TransportError::Io(e) => {
            UpstreamError::Connect(Box::new(e))
        }
        TransportError::HeaderTimeout { .. } => UpstreamError::HeaderTimeout,
        TransportError::Stall { .. } => UpstreamError::Stall,
        TransportError::BodyIncomplete => UpstreamError::BodyIncomplete,
        TransportError::Reset { code } => {
            // The seam's variant is unit-shaped; keep the peer code in the log rather
            // than inventing a second variant for one implementation.
            if let Some(code) = code {
                tracing::debug!(h2_code = code, "upstream stream reset");
            }
            UpstreamError::Reset
        }
        TransportError::Cancelled => UpstreamError::Cancelled,
        TransportError::StaleConnection { kind } => {
            let why = match kind {
                StaleKind::PeerClosed => "peer closed".to_string(),
                StaleKind::Reset => "reset".to_string(),
                StaleKind::Io => "io".to_string(),
                // `StaleKind` is `#[non_exhaustive]`.
                other => format!("{other:?}"),
            };
            UpstreamError::Connect(Box::new(std::io::Error::other(format!(
                "stale pooled connection not recovered ({why})"
            ))))
        }
        // `#[non_exhaustive]` on xlite's side: a future variant must not be silently
        // classified as something it is not.
        other => UpstreamError::Other(Box::new(std::io::Error::other(other.to_string()))),
    }
}

/// The seam's request, translated. `host`/`content-length` are dropped because the
/// transport owns them (and rejects a length that disagrees with the body).
fn to_attempt(request: UpstreamRequest) -> Result<Attempt, UpstreamError> {
    let uri: http::Uri = request
        .url
        .parse()
        .map_err(|e| UpstreamError::InvalidRequest(format!("unparsable upstream URL: {e}")))?;
    let mut headers = request.headers;
    headers.remove(http::header::HOST);
    headers.remove(http::header::CONTENT_LENGTH);
    Ok(Attempt {
        method: request.method,
        uri,
        headers,
        body: request.body.unwrap_or_default(),
    })
}

#[async_trait::async_trait]
impl UpstreamTransport for XliteTransport {
    async fn execute(&self, request: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        let attempt = to_attempt(request)?;
        let outcome = self
            .inner
            .execute_redial_once(attempt)
            .await
            .map_err(map_error)?;
        if outcome.telemetry.redials > 0 {
            self.redials.fetch_add(1, Ordering::Relaxed);
        }
        let body = UpstreamBody::new(outcome.body.map(|chunk| chunk.map_err(map_error)));
        Ok(UpstreamResponse::new(outcome.status, outcome.headers, body))
    }

    fn stats(&self) -> UpstreamStats {
        let snapshot = self.inner.stats();
        snapshot.assert_invariants();
        // The seam reports one flat snapshot; per-authority rows are dropped here on
        // purpose (the label set would otherwise be replica-cardinality). Counters sum
        // exactly, wait percentiles do not aggregate and stay `None`.
        let mut dials = 0;
        let mut failed_dial = 0;
        let mut in_flight = 0;
        let mut connections = 0;
        for host in &snapshot.hosts {
            dials += host.dials;
            failed_dial += host.failed_dials;
            in_flight += host.in_flight;
            connections += host.connections;
        }
        UpstreamStats {
            transport: Self::NAME,
            dials: Some(dials),
            failed_dial: Some(failed_dial),
            in_flight: Some(in_flight),
            connections: Some(connections),
            redials: Some(self.redials.load(Ordering::Relaxed)),
        }
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }
}
