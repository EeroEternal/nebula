//! Thin UniGateway protocol adapters for Nebula Gateway.
//!
//! Uses `unigateway-sdk` (`conversion` feature) for request parsing only.
//! Cluster routing stays on Nebula Router — never call UniGatewayEngine pool
//! selection here.

use serde_json::{json, Value};
use unigateway_sdk::core::{ContentBlock, Message, MessageRole, ProxyChatRequest, ProxyResponsesRequest};
use unigateway_sdk::protocol::{
    anthropic_payload_to_chat_request, openai_payload_to_responses_request,
};

/// Convert a parsed `ProxyChatRequest` into an OpenAI-compatible chat JSON body
/// suitable for `POST {router}/v1/chat/completions`.
pub fn proxy_chat_to_openai_json(req: &ProxyChatRequest) -> Value {
    let messages = if let Some(raw) = &req.raw_messages {
        raw.clone()
    } else {
        Value::Array(structured_messages_to_openai(&req.system, &req.messages))
    };

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
    for (k, v) in &req.extra {
        // Do not clobber core fields already set.
        if !obj.contains_key(k) {
            obj.insert(k.clone(), v.clone());
        }
    }

    body
}

fn structured_messages_to_openai(system: &Option<Value>, messages: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();

    if let Some(system) = system {
        match system {
            Value::String(s) if !s.is_empty() => {
                out.push(json!({"role": "system", "content": s}));
            }
            Value::Array(parts) => {
                let text = parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    out.push(json!({"role": "system", "content": text}));
                }
            }
            _ => {}
        }
    }

    for msg in messages {
        match msg.role {
            MessageRole::Tool => {
                for block in &msg.content {
                    if let ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                    } = block
                    {
                        let content_val = match content {
                            Value::String(s) => Value::String(s.clone()),
                            other => other.clone(),
                        };
                        out.push(json!({
                            "role": "tool",
                            "tool_call_id": tool_use_id,
                            "content": content_val,
                        }));
                    }
                }
            }
            MessageRole::Assistant => {
                let mut text_parts: Vec<String> = Vec::new();
                let mut multimodal: Vec<Value> = Vec::new();
                let mut tool_calls: Vec<Value> = Vec::new();
                for block in &msg.content {
                    match block {
                        ContentBlock::Text { text } => {
                            text_parts.push(text.clone());
                            multimodal.push(json!({"type": "text", "text": text}));
                        }
                        ContentBlock::Image { source, detail } => {
                            let mut image_url = source.clone();
                            if let Some(d) = detail {
                                if let Some(obj) = image_url.as_object_mut() {
                                    obj.insert("detail".into(), json!(d));
                                }
                            }
                            multimodal.push(json!({"type": "image_url", "image_url": image_url}));
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            let arguments = match input {
                                Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {"name": name, "arguments": arguments}
                            }));
                        }
                        ContentBlock::Thinking { .. } | ContentBlock::ToolResult { .. } => {}
                    }
                }
                let mut m = json!({"role": "assistant"});
                let obj = m.as_object_mut().unwrap();
                if multimodal.len() > 1 || multimodal.iter().any(|p| p.get("type") != Some(&json!("text"))) {
                    obj.insert("content".into(), Value::Array(multimodal));
                } else if !text_parts.is_empty() {
                    obj.insert("content".into(), Value::String(text_parts.join("\n")));
                } else if tool_calls.is_empty() {
                    obj.insert("content".into(), Value::String(String::new()));
                } else {
                    obj.insert("content".into(), Value::Null);
                }
                if !tool_calls.is_empty() {
                    obj.insert("tool_calls".into(), Value::Array(tool_calls));
                }
                out.push(m);
            }
            role => {
                let role_str = match role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                };
                let content = blocks_to_openai_content(&msg.content);
                out.push(json!({"role": role_str, "content": content}));
            }
        }
    }

    out
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
            // ToolUse/ToolResult handled at message level for assistant/tool roles.
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
pub fn responses_json_to_openai_chat(
    payload: &Value,
) -> anyhow::Result<(ProxyResponsesRequest, Value)> {
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
    // Preserve tools / tool_choice from Responses inbound (C5).
    if let Some(tools) = &resp.tools {
        obj.insert("tools".into(), tools.clone());
    } else if let Some(tools) = payload.get("tools") {
        obj.insert("tools".into(), tools.clone());
    }
    if let Some(tc) = &resp.tool_choice {
        obj.insert("tool_choice".into(), tc.clone());
    } else if let Some(tc) = payload.get("tool_choice") {
        obj.insert("tool_choice".into(), tc.clone());
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

    let message = openai.pointer("/choices/0/message").cloned().unwrap_or(json!({}));
    let mut content_blocks: Vec<Value> = Vec::new();

    if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            content_blocks.push(json!({"type": "text", "text": text}));
        }
    }

    if let Some(tcs) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tcs {
            let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let name = tc
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args_str = tc
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            content_blocks.push(json!({
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": input,
            }));
        }
    }

    if content_blocks.is_empty() {
        content_blocks.push(json!({"type": "text", "text": ""}));
    }

    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": requested_model,
        "content": content_blocks,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    })
}

