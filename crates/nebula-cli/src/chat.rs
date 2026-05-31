use std::io::{self, BufRead, Write};

use anyhow::Result;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::Value;

use crate::client::auth;

pub async fn run_chat(
    client: &Client,
    base_url: &str,
    token: Option<&String>,
    model: Option<String>,
    system: Option<String>,
    message: Option<String>,
    max_tokens: u32,
) -> Result<()> {
    let model = match model {
        Some(m) => m,
        None => resolve_model(client, base_url, token).await?,
    };

    let mut messages: Vec<Value> = Vec::new();
    if let Some(sys) = &system {
        messages.push(serde_json::json!({"role": "system", "content": sys}));
    }

    if let Some(msg) = message {
        messages.push(serde_json::json!({"role": "user", "content": msg}));
        let reply = send_streaming(client, base_url, token, &model, &messages, max_tokens).await?;
        messages.push(serde_json::json!({"role": "assistant", "content": reply}));
        println!();
        return Ok(());
    }

    println!("Nebula Chat (model: {model})  — type /quit to exit");
    println!();

    let stdin = io::stdin();
    loop {
        print!(">>> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "/quit" || line == "/exit" {
            break;
        }
        if line == "/clear" {
            messages.retain(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"));
            println!("(history cleared)");
            continue;
        }

        messages.push(serde_json::json!({"role": "user", "content": line}));
        let reply = send_streaming(client, base_url, token, &model, &messages, max_tokens).await?;
        messages.push(serde_json::json!({"role": "assistant", "content": reply}));
        println!();
        println!();
    }

    Ok(())
}

async fn resolve_model(client: &Client, base_url: &str, token: Option<&String>) -> Result<String> {
    let url = format!("{base_url}/v1/models");
    let resp = auth(client.get(&url), token).send().await?;
    let body: Value = resp.json().await?;
    let models = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    if models.is_empty() {
        anyhow::bail!("no models available — specify --model explicitly");
    }
    let id = models[0]
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    Ok(id.to_string())
}

async fn send_streaming(
    client: &Client,
    base_url: &str,
    token: Option<&String>,
    model: &str,
    messages: &[Value],
    max_tokens: u32,
) -> Result<String> {
    let url = format!("{base_url}/v1/chat/completions");
    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "max_tokens": max_tokens,
    });

    let resp = auth(client.post(&url), token).json(&body).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("chat request failed ({status}): {text}");
    }

    let mut full_reply = String::new();
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let s = String::from_utf8_lossy(&chunk);
        buf.push_str(&s);

        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);

            if line.is_empty() || !line.starts_with("data:") {
                continue;
            }
            let data = line.trim_start_matches("data:").trim();
            if data == "[DONE]" {
                return Ok(full_reply);
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
                })
                .unwrap_or("");

            if !delta.is_empty() {
                print!("{delta}");
                io::stdout().flush()?;
                full_reply.push_str(delta);
            }
        }
    }

    Ok(full_reply)
}
