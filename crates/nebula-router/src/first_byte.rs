//! First-byte commitment: decide `200` vs retry *before* the client sees a status.
//!
//! `reqwest::Client::send` resolves on response **headers**, so a replica that accepts
//! the request and then stalls in prefill already looks successful. Committing `200`
//! at that point makes the failure reportable but not retryable (issue #42).
//!
//! [`await_first_chunk`] reads the first body chunk under a deadline and hands it back
//! together with a stream that replays it, so the caller can retry another replica if
//! nothing arrived — and still stream normally once something did.

use std::time::Duration;

use bytes::Bytes;
use futures_util::{stream, Stream, StreamExt};

/// Read the first chunk of `stream` within `budget`.
///
/// `None` means *nothing reached the client yet*: the stream errored or ended without a
/// byte, or the budget elapsed. The caller may retry another replica safely, because no
/// status has been committed.
///
/// `Some((head, rest))` returns the first chunk plus a stream that yields `head` again
/// followed by the remaining chunks, so downstream code sees an unmodified byte stream.
pub async fn await_first_chunk<S>(
    mut stream: S,
    budget: Duration,
) -> Option<(Bytes, impl Stream<Item = reqwest::Result<Bytes>>)>
where
    S: Stream<Item = reqwest::Result<Bytes>> + Unpin,
{
    let head = match tokio::time::timeout(budget, stream.next()).await {
        Ok(Some(Ok(chunk))) if !chunk.is_empty() => chunk,
        // timeout, error, empty body or clean EOF before any byte
        _ => return None,
    };
    let replay = head.clone();
    Some((head, stream::once(async move { Ok(replay) }).chain(stream)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn ok(bytes: &'static [u8]) -> reqwest::Result<Bytes> {
        Ok(Bytes::from_static(bytes))
    }

    #[tokio::test]
    async fn returns_head_and_replays_it_before_the_rest() {
        let upstream = stream::iter(vec![ok(b"one"), ok(b"two"), ok(b"three")]);
        let (head, rest) = await_first_chunk(upstream, Duration::from_secs(5))
            .await
            .expect("first chunk");
        assert_eq!(&head[..], b"one");
        let collected: Vec<Bytes> = rest.map(|r| r.unwrap()).collect().await;
        assert_eq!(
            collected,
            vec![
                Bytes::from_static(b"one"),
                Bytes::from_static(b"two"),
                Bytes::from_static(b"three")
            ],
            "the head must be replayed so the client stream is unchanged"
        );
    }

    #[tokio::test]
    async fn silent_stream_times_out_as_retryable() {
        let silent = stream::pending::<reqwest::Result<Bytes>>();
        let started = tokio::time::Instant::now();
        assert!(await_first_chunk(silent, Duration::from_millis(50))
            .await
            .is_none());
        assert!(
            started.elapsed() >= Duration::from_millis(50),
            "the budget, not something else, must end the wait"
        );
    }

    #[tokio::test]
    async fn empty_body_and_immediate_eof_are_retryable() {
        // EOF before any byte: engine closed without answering.
        let eof = stream::iter(Vec::<reqwest::Result<Bytes>>::new());
        assert!(await_first_chunk(eof, Duration::from_secs(1))
            .await
            .is_none());

        // An empty first chunk carries no information; treat it as "nothing yet".
        let empty = stream::iter(vec![ok(b"")]);
        assert!(await_first_chunk(empty, Duration::from_secs(1))
            .await
            .is_none());
    }
}
