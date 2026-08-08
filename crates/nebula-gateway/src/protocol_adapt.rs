//! Thin UniGateway protocol adapters for Nebula Gateway.
//!
//! Uses `unigateway-sdk` (`conversion` feature) for request parsing only.
//! Cluster routing stays on Nebula Router — never call UniGatewayEngine pool
//! selection here.

use serde_json::{json, Value};
use unigateway_sdk::core::{ContentBlock, MessageRole, ProxyChatRequest, ProxyResponsesRequest};
use unigateway_sdk::protocol::{
    anthropic_payload_to_chat_request, openai_payload_to_responses_request,
};

/// Convert a parsed `ProxyChatRequest` into an OpenAI-compatible chat JSON body
/// suitable for `POST {router}/v1/chat/completions`.
pub fn proxy_chat_to_openai_json(req: &ProxyChatRequest) -> Value {
    let mut messages = Vec::new();

    if let Some(system) = &req.system {
        match system {
            Value::String(s) if !s.is_empty() => {
                messages.push(json!({"role": "system", "content": s}));
            }
            Value::Array(parts) => {
                let text = parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    messages.push(json!({"role": "system", "content": text}));
                }
            }
            _ => {}
        }
    }

    for msg in &req.messages {
        let role = match msg.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        let content = blocks_to_openai_content(&msg.content);
        messages.push(json!({"role": role, "content": content}));
    }

    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": req.stream,
    });

    let obj = body.as_object_mut().expect("json object");
    if let Some(t) = req.temperature {
        obj.insert("temperature".into(), json!(t));
    }
    if let Some(p) = req.top_p {
        obj.insert("top_p".into(), json!(p));
    }
    if let Some(k) = req.top_k {
        obj.insert("top_k".into(), json!(k));
    }
    if let Some(m) = req.max_tokens {
        obj.insert("max_tokens".into(), json!(m));
    }
    if let Some(stop) = &req.stop_sequences {
        obj.insert("stop".into(), stop.clone());
    }
    if let Some(tools) = &req.tools {
        obj.insert("tools".into(), tools.clone());
    }
    if let Some(tc) = &req.tool_choice {
        obj.insert("tool_choice".into(), tc.clone());
    }

    body
}

fn blocks_to_openai_content(blocks: &[ContentBlock]) -> Value {
    if blocks.len() == 1 {
        if let ContentBlock::Text { text } = &blocks[0] {
            return Value::String(text.clone());
        }
    }
    let parts: Vec<Value> = blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(json!({"type": "text", "text": text})),
            ContentBlock::Image { source, detail } => {
                let mut image_url = source.clone();
                if let Some(d) = detail {
                    if let Some(obj) = image_url.as_object_mut() {
                        obj.insert("detail".into(), json!(d));
                    }
                }
                Some(json!({"type": "image_url", "image_url": image_url}))
            }
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        Value::String(String::new())
    } else {
        Value::Array(parts)
    }
}

/// Parse Anthropic Messages API JSON → OpenAI chat body for Router.
pub fn anthropic_json_to_openai_chat(payload: &Value) -> anyhow::Result<Value> {
    let chat = anthropic_payload_to_chat_request(payload, "unknown")?;
    Ok(proxy_chat_to_openai_json(&chat))
}

/// Parse OpenAI Responses API JSON via UniGateway; return model + chat body for Router.
pub fn responses_json_to_openai_chat(payload: &Value) -> anyhow::Result<(ProxyResponsesRequest, Value)> {
    let resp = openai_payload_to_responses_request(payload, "unknown")?;
    let user_text = responses_input_text(&resp.input);
    let mut messages = Vec::new();
    if let Some(s) = &resp.instructions {
        if !s.is_empty() {
            messages.push(json!({"role": "system", "content": s}));
        }
    }
    messages.push(json!({"role": "user", "content": user_text}));

    let mut body = json!({
        "model": resp.model,
        "messages": messages,
        "stream": resp.stream,
    });
    let obj = body.as_object_mut().expect("json object");
    if let Some(t) = resp.temperature {
        obj.insert("temperature".into(), json!(t));
    }
    if let Some(p) = resp.top_p {
        obj.insert("top_p".into(), json!(p));
    }
    if let Some(m) = resp.max_output_tokens {
        obj.insert("max_tokens".into(), json!(m));
    }
    Ok((resp, body))
}

fn responses_input_text(input: &Option<Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    match input {
        Value::String(s) => s.clone(),
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(s) = item.as_str() {
                    parts.push(s.to_string());
                    continue;
                }
                if let Some(content) = item.get("content").and_then(|v| v.as_str()) {
                    parts.push(content.to_string());
                    continue;
                }
                if let Some(arr) = item.get("content").and_then(|v| v.as_array()) {
                    for p in arr {
                        if p.get("type").and_then(|t| t.as_str()) == Some("input_text")
                            || p.get("type").and_then(|t| t.as_str()) == Some("text")
                        {
                            if let Some(text) = p.get("text").and_then(|t| t.as_str()) {
                                parts.push(text.to_string());
                            }
                        }
                    }
                }
            }
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Map a completed OpenAI chat completion JSON to Anthropic Messages API shape.
pub fn openai_chat_json_to_anthropic(openai: &Value, requested_model: &str) -> Value {
    let text = openai
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let id = openai
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("msg_nebula");
    let stop = openai
        .pointer("/choices/0/finish_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("end_turn");
    let stop_reason = match stop {
        "length" => "max_tokens",
        "tool_calls" => "tool_use",
        _ => "end_turn",
    };
    let usage = openai.get("usage").cloned().unwrap_or(json!({}));
    let input_tokens = usage.get("prompt_tokens").cloned().unwrap_or(json!(0));
    let output_tokens = usage.get("completion_tokens").cloned().unwrap_or(json!(0));

    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": requested_model,
        "content": [{"type": "text", "text": text}],
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    })
}

/// Extract text delta from an OpenAI chat SSE `data:` JSON payload.
pub fn openai_sse_content_delta(data: &str) -> Option<String> {
    if data == "[DONE]" {
        return None;
    }
    let v: Value = serde_json::from_str(data).ok()?;
    v.pointer("/choices/0/delta/content")
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_messages_convert_to_openai_chat() {
        let payload = json!({
            "model": "claude-test",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let openai = anthropic_json_to_openai_chat(&payload).expect("parse");
        assert_eq!(openai["model"], "claude-test");
        assert_eq!(openai["messages"][0]["role"], "user");
        assert_eq!(openai["messages"][0]["content"], "hello");
        assert_eq!(openai["max_tokens"], 64);
    }

    #[test]
    fn responses_payload_converts_to_chat() {
        let payload = json!({
            "model": "m1",
            "input": "ping",
            "stream": false
        });
        let (resp, chat) = responses_json_to_openai_chat(&payload).expect("parse");
        assert_eq!(resp.model, "m1");
        assert_eq!(chat["model"], "m1");
        assert_eq!(chat["messages"].as_array().unwrap().last().unwrap()["content"], "ping");
    }

    #[test]
    fn openai_to_anthropic_maps_content() {
        let openai = json!({
            "id": "chatcmpl-1",
            "choices": [{"message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        });
        let anth = openai_chat_json_to_anthropic(&openai, "claude-test");
        assert_eq!(anth["content"][0]["text"], "hi");
        assert_eq!(anth["model"], "claude-test");
        assert_eq!(anth["stop_reason"], "end_turn");
    }
}
