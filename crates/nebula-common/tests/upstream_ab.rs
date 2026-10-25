//! Transport A/B probe: same workload, two implementations of the engine seam.
//!
//! Opt-in (`--ignored`) because it needs a mock upstream and is a measurement, not an
//! assertion. `scripts/ab_upstream_transport.sh` starts the mock and runs this once per
//! transport, then diffs the two `AB_RESULT` lines.
//!
//! What it measures: what the seam actually changes — time to first byte, stream
//! throughput, and the process cost of holding those streams (RSS high-water, open FDs),
//! against an engine that answers chunked + keep-alive like a real one.
//! What it does NOT measure: prefill/decode cost, KV reuse, or anything the engine does
//! with the bytes. Those need a real engine; a mock can only prove the proxy is not the
//! bottleneck and did not change semantics.

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use nebula_common::upstream::{ReqwestTransport, UpstreamRequest, UpstreamTransport};
use tokio::sync::Semaphore;

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn proc_status_kb(key: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                (k == key).then(|| v.trim().trim_end_matches(" kB").parse().ok())?
            })
        })
        .unwrap_or(0)
}

fn open_fds() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .map(|d| d.count())
        .unwrap_or(0)
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx]
}

fn transport(name: &str) -> Arc<dyn UpstreamTransport> {
    match name {
        "reqwest" => Arc::new(ReqwestTransport::new(
            nebula_common::proxy_http_client().expect("reqwest client"),
        )),
        #[cfg(feature = "xlite")]
        "xlite" => Arc::new(nebula_common::XliteTransport::from_env()),
        other => panic!("unknown AB_TRANSPORT={other} (reqwest|xlite)"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "A/B harness: needs the mock upstream; run via scripts/ab_upstream_transport.sh"]
async fn ab_probe() {
    let name = std::env::var("AB_TRANSPORT").unwrap_or_else(|_| "reqwest".to_string());
    let base = std::env::var("AB_BASE_URL").expect("AB_BASE_URL");
    let requests = env_usize("AB_REQUESTS", 300);
    let concurrency = env_usize("AB_CONCURRENCY", 16);

    // Against a mock the model name is irrelevant; against a real engine it is not, and a
    // fixed `max_tokens` + `ignore_eos` is what makes the two runs comparable: every
    // request then streams exactly the same number of tokens.
    let model = std::env::var("AB_MODEL").unwrap_or_else(|_| "mock".to_string());
    let max_tokens = env_usize("AB_MAX_TOKENS", 0);
    let prompt = std::env::var("AB_PROMPT")
        .unwrap_or_else(|_| "Write one short paragraph about load balancing.".to_string());
    let mut payload = serde_json::json!({
        "model": model,
        "stream": true,
        "temperature": 0.0,
        "messages": [{"role": "user", "content": prompt}],
    });
    if max_tokens > 0 {
        payload["max_tokens"] = serde_json::json!(max_tokens);
        payload["ignore_eos"] = serde_json::json!(true);
    }
    let transport = transport(&name);
    let body = Bytes::from(serde_json::to_vec(&payload).expect("body"));
    let make_request = || {
        let mut request =
            UpstreamRequest::new(http::Method::POST, format!("{base}/v1/chat/completions"));
        request.headers.insert(
            http::header::CONTENT_TYPE,
            "application/json".parse().expect("content-type"),
        );
        request.body = Some(body.clone());
        request
    };

    // Warmup: dial pools and JIT the decode path so the two transports start even.
    for _ in 0..8 {
        let response = transport.execute(make_request()).await.expect("warmup");
        let _ = response.bytes().await.expect("warmup body");
    }
    let baseline_fds = open_fds();

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let started = Instant::now();
    let mut tasks = Vec::with_capacity(requests);
    for _ in 0..requests {
        let (transport, semaphore, request) = (
            Arc::clone(&transport),
            Arc::clone(&semaphore),
            make_request(),
        );
        tasks.push(tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await.expect("permit");
            let attempt = Instant::now();
            let mut response = transport.execute(request).await.expect("execute");
            let mut first: Option<f64> = None;
            let mut bytes = 0usize;
            loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        if first.is_none() {
                            first = Some(attempt.elapsed().as_secs_f64() * 1000.0);
                        }
                        bytes += chunk.len();
                    }
                    Ok(None) => break,
                    Err(e) => panic!("stream: {e}"),
                }
            }
            (
                first.expect("first chunk"),
                attempt.elapsed().as_secs_f64() * 1000.0,
                bytes,
            )
        }));
    }
    let mut ttfts = Vec::with_capacity(requests);
    let mut totals = Vec::with_capacity(requests);
    let mut payload = 0usize;
    for task in tasks {
        let (ttft, total, bytes) = task.await.expect("task");
        ttfts.push(ttft);
        totals.push(total);
        payload += bytes;
    }
    let wall = started.elapsed().as_secs_f64();

    ttfts.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    totals.sort_by(|a, b| a.partial_cmp(b).expect("finite"));

    let stats = transport.stats();
    let result = serde_json::json!({
        "transport": name,
        "requests": requests,
        "concurrency": concurrency,
        "ttft_ms_p50": pct(&ttfts, 0.50),
        "ttft_ms_p99": pct(&ttfts, 0.99),
        "total_ms_p50": pct(&totals, 0.50),
        "total_ms_p99": pct(&totals, 0.99),
        "wall_s": wall,
        "rps": requests as f64 / wall,
        "payload_bytes": payload,
        "rss_peak_mb": proc_status_kb("VmHWM") as f64 / 1024.0,
        "rss_end_mb": proc_status_kb("VmRSS") as f64 / 1024.0,
        "fds_peak_growth": open_fds().saturating_sub(baseline_fds),
        "pool_dials": stats.dials,
        "pool_connections": stats.connections,
        "pool_failed_dial": stats.failed_dial,
        "pool_redials": stats.redials,
    });
    println!("AB_RESULT {result}");
}