/// One logical piece from an OpenAI chat completions SSE `data:` JSON payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiStreamChunk {
    Done,
    Text(String),
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    },
    FinishReason(String),
    Ignored,
}

/// Parse an OpenAI chat SSE data payload into zero or more logical chunks.
pub fn parse_openai_sse_chunk(data: &str) -> Vec<OpenAiStreamChunk> {
    if data == "[DONE]" {
        return vec![OpenAiStreamChunk::Done];
    }
    let Ok(v) = serde_json::from_str::<Value>(data) else {
        return vec![OpenAiStreamChunk::Ignored];
    };
    let Some(choice) = v.pointer("/choices/0") else {
        return vec![OpenAiStreamChunk::Ignored];
    };
    let mut out = Vec::new();
    if let Some(content) = choice
        .pointer("/delta/content")
        .and_then(|c| c.as_str())
        .filter(|s| !s.is_empty())
    {
        out.push(OpenAiStreamChunk::Text(content.to_string()));
    }
    if let Some(tcs) = choice
        .pointer("/delta/tool_calls")
        .and_then(|t| t.as_array())
    {
        for (fallback_idx, tc) in tcs.iter().enumerate() {
            let index = tc
                .get("index")
                .and_then(|i| i.as_u64())
                .map(|i| i as usize)
                .unwrap_or(fallback_idx);
            let id = tc
                .get("id")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let name = tc
                .pointer("/function/name")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let arguments = tc
                .pointer("/function/arguments")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            if id.is_none() && name.is_none() && arguments.as_ref().map(|s| s.is_empty()).unwrap_or(true)
            {
                continue;
            }
            out.push(OpenAiStreamChunk::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            });
        }
    }
    if let Some(fr) = choice
        .get("finish_reason")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty() && *s != "null")
    {
        out.push(OpenAiStreamChunk::FinishReason(fr.to_string()));
    }
    if out.is_empty() {
        out.push(OpenAiStreamChunk::Ignored);
    }
    out
}

/// Extract text delta from an OpenAI chat SSE `data:` JSON payload.
pub fn openai_sse_content_delta(data: &str) -> Option<String> {
    parse_openai_sse_chunk(data).into_iter().find_map(|c| match c {
        OpenAiStreamChunk::Text(t) => Some(t),
        _ => None,
    })
}

/// Maps OpenAI chat SSE chunks into Anthropic Messages SSE event payloads.
#[derive(Debug)]
pub struct AnthropicSseMapper {
    /// Text block at index 0 starts open (handlers emit content_block_start for text).
    text_open: bool,
    next_block_index: usize,
    /// OpenAI tool_call index → (anthropic block index, started, id, name).
    tools: std::collections::BTreeMap<usize, (usize, bool, String, String)>,
    saw_tool_calls: bool,
    finish_reason: Option<String>,
}

impl AnthropicSseMapper {
    pub fn new() -> Self {
        Self {
            text_open: true,
            next_block_index: 1,
            tools: std::collections::BTreeMap::new(),
            saw_tool_calls: false,
            finish_reason: None,
        }
    }

