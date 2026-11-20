//! GPU-free vLLM-compatible inference simulator (`llm-d-inference-sim` pattern)
//! for Router/Gateway scale-out regression testing and CI without GPU hardware.
//!
//! Exposes `/health`, `/v1/models`, `/metrics` (Prometheus vLLM gauges/counters),
//! `/v1/chat/completions`, `/v1/completions` (streaming SSE & non-streaming),
//! and `/sleep` + `/wake_up` endpoints.

use std::collections::{HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Parser, Debug)]
#[command(name = "inference_sim", about = "GPU-free vLLM-compatible simulator")]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,
    #[arg(long, default_value_t = 18000)]
    port: u16,
    #[arg(long, default_value = "Qwen/Qwen3-32B")]
    model: String,
    #[arg(long, default_value_t = 20)]
    ttft_ms: u64,
    #[arg(long, default_value_t = 5)]
    tpot_ms: u64,
    #[arg(long, default_value_t = 16)]
    default_max_tokens: usize,
}

struct SimState {
    model: String,
    ttft_ms: u64,
    tpot_ms: u64,
    default_max_tokens: usize,
    running: AtomicU64,
    sleeping: AtomicBool,
    prefix_queries: AtomicU64,
    prefix_hits: AtomicU64,
    seen_prefixes: Mutex<(HashSet<String>, VecDeque<String>)>,
}

impl SimState {
    fn check_and_record_prefix(&self, key: &str) -> bool {
        self.prefix_queries.fetch_add(1, Ordering::Relaxed);
        if key.is_empty() {
            return false;
        }
        let mut guard = self.seen_prefixes.lock().unwrap_or_else(|e| e.into_inner());
        if guard.0.contains(key) {
            self.prefix_hits.fetch_add(1, Ordering::Relaxed);
            return true;
        }
        if guard.1.len() >= 1024 {
            if let Some(old) = guard.1.pop_front() {
                guard.0.remove(&old);
            }
        }
        guard.0.insert(key.to_string());
        guard.1.push_back(key.to_string());
        false
    }
}

struct RunningGuard(Arc<SimState>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn health(State(st): State<Arc<SimState>>) -> Response {
    if st.sleeping.load(Ordering::Relaxed) {
        (StatusCode::SERVICE_UNAVAILABLE, "sleeping").into_response()
    } else {
        (StatusCode::OK, "ok").into_response()
    }
}

async fn models(State(st): State<Arc<SimState>>) -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": [{ "id": st.model, "object": "model", "owned_by": "nebula-sim" }]
    }))
}

async fn metrics(State(st): State<Arc<SimState>>) -> String {
    let running = st.running.load(Ordering::Relaxed);
    let kv_usage = ((running as f64) * 0.05).min(0.95);
    let queries = st.prefix_queries.load(Ordering::Relaxed);
    let hits = st.prefix_hits.load(Ordering::Relaxed);
    format!(
        "# TYPE vllm:num_requests_running gauge\n\
         vllm:num_requests_running {running}\n\
         # TYPE vllm:num_requests_waiting gauge\n\
         vllm:num_requests_waiting 0\n\
         # TYPE vllm:gpu_cache_usage_perc gauge\n\
         vllm:gpu_cache_usage_perc {kv_usage:.4}\n\
         # TYPE vllm:prefix_cache_queries_total counter\n\
         vllm:prefix_cache_queries_total {queries}\n\
         # TYPE vllm:prefix_cache_hits_total counter\n\
         vllm:prefix_cache_hits_total {hits}\n"
    )
}

async fn sleep_ep(State(st): State<Arc<SimState>>) -> StatusCode {
    st.sleeping.store(true, Ordering::Relaxed);
    StatusCode::OK
}

async fn wake_ep(State(st): State<Arc<SimState>>) -> StatusCode {
    st.sleeping.store(false, Ordering::Relaxed);
    StatusCode::OK
}

