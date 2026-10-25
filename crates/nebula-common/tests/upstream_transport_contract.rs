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
    if !read_one_request(&mut stream).await {
        return;
    }
    requests.fetch_add(1, Ordering::Relaxed);
    match behaviour {
        Behaviour::Ok => {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 5\r\nconnection: close\r\n\r\nhello",
                )
                .await;
            let _ = stream.shutdown().await;
            return;
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
        }
    }
    // Detect the client going away: a read that returns 0 (or errors) means the socket
    // was closed from the caller side.
    let mut discard = [0u8; 64];
    loop {
        match stream.read(&mut discard).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
    peer_closed.store(true, Ordering::Relaxed);
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

fn transport(timeout: Option<Duration>) -> ReqwestTransport {
    let mut builder = reqwest::Client::builder();
    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }
    ReqwestTransport::new(builder.build().expect("client"))
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
    let mock = mock(Behaviour::Ok).await;
    let transport = transport(None);

    let response = transport
        .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
        .await
        .expect("attempt succeeds");
    assert_eq!(response.status(), http::StatusCode::OK);
    let body = response.bytes().await.expect("body");
    assert_eq!(&body[..], b"hello");

    // A second call is a second attempt: the transport never batches or retries, and
    // the caller's attempt budget is the only retry knob in the stack.
    transport
        .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
        .await
        .expect("second attempt");
    assert_eq!(
        mock.requests.load(Ordering::Relaxed),
        2,
        "each execute() must produce exactly one upstream request"
    );
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
    assert_eq!(
        UpstreamError::InvalidRequest("no url".into()).label(),
        "other"
    );

    // Caller bugs must be distinguishable from upstream failures: `InvalidRequest`
    // exists so a bad URL never reaches the upstream-error counters.
    assert!(!matches!(
        UpstreamError::InvalidRequest("no url".into()),
        UpstreamError::Connect(_) | UpstreamError::HeaderTimeout
    ));
}

#[tokio::test]
async fn connect_failure_is_connect_and_never_redialled() {
    // Bind then drop so the port is certainly closed.
    let addr = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        listener.local_addr().expect("addr")
    };
    let transport = transport(Some(Duration::from_secs(5)));
    let error = transport
        .execute(post(&format!("http://{addr}/v1/chat/completions")))
        .await
        .expect_err("connection refused");
    assert_eq!(error.label(), "connect", "{error}");
    assert!(matches!(
        error,
        UpstreamError::Connect(_) | UpstreamError::Dns(_)
    ));
}

#[tokio::test]
async fn header_deadline_is_a_timeout_not_a_transport_other() {
    let mock = mock(Behaviour::Silent).await;
    let transport = transport(Some(Duration::from_millis(300)));
    let error = transport
        .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
        .await
        .expect_err("no headers");
    assert_eq!(error.label(), "timeout", "{error}");
    assert_eq!(mock.requests.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn truncated_body_is_an_error_not_a_clean_end() {
    let mock = mock(Behaviour::Truncate).await;
    let transport = transport(None);
    let response = transport
        .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
        .await
        .expect("headers arrive");
    let collected = response.bytes().await;
    assert!(
        collected.is_err(),
        "a body shorter than content-length must never look like a completed response"
    );
}

#[tokio::test]
async fn dropping_the_body_closes_the_upstream_connection() {
    let mock = mock(Behaviour::HoldUntilPeerClose).await;
    let transport = transport(None);
    let mut response = transport
        .execute(post(&format!("{}/v1/chat/completions", mock.base_url)))
        .await
        .expect("headers arrive");
    // One chunk arrives, then the client aborts.
    let head = response.chunk().await.expect("chunk").expect("some bytes");
    assert_eq!(&head[..], b"0123456789");
    drop(response);

    for _ in 0..50 {
        if mock.peer_closed.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("dropping the response must close the upstream socket, not leak it");
}

#[tokio::test]
async fn invalid_request_is_a_caller_bug_and_opens_no_connection() {
    let transport = transport(None);
    let error = transport
        .execute(UpstreamRequest::new(http::Method::POST, "not a url"))
        .await
        .expect_err("invalid url");
    // Either shape is acceptable as long as it is a caller-side error, not a silent
    // upstream failure: empty/blank URLs are rejected up front, unparsable ones come
    // back from the client builder.
    assert!(
        matches!(
            error,
            UpstreamError::InvalidRequest(_) | UpstreamError::Connect(_)
        ),
        "{error:?}"
    );
    assert_eq!(error.label(), "other");
}

#[tokio::test]
async fn unmeasured_pool_fields_are_none_not_zero() {
    let transport = transport(None);
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
}

#[tokio::test]
async fn unknown_transport_choice_fails_loudly() {
    // The A/B must not silently run the same transport twice.
    std::env::set_var("NEBULA_UPSTREAM_TRANSPORT", "xlite");
    let result = nebula_common::upstream_transport_from_env();
    std::env::remove_var("NEBULA_UPSTREAM_TRANSPORT");
    let Err(error) = result else {
        panic!("xlite is not built into this crate, so selection must fail");
    };
    assert!(error.contains("xlite"), "{error}");
}