    /// Returns `(event_name, data_json)` pairs to send as Anthropic SSE.
    pub fn push(&mut self, chunk: OpenAiStreamChunk) -> Vec<(String, Value)> {
        match chunk {
            OpenAiStreamChunk::Ignored => Vec::new(),
            OpenAiStreamChunk::Text(text) => {
                if !self.text_open {
                    // Late text after tools — ignore (rare).
                    return Vec::new();
                }
                vec![(
                    "content_block_delta".into(),
                    json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {"type": "text_delta", "text": text}
                    }),
                )]
            }
            OpenAiStreamChunk::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            } => {
                let mut evs = Vec::new();
                if self.text_open {
                    evs.push((
                        "content_block_stop".into(),
                        json!({"type": "content_block_stop", "index": 0}),
                    ));
                    self.text_open = false;
                }
                self.saw_tool_calls = true;
                let entry = self.tools.entry(index).or_insert_with(|| {
                    let ai = self.next_block_index;
                    self.next_block_index += 1;
                    (ai, false, String::new(), String::new())
                });
                if let Some(id) = id {
                    entry.2 = id;
                }
                if let Some(name) = name {
                    entry.3 = name;
                }
                if !entry.1 {
                    entry.1 = true;
                    let anth_index = entry.0;
                    let call_id = if entry.2.is_empty() {
                        format!("toolu_{index}")
                    } else {
                        entry.2.clone()
                    };
                    let call_name = entry.3.clone();
                    evs.push((
                        "content_block_start".into(),
                        json!({
                            "type": "content_block_start",
                            "index": anth_index,
                            "content_block": {
                                "type": "tool_use",
                                "id": call_id,
                                "name": call_name,
                                "input": {}
                            }
                        }),
                    ));
                }
                if let Some(args) = arguments {
                    if !args.is_empty() {
                        let anth_index = entry.0;
                        evs.push((
                            "content_block_delta".into(),
                            json!({
                                "type": "content_block_delta",
                                "index": anth_index,
                                "delta": {"type": "input_json_delta", "partial_json": args}
                            }),
                        ));
                    }
                }
                evs
            }
            OpenAiStreamChunk::FinishReason(reason) => {
                self.finish_reason = Some(reason);
                Vec::new()
            }
            OpenAiStreamChunk::Done => self.finish(),
        }
    }

    fn finish(&mut self) -> Vec<(String, Value)> {
        let mut evs = Vec::new();
        if self.text_open {
            evs.push((
                "content_block_stop".into(),
                json!({"type": "content_block_stop", "index": 0}),
            ));
            self.text_open = false;
        }
        for (_, (anth_index, started, _, _)) in &self.tools {
            if *started {
                evs.push((
                    "content_block_stop".into(),
                    json!({"type": "content_block_stop", "index": anth_index}),
                ));
            }
        }
        let stop_reason = match self.finish_reason.as_deref() {
            Some("tool_calls") => "tool_use",
            Some("length") => "max_tokens",
            _ if self.saw_tool_calls => "tool_use",
            _ => "end_turn",
        };
        evs.push((
            "message_delta".into(),
            json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                "usage": {"output_tokens": 0}
            }),
        ));
        evs.push((
            "message_stop".into(),
            json!({"type": "message_stop"}),
        ));
        evs
    }
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
    fn anthropic_tools_preserved() {
        let payload = json!({
            "model": "claude-test",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hello"}],
            "tools": [{
                "name": "get_weather",
                "description": "weather",
                "input_schema": {"type": "object", "properties": {}}
            }]
        });
        let openai = anthropic_json_to_openai_chat(&payload).expect("parse");
        assert!(openai.get("tools").is_some(), "tools must not be dropped");
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
        assert_eq!(
            chat["messages"].as_array().unwrap().last().unwrap()["content"],
            "ping"
        );
    }

    #[test]
    fn responses_tools_preserved() {
        let payload = json!({
            "model": "m1",
            "input": "ping",
            "tools": [{"type": "function", "function": {"name": "f", "parameters": {}}}]
        });
        let (_resp, chat) = responses_json_to_openai_chat(&payload).expect("parse");
        assert!(chat.get("tools").is_some());
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

    #[test]
    fn openai_to_anthropic_maps_tool_calls() {
        let openai = json!({
            "id": "chatcmpl-1",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "get_weather", "arguments": "{\"city\":\"SF\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}
        });
        let anth = openai_chat_json_to_anthropic(&openai, "claude-test");
        assert_eq!(anth["stop_reason"], "tool_use");
        assert_eq!(anth["content"][0]["type"], "tool_use");
        assert_eq!(anth["content"][0]["name"], "get_weather");
        assert_eq!(anth["content"][0]["input"]["city"], "SF");
    }

    #[test]
    fn parse_sse_text_and_tool_call_deltas() {
        let text = parse_openai_sse_chunk(
            r#"{"choices":[{"delta":{"content":"hi"},"finish_reason":null}]}"#,
        );
        assert_eq!(text, vec![OpenAiStreamChunk::Text("hi".into())]);
        assert_eq!(
            openai_sse_content_delta(
                r#"{"choices":[{"delta":{"content":"hi"},"finish_reason":null}]}"#
            )
            .as_deref(),
            Some("hi")
        );

        let tc = parse_openai_sse_chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"fn","arguments":"{"}}]}}]}"#,
        );
        assert!(matches!(
            &tc[0],
            OpenAiStreamChunk::ToolCallDelta {
                index: 0,
                id: Some(_),
                name: Some(_),
                arguments: Some(_)
            }
        ));

        let done = parse_openai_sse_chunk("[DONE]");
        assert_eq!(done, vec![OpenAiStreamChunk::Done]);
    }

    #[test]
    fn anthropic_mapper_emits_tool_use_stream() {
        let mut m = AnthropicSseMapper::new();
        let evs = m.push(OpenAiStreamChunk::ToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("get_weather".into()),
            arguments: Some("{\"city\":".into()),
        });
        assert!(evs.iter().any(|(e, _)| e == "content_block_stop"));
        assert!(evs.iter().any(|(e, v)| {
            e == "content_block_start" && v["content_block"]["type"] == "tool_use"
        }));
        assert!(evs.iter().any(|(e, v)| {
            e == "content_block_delta" && v["delta"]["type"] == "input_json_delta"
        }));
        let fin = m.push(OpenAiStreamChunk::FinishReason("tool_calls".into()));
        assert!(fin.is_empty());
        let done = m.push(OpenAiStreamChunk::Done);
        assert_eq!(done.last().unwrap().0, "message_stop");
        let delta = done
            .iter()
            .find(|(e, _)| e == "message_delta")
            .unwrap();
        assert_eq!(delta.1["delta"]["stop_reason"], "tool_use");
    }
}
