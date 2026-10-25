//! Contract tests for the outbound transport seam (`nebula-common::upstream`).
//!
//! These pin the properties the Gateway/Router rely on and that the future
//! `xlite-transport` swap must preserve, using a raw TCP mock so the assertions are
//! about bytes on the wire rather than about `reqwest` internals:
//!
//! 1. one `execute()` = exactly one upstream request (the attempt budget belongs to the
//!    caller, so the transport may never retry or redial);
//! 2. the metric label taxonomy stays the frozen `timeout` / `connect` / `other`;
//! 3. a truncated body is an error, never a clean end of stream;
//! 4. dropping the body closes the upstream connection (client abort);
//! 5. unmeasured pool fields are `None`, never a fake `0`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use nebula_common::upstream::{
    ReqwestTransport, UpstreamError, UpstreamRequest, UpstreamTransport,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What the mock does after it has read one request.
#[derive(Clone, Copy)]
enum Behaviour {
    /// Answer `200` with a complete body.
    Ok,
    /// Promise 100 bytes, send 10, then close: a truncated response.
    Truncate,
    /// Never answer: exercises the header deadline.
    Silent,
    /// Promise 100 bytes, send 10, then hold the socket until the peer disconnects.
    HoldUntilPeerClose,
}

struct Mock {
    base_url: String,
    requests: Arc<AtomicU64>,
    peer_closed: Arc<AtomicBool>,
}

async fn read_one_request(stream: &mut TcpStream) -> bool {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => return false,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buf[..end]).to_string();
                    let content_length = head
                        .lines()
                        .find_map(|l| {
                            l.split_once(':').and_then(|(k, v)| {
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                        })
                        .unwrap_or(0);
                    let body_have = buf.len() - (end + 4);
                    return body_have >= content_length;
                }
            }
            Err(_) => return false,
        }
    }
}

async fn serve(
    mut stream: TcpStream,
    behaviour: Behaviour,
    requests: Arc<AtomicU64>,
    peer_closed: Arc<AtomicBool>,
) {
    loop {
        if !read_one_request(&mut stream).await {
            return;
        }
        requests.fetch_add(1, Ordering::Relaxed);
        match behaviour {
            Behaviour::Ok => {
                // Keep-alive on purpose: a reusable pooled socket is the normal case, and
                // it keeps the "one execute = one request" assertion about attempts rather
                // than about stale-socket redials.
                if stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 5\r\n\r\nhello",
                    )
                    .await
                    .is_err()
                {
                    return;
                }
                continue;
            }
            Behaviour::Truncate => {
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 100\r\n\r\n0123456789")
                    .await;
                let _ = stream.shutdown().await;
                peer_closed.store(true, Ordering::Relaxed);
                return;
            }
            Behaviour::Silent => {
                tokio::time::sleep(Duration::from_secs(30)).await;
                return;
            }
            Behaviour::HoldUntilPeerClose => {
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 100\r\n\r\n0123456789")
                    .await;
                let mut discard = [0u8; 64];
                loop {
                    match stream.read(&mut discard).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
                peer_closed.store(true, Ordering::Relaxed);
                return;
            }
        }
    }
}

async fn mock(behaviour: Behaviour) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    let requests = Arc::new(AtomicU64::new(0));
    let peer_closed = Arc::new(AtomicBool::new(false));
    let requests_task = Arc::clone(&requests);
    let peer_closed_task = Arc::clone(&peer_closed);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            // One task per connection so a held keep-alive socket cannot block accept.
            tokio::spawn(serve(
                stream,
                behaviour,
                Arc::clone(&requests_task),
                Arc::clone(&peer_closed_task),
            ));
        }
    });
    Mock {
        base_url: format!("http://{addr}"),
        requests,
        peer_closed,
    }
}

fn reqwest_transport(timeout: Option<Duration>) -> ReqwestTransport {
    let mut builder = reqwest::Client::builder();
    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }
    ReqwestTransport::new(builder.build().expect("client"))
}