fn extract_prefix_key(payload: &Value) -> String {
    if let Some(p) = payload.get("prompt").and_then(|v| v.as_str()) {
        return p.chars().take(128).collect();
    }
    if let Some(msgs) = payload.get("messages").and_then(|v| v.as_array()) {
        if let Some(first) = msgs.first() {
            if let Some(c) = first.get("content").and_then(|v| v.as_str()) {
                return c.chars().take(128).collect();
            }
        }
    }
    String::new()
}

async fn completions(State(st): State<Arc<SimState>>, Json(payload): Json<Value>) -> Response {
    if st.sleeping.load(Ordering::Relaxed) {
        return (StatusCode::SERVICE_UNAVAILABLE, "engine sleeping").into_response();
    }
    st.running.fetch_add(1, Ordering::Relaxed);
    let guard = RunningGuard(st.clone());

    let stream = payload
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let max_tokens = payload
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(1, 256) as usize)
        .unwrap_or(st.default_max_tokens);
    let prefix_hit = st.check_and_record_prefix(&extract_prefix_key(&payload));
    let ttft_ms = if prefix_hit {
        (st.ttft_ms / 3).max(1)
    } else {
        st.ttft_ms
    };
    let model = payload
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&st.model)
        .to_string();

    if stream {
        let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(32);
        let tpot_ms = st.tpot_ms;
        tokio::spawn(async move {
            let _guard = guard;
            tokio::time::sleep(Duration::from_millis(ttft_ms)).await;
            for i in 0..max_tokens {
                let chunk = json!({
                    "id": "sim-cmpl",
                    "object": "chat.completion.chunk",
                    "model": model,
                    "choices": [{
                        "index": 0,
                        "delta": { "content": if i == 0 { "sim" } else { " tok" } },
                        "finish_reason": if i + 1 == max_tokens { json!("stop") } else { Value::Null }
                    }]
                });
                if tx
                    .send(Ok(Event::default().data(chunk.to_string())))
                    .await
                    .is_err()
                {
                    return;
                }
                if i + 1 < max_tokens && tpot_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(tpot_ms)).await;
                }
            }
            let _ = tx.send(Ok(Event::default().data("[DONE]"))).await;
        });
        return Sse::new(ReceiverStream::new(rx)).into_response();
    }

    tokio::time::sleep(Duration::from_millis(
        ttft_ms + st.tpot_ms * (max_tokens.saturating_sub(1) as u64),
    ))
    .await;
    drop(guard);
    Json(json!({
        "id": "sim-cmpl",
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "simulated response" },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 16,
            "completion_tokens": max_tokens,
            "total_tokens": 16 + max_tokens
        }
    }))
    .into_response()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let st = Arc::new(SimState {
        model: args.model,
        ttft_ms: args.ttft_ms,
        tpot_ms: args.tpot_ms,
        default_max_tokens: args.default_max_tokens,
        running: AtomicU64::new(0),
        sleeping: AtomicBool::new(false),
        prefix_queries: AtomicU64::new(0),
        prefix_hits: AtomicU64::new(0),
        seen_prefixes: Mutex::new((HashSet::new(), VecDeque::new())),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/metrics", get(metrics))
        .route("/sleep", post(sleep_ep))
        .route("/wake_up", post(wake_ep))
        .route("/v1/chat/completions", post(completions))
        .route("/v1/completions", post(completions))
        .with_state(st);
    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulator_tracks_prefix_cache_hits() {
        let st = SimState {
            model: "m".into(),
            ttft_ms: 30,
            tpot_ms: 5,
            default_max_tokens: 4,
            running: AtomicU64::new(0),
            sleeping: AtomicBool::new(false),
            prefix_queries: AtomicU64::new(0),
            prefix_hits: AtomicU64::new(0),
            seen_prefixes: Mutex::new((HashSet::new(), VecDeque::new())),
        };
        assert!(!st.check_and_record_prefix("system prompt A"));
        assert!(st.check_and_record_prefix("system prompt A"));
        assert_eq!(st.prefix_queries.load(Ordering::Relaxed), 2);
        assert_eq!(st.prefix_hits.load(Ordering::Relaxed), 1);
    }
}
