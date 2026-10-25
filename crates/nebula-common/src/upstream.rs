//! Outbound HTTP transport seam for the inference data plane.
//!
//! One contract for "send this request to an engine replica and give me the raw
//! response bytes back": [`UpstreamTransport::execute`]. Today the only
//! implementation is [`ReqwestTransport`]; the point of the seam is that the
//! `xlite-transport` data plane (xlite#96) can be dropped in behind the same trait
//! without touching retry, deadline, cancellation or metric semantics, which all stay
//! with the caller (Gateway / Router).
//!
//! Boundaries this module deliberately keeps:
//!
//! - **One call = one upstream attempt.** No retry, no failover, no reroute, no
//!   cooldown. The caller owns the attempt budget (`retry_max`, first-byte budget,
//!   cross-attempt total budget).
//! - **Zero parsing.** Request body in, `Bytes` out. No JSON DOM, no SSE framing.
//! - **Deadlines belong to the caller.** `reqwest`'s own timeout is whatever the
//!   injected client was built with; the per-phase budgets
//!   ([`crate::first_byte_timeout`] / [`crate::stream_idle_timeout`]) are enforced by
//!   the caller's stream loop, so there is exactly one observer per attempt.
//! - **Dropping the body cancels upstream.** The `in_flight` counter is released from
//!   the body's `Drop`, and dropping a `reqwest` body stream closes the connection.
//!
//! **Not guaranteed by every implementation:** `reqwest`/hyper transparently re-sends a
//! request whose pooled connection was already closed by the peer, so one `execute()` can
//! put two requests on the wire when the socket went stale between checkout and write.
//! The contract test cannot see it (its mock closes the connection after each response, so
//! every attempt is a fresh socket). `xlite-transport` must not inherit this: there the
//! re-dial is an explicit caller-visible `execute_redial_once` with telemetry.
//!
//! Selection is `NEBULA_UPSTREAM_TRANSPORT=reqwest|xlite` (default `reqwest`) via
//! [`upstream_transport_from_env`]. An unimplemented value fails at startup instead of
//! falling back silently: an A/B that quietly runs two clones of the same transport
//! measures nothing.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use http::{HeaderMap, Method, StatusCode};

/// A response body as raw bytes, carrying the transport's error type.
type BoxBytesStream = Pin<Box<dyn Stream<Item = Result<Bytes, UpstreamError>> + Send>>;

/// Why an upstream attempt did not produce a usable response.
///
/// `#[non_exhaustive]`: new variants are additive. `label()` is the *metric* taxonomy and
/// is frozen — it reproduces the strings the pre-seam `classify_reqwest_error` produced
/// (`timeout` / `connect` / `other`), so existing dashboards and alert rules keep
/// working while the typed variants give callers something better to match on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UpstreamError {
    /// The request could not be built or sent (bad URL, bad header, …). A caller bug,
    /// never an upstream failure: call sites must skip the upstream-error counter and
    /// must not blame the replica (see [`UpstreamError::is_caller_fault`]).
    #[error("invalid upstream request: {0}")]
    InvalidRequest(String),
    /// Name resolution failed.
    ///
    /// Produced by `xlite-transport` (xlite#96 item 2), which types its dial errors; the
    /// `reqwest` implementation cannot tell DNS from connect and reports [`Self::Connect`].
    #[error("dns resolution failed")]
    Dns(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// TCP connect failed, or a TLS handshake did: `reqwest` does not expose the two
    /// separately, so both land here. `xlite-transport` reports its handshake failures as
    /// its own `Tls` variant, which the adapter maps back here until this enum splits.
    #[error("connect failed")]
    Connect(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// The attempt expired before response headers arrived.
    #[error("timed out waiting for upstream headers")]
    HeaderTimeout,
    /// The body stalled past the caller's idle budget.
    ///
    /// Produced by a transport that runs its own stall watchdog; Nebula passes no stall
    /// budget (deadlines belong to `ChunkDeadline`), so this is unreachable from the
    /// `reqwest` implementation by construction — and if it ever appears, that is a bug.
    #[error("upstream body stalled")]
    Stall,
    /// The connection ended before the body completed (truncated response).
    ///
    /// Produced by `xlite-transport` (typed pump errors); `reqwest` surfaces a truncated
    /// body as a decode/other error, so it arrives as [`Self::Other`] today.
    #[error("upstream body incomplete")]
    BodyIncomplete,
    /// The peer reset the connection.
    ///
    /// Same producer split as [`Self::BodyIncomplete`]: typed by `xlite-transport`,
    /// [`Self::Other`] under `reqwest`.
    #[error("upstream connection reset")]
    Reset,
    /// The caller dropped the attempt (client abort).
    ///
    /// Only a transport that observes its own cancellation can report this; the caller
    /// notices an abort as a dropped body, so no current implementation produces it.
    #[error("upstream attempt cancelled")]
    Cancelled,
    /// Transport failure that fits none of the above; the cause is preserved.
    #[error("upstream transport failure")]
    Other(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl UpstreamError {
    /// True when the failure is Nebula's own bug rather than the upstream's.
    ///
    /// Call sites must not count these as upstream errors, must not mark the replica
    /// unhealthy, and must not retry another replica: a request we failed to build would
    /// fail identically everywhere.
    pub fn is_caller_fault(&self) -> bool {
        matches!(self, UpstreamError::InvalidRequest(_))
    }

    /// Metric label from the frozen three-value taxonomy.
    pub fn label(&self) -> &'static str {
        match self {
            UpstreamError::HeaderTimeout | UpstreamError::Stall => "timeout",
            UpstreamError::Dns(_) | UpstreamError::Connect(_) => "connect",
            _ => "other",
        }
    }
}

/// Metric label for a `reqwest` error, kept as the single mapping table so the Gateway's
/// internal hop and the engine path cannot drift apart.
pub fn reqwest_error_kind(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        return "timeout";
    }
    if error.is_connect() {
        return "connect";
    }
    "other"
}

/// Map a `reqwest` error onto the typed taxonomy.
///
/// `reqwest` does not expose typed DNS/TLS discrimination, so those two variants are
/// produced by transports that can tell them apart (the xlite data plane); here the
/// distinction collapses into `Connect`, and `Display` is preserved in the source chain.
fn from_reqwest_error(error: reqwest::Error) -> UpstreamError {
    // A request the client refuses to build is a caller bug, not an upstream failure:
    // it must be distinguishable so it never reaches the upstream-error counters.
    if error.is_builder() {
        return UpstreamError::InvalidRequest(error.to_string());
    }
    let kind = reqwest_error_kind(&error);
    match kind {
        "timeout" => UpstreamError::HeaderTimeout,
        "connect" => UpstreamError::Connect(Box::new(error)),
        _ => UpstreamError::Other(Box::new(error)),
    }
}

/// A prepared outbound request. `body` is `Bytes` so a caller can replay it across
/// attempts without re-serializing.
#[derive(Debug, Clone)]
pub struct UpstreamRequest {
    pub method: Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
}

impl UpstreamRequest {
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HeaderMap::new(),
            body: None,
        }
    }
}