/// Every transport the seam is expected to support, so the same assertions run against
/// all of them. With `--features xlite` the contract suite covers the xlite data plane
/// too, which is the whole point of the seam: identical behaviour, swappable transport.
fn transports(timeout: Option<Duration>) -> Vec<(&'static str, Arc<dyn UpstreamTransport>)> {
    #[cfg_attr(not(feature = "xlite"), allow(unused_mut))]
    let mut all: Vec<(&'static str, Arc<dyn UpstreamTransport>)> =
        vec![("reqwest", Arc::new(reqwest_transport(timeout)))];
    #[cfg(feature = "xlite")]
    all.push((
        "xlite",
        Arc::new(nebula_common::XliteTransport::with_timeouts(
            Duration::from_secs(3),
            timeout,
        )),
    ));
    all
}

fn post(url: &str) -> UpstreamRequest {
    let mut request = UpstreamRequest::new(http::Method::POST, url);
    request.headers.insert(
        http::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    request.body = Some(Bytes::from_static(br#"{"model":"m"}"#));
    request
}

#[tokio::test]
async fn one_execute_is_exactly_one_upstream_request() {
    for (name, transport) in transports(None) {
        let mock = mock(Behaviour::Ok).await;
        let url = format!("{}/v1/chat/completions", mock.base_url);

        let response = transport
            .execute(post(&url))
            .await
            .unwrap_or_else(|e| panic!("[{name}] attempt must succeed: {e}"));
        assert_eq!(response.status(), http::StatusCode::OK, "[{name}] status");
        let body = response
            .bytes()
            .await
            .unwrap_or_else(|e| panic!("[{name}] body: {e}"));
        assert_eq!(&body[..], b"hello", "[{name}] body bytes");

        // A second call is a second attempt: the transport never batches or retries, and
        // the caller's attempt budget is the only retry knob in the stack.
        transport
            .execute(post(&url))
            .await
            .unwrap_or_else(|e| panic!("[{name}] second attempt: {e}"));
        assert_eq!(
            mock.requests.load(Ordering::Relaxed),
            2,
            "[{name}] each execute() must produce exactly one upstream request"
        );
    }
}

#[tokio::test]
async fn error_taxonomy_labels_are_frozen() {
    // The metric taxonomy predates the typed enum and must not move: dashboards and
    // alert rules key on these three strings.
    assert_eq!(UpstreamError::HeaderTimeout.label(), "timeout");
    assert_eq!(UpstreamError::Stall.label(), "timeout");
    assert_eq!(
        UpstreamError::Connect(Box::new(std::io::Error::other("refused"))).label(),
        "connect"
    );
    assert_eq!(
        UpstreamError::Dns(Box::new(std::io::Error::other("dns"))).label(),
        "connect"
    );
    assert_eq!(UpstreamError::BodyIncomplete.label(), "other");
    assert_eq!(UpstreamError::Reset.label(), "other");
    assert_eq!(UpstreamError::Cancelled.label(), "other");
    // A caller fault keeps the "other" label purely so the frozen series shape holds if
    // it ever leaks past `is_caller_fault`; the branch above is what actually protects it.
    assert_eq!(
        UpstreamError::InvalidRequest("no url".into()).label(),
        "other"
    );

    // Caller bugs are a separate class: `is_caller_fault()` is what the Gateway/Router
    // branches on to skip the upstream-error counter, skip `record_endpoint_failure` and
    // skip the replica retry — so this predicate is the invariant, not the label.
    assert!(UpstreamError::InvalidRequest("no url".into()).is_caller_fault());
    for upstream_fault in [
        UpstreamError::HeaderTimeout,
        UpstreamError::Stall,
        UpstreamError::BodyIncomplete,
        UpstreamError::Reset,
        UpstreamError::Cancelled,
        UpstreamError::Other(Box::new(std::io::Error::other("x"))),
        UpstreamError::Connect(Box::new(std::io::Error::other("refused"))),
    ] {
        assert!(
            !upstream_fault.is_caller_fault(),
            "{upstream_fault:?} is an upstream failure and must be counted/retried"
        );
    }
}

#[tokio::test]
async fn connect_failure_is_connect_and_never_redialled() {
    // Bind then drop so the port is certainly closed.
    let addr = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        listener.local_addr().expect("addr")
    };
    for (name, transport) in transports(Some(Duration::from_secs(5))) {
        let error = transport
            .execute(post(&format!("http://{addr}/v1/chat/completions")))
            .await
            .expect_err("connection refused");
        assert_eq!(error.label(), "connect", "[{name}] {error}");
        assert!(
            matches!(error, UpstreamError::Connect(_) | UpstreamError::Dns(_)),
            "[{name}] a refused connection is a pre-request failure: {error:?}"
        );
        assert!(
            !error.is_caller_fault(),
            "[{name}] a refused connection is the upstream's problem, not ours"
        );
    }
}

#[tokio::test]
async fn header_deadline_is_a_timeout_not_a_transport_other() {
    for (name, transport) in transports(Some(Duration::from_millis(300))) {
        let mock = mock(Behaviour::Silent).await;
        let error = transport
            .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
            .await
            .expect_err("no headers");
        assert_eq!(error.label(), "timeout", "[{name}] {error}");
        assert_eq!(
            mock.requests.load(Ordering::Relaxed),
            1,
            "[{name}] a header deadline must not trigger a second request"
        );
    }
}

#[tokio::test]
async fn truncated_body_is_an_error_not_a_clean_end() {
    for (name, transport) in transports(None) {
        let mock = mock(Behaviour::Truncate).await;
        let response = transport
            .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
            .await
            .unwrap_or_else(|e| panic!("[{name}] headers arrive: {e}"));
        let collected = response.bytes().await;
        assert!(
            collected.is_err(),
            "[{name}] a body shorter than content-length must never look like a completed response"
        );
    }
}

#[tokio::test]
async fn dropping_the_body_closes_the_upstream_connection() {
    for (name, transport) in transports(None) {
        let mock = mock(Behaviour::HoldUntilPeerClose).await;
        let mut response = transport
            .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
            .await
            .unwrap_or_else(|e| panic!("[{name}] headers arrive: {e}"));
        // One chunk arrives, then the client aborts.
        let head = response
            .chunk()
            .await
            .unwrap_or_else(|e| panic!("[{name}] chunk: {e}"))
            .unwrap_or_else(|| panic!("[{name}] expected a first chunk"));
        assert_eq!(&head[..], b"0123456789", "[{name}] first chunk");
        drop(response);

        let mut closed = false;
        for _ in 0..50 {
            if mock.peer_closed.load(Ordering::Relaxed) {
                closed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(
            closed,
            "[{name}] dropping the response must close the upstream socket, not leak it"
        );
    }
}

/// The classification is what the call sites branch on; no mock is needed because the
/// failure is decided before any socket exists (an empty URL is rejected up front, an
/// unparsable one is rejected by the client builder).
#[tokio::test]
async fn invalid_url_is_a_caller_fault_not_an_upstream_failure() {
    // No mock: the failure is decided before any socket exists.
    for (name, transport) in transports(None) {
        let error = transport
            .execute(UpstreamRequest::new(http::Method::POST, "not a url"))
            .await
            .expect_err("invalid url");
        // Strict on purpose: this assertion exists to catch the regression where a
        // builder error is classified as an upstream fault again (see `is_caller_fault`),
        // so it must not accept `Connect`.
        assert!(
            matches!(error, UpstreamError::InvalidRequest(_)),
            "[{name}] an unbuildable request must be InvalidRequest, never an upstream failure: {error:?}"
        );
        assert!(error.is_caller_fault(), "[{name}] {error:?}");
    }
}

#[tokio::test]
async fn unmeasured_pool_fields_are_none_not_zero() {
    // `reqwest` specifically: it exposes no pool surface, so the fields must stay `None`.
    let transport = reqwest_transport(None);
    let stats = transport.stats();
    assert!(stats.dials.is_none(), "reqwest exposes no dial counter");
    assert!(stats.connections.is_none(), "reqwest exposes no pool size");
    assert!(stats.in_flight.is_some(), "in-flight is measured here");
    let mut rendered = String::new();
    stats.render_prometheus(&mut rendered);
    assert!(
        !rendered.contains("nebula_upstream_dials_total"),
        "an unmeasured field must not be rendered as 0: {rendered}"
    );
    assert!(rendered.contains("nebula_upstream_in_flight"), "{rendered}");
    assert!(
        !rendered.contains("nebula_upstream_stale_redial_total"),
        "reqwest redials invisibly (hyper), so it must not claim a redial counter: {rendered}"
    );
}

/// The xlite transport *does* measure the pool, so the fields that reqwest leaves `None`
/// must be real numbers here — that is what the A/B will read.
#[cfg(feature = "xlite")]
#[tokio::test]
async fn xlite_reports_measured_pool_counters() {
    let mock = mock(Behaviour::Ok).await;
    let transport = nebula_common::XliteTransport::with_timeouts(Duration::from_secs(3), None);
    transport
        .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
        .await
        .expect("attempt succeeds");

    let stats = transport.stats();
    assert_eq!(stats.transport, "xlite");
    assert!(stats.dials.is_some(), "xlite counts dials");
    assert!(stats.connections.is_some(), "xlite reports pool size");
    assert!(stats.failed_dial.is_some(), "xlite counts failed dials");
    assert_eq!(
        stats.redials,
        Some(0),
        "a healthy keep-alive connection must not redial"
    );
    let mut rendered = String::new();
    stats.render_prometheus(&mut rendered);
    assert!(
        rendered.contains("nebula_upstream_stale_redial_total"),
        "{rendered}"
    );
}

#[tokio::test]
async fn transport_selection_matches_what_this_build_contains() {
    // The A/B must not silently run the same transport twice: asking for `xlite` in a
    // build without it is an error, not a quiet fallback to reqwest.
    std::env::set_var("NEBULA_UPSTREAM_TRANSPORT", "xlite");
    let result = nebula_common::upstream_transport_from_env();
    std::env::remove_var("NEBULA_UPSTREAM_TRANSPORT");

    #[cfg(feature = "xlite")]
    {
        let transport = result.expect("the xlite transport is compiled in");
        assert_eq!(transport.name(), "xlite");
    }
    #[cfg(not(feature = "xlite"))]
    {
        let Err(error) = result else {
            panic!("xlite is not built into this crate, so selection must fail");
        };
        assert!(error.contains("xlite"), "{error}");
    }
}
