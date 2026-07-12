use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;
use nebula_common::proxy_http_client;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub type EngineDeltaStream = Pin<Box<dyn Stream<Item = String> + Send>>;

pub trait EngineClient: Send + Sync {
    fn stream_text(&self, input: String) -> EngineDeltaStream;
}

#[derive(Debug, Clone)]
pub struct OpenAIEngineClient {
    base_url: String,
    model: String,
    http: reqwest::Client,
}

impl OpenAIEngineClient {
    pub fn new(base_url: String, model: String) -> Self {
        let http = proxy_http_client().unwrap_or_else(|e| {
            tracing::error!(error=%e, "failed to build reqwest client");
            std::process::exit(1);
        });

        Self {
            base_url,
            model,
            http,
        }
    }
}

impl EngineClient for OpenAIEngineClient {
    fn stream_text(&self, input: String) -> EngineDeltaStream {
        let (tx, rx) = mpsc::channel::<String>(256);
        let http = self.http.clone();
        let base = self.base_url.trim_end_matches('/').to_string();
        let model = self.model.clone();

        tokio::spawn(async move {
            let url = format!("{base}/v1/chat/completions");
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": input}],
                "stream": true,
                "max_tokens": 512,
            });

            let resp = match http.post(url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(error=%e, "engine request failed");
                    return;
                }
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let text = match resp.text().await {
                    Ok(text) => text,
                    Err(e) => {
                        tracing::warn!(error=%e, "failed to read engine error body");
                        String::new()
                    }
                };
                tracing::error!(%status, body=%text, "engine returned error");
                return;
            }

            let mut buf = String::new();
            let mut stream = resp.bytes_stream();
            while let Some(item) = stream.next().await {
                let chunk = match item {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!(error=%e, "engine stream read failed");
                        return;
                    }
                };

                let s = String::from_utf8_lossy(&chunk);
                buf.push_str(&s);

                while let Some(pos) = buf.find('\n') {
                    let mut line = buf[..pos].to_string();
                    buf.drain(..=pos);

                    line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    if !line.starts_with("data:") {
                        continue;
                    }

                    let data = line.trim_start_matches("data:").trim();
                    if data == "[DONE]" {
                        return;
                    }

                    let v: Value = match serde_json::from_str(data) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let delta = v
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c0| {
                            c0.get("delta")
                                .and_then(|d| d.get("content"))
                                .and_then(|t| t.as_str())
                                .or_else(|| c0.get("text").and_then(|t| t.as_str()))
                        })
                        .unwrap_or("");

                    if !delta.is_empty() {
                        let _ = tx.send(delta.to_string()).await;
                    }
                }
            }
        });

        Box::pin(ReceiverStream::new(rx))
    }
}