/// Streaming body with the transport's error type.
pub struct UpstreamBody {
    inner: BoxBytesStream,
}

impl std::fmt::Debug for UpstreamBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UpstreamBody")
    }
}

impl UpstreamBody {
    pub fn new(stream: impl Stream<Item = Result<Bytes, UpstreamError>> + Send + 'static) -> Self {
        Self {
            inner: Box::pin(stream),
        }
    }

    /// Replay `head` before the rest of the stream.
    ///
    /// This is the pre-first-byte commitment pattern: read one chunk to decide whether
    /// the status may be committed, then hand downstream a stream that still starts with
    /// that chunk so no byte is lost or reordered.
    pub fn prepend(self, head: Bytes) -> Self {
        Self::new(futures_util::stream::once(async move { Ok(head) }).chain(self))
    }
}

impl Stream for UpstreamBody {
    type Item = Result<Bytes, UpstreamError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

/// Response head plus the unread body.
#[derive(Debug)]
pub struct UpstreamResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: UpstreamBody,
}

impl UpstreamResponse {
    /// The constructor a transport implementation uses to hand a response back
    /// (`xlite-transport` will call this; `from_reqwest` does today).
    pub fn new(status: StatusCode, headers: HeaderMap, body: UpstreamBody) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// Adapter for the one path that is not an engine call: the Gateway → Router hop,
    /// which keeps using `reqwest` directly (it is Nebula-internal, not the engine data
    /// plane the xlite transport replaces).
    pub fn from_reqwest(response: reqwest::Response) -> Self {
        let status = response.status();
        let headers = response.headers().clone();
        let body = UpstreamBody::new(
            response
                .bytes_stream()
                .map(|chunk| chunk.map_err(from_reqwest_error)),
        );
        Self::new(status, headers, body)
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Next body chunk; `None` at a clean end of body.
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, UpstreamError> {
        self.body.next().await.transpose()
    }

    pub fn bytes_stream(self) -> UpstreamBody {
        self.body
    }

    /// Read the whole body. The caller bounds this with its own idle deadline.
    pub async fn bytes(self) -> Result<Bytes, UpstreamError> {
        let mut body = self.body;
        let mut collected = Vec::new();
        while let Some(chunk) = body.next().await {
            collected.extend_from_slice(&chunk?);
        }
        Ok(Bytes::from(collected))
    }
}

/// Per-transport pool counters. `None` means *not measured*, never zero: the reqwest
/// client exposes no pool surface, and reporting 0 would make "no dials" and "cannot
/// see dials" indistinguishable in an A/B.
#[derive(Debug, Clone)]
pub struct UpstreamStats {
    pub transport: &'static str,
    pub dials: Option<u64>,
    pub failed_dial: Option<u64>,
    pub in_flight: Option<u64>,
    pub connections: Option<u64>,
}

impl Default for UpstreamStats {
    fn default() -> Self {
        Self {
            transport: "unknown",
            dials: None,
            failed_dial: None,
            in_flight: None,
            connections: None,
        }
    }
}

impl UpstreamStats {
    /// Append Prometheus lines for the measured fields only; unmeasured ones emit nothing
    /// rather than a fake zero.
    ///
    /// The series carry no component prefix because the Gateway and the Router are
    /// separate scrape targets publishing their own `transport` label — two processes
    /// never contribute to the same series.
    pub fn render_prometheus(&self, out: &mut String) {
        let transport = self.transport;
        let mut emit = |metric: &str, help: &str, kind: &str, value: Option<u64>| {
            if let Some(v) = value {
                out.push_str(&format!(
                    "# HELP {metric} {help}\n# TYPE {metric} {kind}\n\
                     {metric}{{transport=\"{transport}\"}} {v}\n"
                ));
            }
        };
        emit(
            "nebula_upstream_dials_total",
            "Upstream connections dialled by the engine transport (unmeasured by reqwest).",
            "counter",
            self.dials,
        );
        emit(
            "nebula_upstream_failed_dial_total",
            "Upstream dial attempts that failed before a request could be written.",
            "counter",
            self.failed_dial,
        );
        emit(
            "nebula_upstream_in_flight",
            "Upstream attempts currently holding a response body.",
            "gauge",
            self.in_flight,
        );
        emit(
            "nebula_upstream_connections",
            "Pooled upstream connections (unmeasured by reqwest).",
            "gauge",
            self.connections,
        );
    }
}

/// One outbound attempt against one upstream address.
#[async_trait::async_trait]
pub trait UpstreamTransport: Send + Sync {
    /// Execute exactly one attempt. No retry, no failover, no hidden redial.
    async fn execute(&self, request: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError>;

    /// Pool counters; fields the implementation cannot measure stay `None`.
    fn stats(&self) -> UpstreamStats;

    /// Stable name for logs and metric labels (`reqwest`, `xlite`).
    fn name(&self) -> &'static str;
}

/// `reqwest`-backed transport: the current behaviour, unchanged.
pub struct ReqwestTransport {
    client: reqwest::Client,
    in_flight: Arc<AtomicU64>,
    failed_dial: AtomicU64,
}

impl ReqwestTransport {
    /// Label used for logs and the `transport` metric label.
    pub const NAME: &'static str = "reqwest";

    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            in_flight: Arc::new(AtomicU64::new(0)),
            failed_dial: AtomicU64::new(0),
        }
    }
}

/// Releases the in-flight gauge when the body (or the response) is dropped, so a client
/// abort is visible as a drop rather than a leaked gauge.
struct InFlightGuard(Arc<AtomicU64>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[async_trait::async_trait]
impl UpstreamTransport for ReqwestTransport {
    async fn execute(&self, request: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        if request.url.trim().is_empty() {
            return Err(UpstreamError::InvalidRequest("empty upstream URL".into()));
        }
        let mut builder = self
            .client
            .request(request.method, &request.url)
            .headers(request.headers);
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        let guard = InFlightGuard(Arc::clone(&self.in_flight));
        match builder.send().await {
            Ok(response) => {
                let status = response.status();
                let headers = response.headers().clone();
                let body = UpstreamBody::new(response.bytes_stream().map(move |chunk| {
                    // Keep the gauge alive for the body's lifetime.
                    let _ = &guard;
                    chunk.map_err(from_reqwest_error)
                }));
                Ok(UpstreamResponse::new(status, headers, body))
            }
            Err(error) => {
                if matches!(reqwest_error_kind(&error), "connect") {
                    self.failed_dial.fetch_add(1, Ordering::Relaxed);
                }
                Err(from_reqwest_error(error))
            }
        }
    }

    fn stats(&self) -> UpstreamStats {
        UpstreamStats {
            // reqwest reports no pool internals: `None`, not 0.
            dials: None,
            failed_dial: Some(self.failed_dial.load(Ordering::Relaxed)),
            in_flight: Some(self.in_flight.load(Ordering::Relaxed)),
            connections: None,
            transport: Self::NAME,
        }
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }
}

/// Build the configured transport.
///
/// Fails loudly on an unknown or not-yet-built implementation: silently running
/// `reqwest` for `NEBULA_UPSTREAM_TRANSPORT=xlite` would make an A/B compare two
/// identical transports.
pub fn upstream_transport_from_env() -> Result<Arc<dyn UpstreamTransport>, String> {
    let choice = std::env::var("NEBULA_UPSTREAM_TRANSPORT")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match choice.as_str() {
        "" | "reqwest" => {
            let client = crate::http::proxy_http_client()
                .map_err(|e| format!("failed to build the reqwest upstream client: {e}"))?;
            Ok(Arc::new(ReqwestTransport::new(client)))
        }
        "xlite" => Err(
            "NEBULA_UPSTREAM_TRANSPORT=xlite: the xlite-transport data plane is not compiled \
             into this build (upstream of xlite#96); rebuild with the xlite feature once it \
             lands instead of silently falling back to reqwest"
                .to_string(),
        ),
        other => Err(format!(
            "NEBULA_UPSTREAM_TRANSPORT={other:?} is not a transport (expected reqwest|xlite; \
             unset defaults to reqwest)"
        )),
    }
}
