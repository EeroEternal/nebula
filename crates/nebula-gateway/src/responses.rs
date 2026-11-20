use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

const DEFAULT_RESPONSE_STATE_TTL_SECS: u64 = 3600;
const MAX_RESPONSE_STATE_ENTRIES: usize = 4096;

#[derive(Clone)]
struct StoredResponseState {
    messages: Vec<Value>,
    root_session_id: String,
    created_at: Instant,
}

/// Gateway-side bounded state store for `/v1/responses` `previous_response_id`
/// continuation. Keeps `nebula-router` completely stateless by rehydrating
/// conversation history at the Gateway boundary before forwarding.
pub struct ResponseStateStore {
    ttl: Duration,
    entries: Mutex<HashMap<(Option<String>, String), StoredResponseState>>,
}

impl Default for ResponseStateStore {
    fn default() -> Self {
        let ttl_secs = std::env::var("NEBULA_RESPONSE_STATE_TTL_SECS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_RESPONSE_STATE_TTL_SECS);
        Self {
            ttl: Duration::from_secs(ttl_secs),
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl ResponseStateStore {
    pub fn rehydrate_chat_messages(
        &self,
        tenant_id: Option<&str>,
        prev_response_id: &str,
        new_messages: &[Value],
    ) -> Option<(Vec<Value>, String)> {
        let key = (tenant_id.map(str::to_string), prev_response_id.to_string());
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let entry = map.get(&key)?;
        if entry.created_at.elapsed() >= self.ttl {
            map.remove(&key);
            return None;
        }
        let mut merged = entry.messages.clone();
        // Skip duplicate leading system instructions if the new turn re-supplies one
        let start_idx = if !merged.is_empty()
            && merged[0].get("role").and_then(|r| r.as_str()) == Some("system")
            && new_messages
                .first()
                .and_then(|m| m.get("role"))
                .and_then(|r| r.as_str())
                == Some("system")
        {
            1
        } else {
            0
        };
        let root_sid = entry.root_session_id.clone();
        merged.extend_from_slice(&new_messages[start_idx..]);
        Some((merged, root_sid))
    }

    pub fn save_response(
        &self,
        tenant_id: Option<&str>,
        response_id: &str,
        root_session_id: Option<&str>,
        full_messages: Vec<Value>,
    ) {
        let now = Instant::now();
        let mut map = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= MAX_RESPONSE_STATE_ENTRIES {
            map.retain(|_, v| now.duration_since(v.created_at) < self.ttl);
            if map.len() >= MAX_RESPONSE_STATE_ENTRIES {
                if let Some(oldest) = map
                    .iter()
                    .min_by_key(|(_, v)| v.created_at)
                    .map(|(k, _)| k.clone())
                {
                    map.remove(&oldest);
                }
            }
        }
        let key = (tenant_id.map(str::to_string), response_id.to_string());
        let session = root_session_id
            .map(str::to_string)
            .unwrap_or_else(|| response_id.to_string());
        map.insert(
            key,
            StoredResponseState {
                messages: full_messages,
                root_session_id: session,
                created_at: now,
            },
        );
    }
}

/// Extract an OpenAI-chat-style assistant message from a `/v1/responses` output array
/// so it can be appended to the stored conversation history.
pub fn assistant_message_from_response_output(output: &[Value]) -> Option<Value> {
    let mut text_parts = Vec::new();
    for item in output {
        if item.get("type").and_then(|v| v.as_str()) == Some("message") {
            if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                for part in content {
                    if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                        text_parts.push(t);
                    }
                }
            }
        }
    }
    if text_parts.is_empty() {
        return None;
    }
    Some(json!({
        "role": "assistant",
        "content": text_parts.join("")
    }))
}

#[derive(Debug, Deserialize)]
pub struct CreateResponseRequest {
    pub model: Option<String>,
    pub input: Option<Value>,
    #[allow(dead_code)]
    pub instructions: Option<Value>,
    #[allow(dead_code)]
    // accepted for OpenAI Responses compatibility; streaming is handled upstream
    pub stream: Option<bool>,
}

impl CreateResponseRequest {
    pub fn extract_input_text(&self) -> String {
        if let Some(Value::String(s)) = self.input.as_ref() {
            return s.clone();
        }

        if let Some(Value::Array(items)) = self.input.as_ref() {
            let mut parts = Vec::new();
            for item in items {
                if let Some(content_str) = item.get("content").and_then(|v| v.as_str()) {
                    parts.push(content_str.to_string());
                    continue;
                }

                if let Some(content_parts) = item.get("content").and_then(|v| v.as_array()) {
                    for p in content_parts {
                        if p.get("type").and_then(|v| v.as_str()) == Some("input_text") {
                            if let Some(text) = p.get("text").and_then(|v| v.as_str()) {
                                parts.push(text.to_string());
                            }
                        }
                    }
                }
            }
            if !parts.is_empty() {
                return parts.join("\n");
            }
        }

        "".to_string()
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn estimate_tokens(s: &str) -> u32 {
    if s.is_empty() {
        return 0;
    }
    ((s.len() as f64) / 4.0).ceil() as u32
}

pub struct BuiltResponse {
    pub response_id: String,
    pub message_id: String,
    pub created: u64,
    pub model: String,
    pub text: String,
    pub usage: Value,
}

struct PendingFunctionCall {
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    started: bool,
}

pub struct ResponseStreamBuilder {
    response_id: String,
    message_id: String,
    created: u64,
    model: String,
    seq: u64,
    deltas: Vec<String>,
    input_tokens: u32,
    /// OpenAI tool_call index → pending function call.
    tool_calls: HashMap<usize, PendingFunctionCall>,
    next_output_index: usize,
}

impl ResponseStreamBuilder {
    pub fn new(req: &CreateResponseRequest) -> Self {
        let model = req.model.clone().unwrap_or_else(|| "unknown".to_string());
        let created = now_unix_seconds();
        let input_text = req.extract_input_text();
        let input_tokens = estimate_tokens(&input_text);

        Self {
            response_id: format!("resp_{}", Uuid::new_v4()),
            message_id: format!("msg_{}", Uuid::new_v4()),
            created,
            model,
            seq: 0,
            deltas: Vec::new(),
            input_tokens,
            tool_calls: HashMap::new(),
            // output index 0 reserved for assistant message text item in completed payload
            next_output_index: 1,
        }
    }

    pub fn created_event(&mut self) -> Value {
        let created_response = json!({
            "id": self.response_id,
            "object": "response",
            "created": self.created,
            "model": self.model,
            "status": "in_progress"
        });

        let ev = json!({
            "type": "response.created",
            "sequence_number": self.seq,
            "response_id": self.response_id,
            "response": created_response
        });

        self.seq += 1;
        ev
    }

    pub fn push_delta(&mut self, delta: String) -> Value {
        self.deltas.push(delta.clone());
        let ev = json!({
            "type": "response.output_text.delta",
            "sequence_number": self.seq,
            "response_id": self.response_id,
            "delta": delta,
            "item_id": self.message_id,
            "output_index": 0,
            "content_index": 0
        });
        self.seq += 1;
        ev
    }

    /// Ingest an OpenAI tool_call delta; returns zero or more Responses SSE events.
    pub fn push_tool_call_delta(
        &mut self,
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    ) -> Vec<Value> {
        let mut evs = Vec::new();
        if !self.tool_calls.contains_key(&index) {
            self.next_output_index += 1;
            self.tool_calls.insert(
                index,
                PendingFunctionCall {
                    item_id: format!("fc_{}", Uuid::new_v4()),
                    call_id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                    started: false,
                },
            );
        }
        let entry = self.tool_calls.get_mut(&index).expect("just inserted");
        if let Some(id) = id {
            entry.call_id = id;
        }
        if let Some(name) = name {
            entry.name = name;
        }
        if let Some(ref args) = arguments {
            entry.arguments.push_str(args);
        }

        let need_start = !entry.started;
        if need_start {
            entry.started = true;
            if entry.call_id.is_empty() {
                entry.call_id = format!("call_{index}");
            }
        }
        let item_id = entry.item_id.clone();
        let call_id = entry.call_id.clone();
        let call_name = entry.name.clone();
        let output_index = index + 1;

        if need_start {
            let ev = json!({
                "type": "response.output_item.added",
                "sequence_number": self.seq,
                "response_id": self.response_id,
                "output_index": output_index,
                "item": {
                    "id": item_id,
                    "type": "function_call",
                    "status": "in_progress",
                    "call_id": call_id,
                    "name": call_name,
                    "arguments": ""
                }
            });
            self.seq += 1;
            evs.push(ev);
        }

        if let Some(args) = arguments {
            if !args.is_empty() {
                let entry = self.tool_calls.get(&index).expect("exists");
                let ev = json!({
                    "type": "response.function_call_arguments.delta",
                    "sequence_number": self.seq,
                    "response_id": self.response_id,
                    "item_id": entry.item_id,
                    "output_index": output_index,
                    "delta": args,
                });
                self.seq += 1;
                evs.push(ev);
            }
        }
        evs
    }

    pub fn completed_event(&mut self) -> Value {
        let full_text = self.deltas.join("");
        let mut output_tokens = estimate_tokens(&full_text);
        for tc in self.tool_calls.values() {
            output_tokens += estimate_tokens(&tc.arguments);
        }
        let usage = json!({
            "input_tokens": self.input_tokens,
            "output_tokens": output_tokens,
            "total_tokens": self.input_tokens + output_tokens
        });

        let b = BuiltResponse {
            response_id: self.response_id.clone(),
            message_id: self.message_id.clone(),
            created: self.created,
            model: self.model.clone(),
            text: full_text,
            usage: usage.clone(),
        };
        let mut completed_response = build_non_stream_json(&b);
        if let Some(output) = completed_response
            .get_mut("output")
            .and_then(|v| v.as_array_mut())
        {
            let mut keys: Vec<usize> = self.tool_calls.keys().copied().collect();
            keys.sort_unstable();
            for k in keys {
                if let Some(tc) = self.tool_calls.get(&k) {
                    output.push(json!({
                        "id": tc.item_id,
                        "type": "function_call",
                        "status": "completed",
                        "call_id": tc.call_id,
                        "name": tc.name,
                        "arguments": tc.arguments,
                    }));
                }
            }
        }

        let ev = json!({
            "type": "response.completed",
            "sequence_number": self.seq,
            "response_id": self.response_id,
            "response": completed_response
        });

        self.seq += 1;
        ev
    }
}

pub fn build_response(req: &CreateResponseRequest, text: String) -> BuiltResponse {
    let model = req.model.clone().unwrap_or_else(|| "unknown".to_string());

    let created = now_unix_seconds();

    let input_tokens = estimate_tokens(&req.extract_input_text());
    let output_tokens = estimate_tokens(&text);

    BuiltResponse {
        response_id: format!("resp_{}", Uuid::new_v4()),
        message_id: format!("msg_{}", Uuid::new_v4()),
        created,
        model,
        text,
        usage: json!({
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens
        }),
    }
}

pub fn build_non_stream_json(b: &BuiltResponse) -> Value {
    json!({
        "id": b.response_id,
        "object": "response",
        "created": b.created,
        "model": b.model,
        "status": "completed",
        "output": [
            {
                "id": b.message_id,
                "type": "message",
                "role": "assistant",
                "status": "completed",
                "content": [
                    {
                        "type": "output_text",
                        "text": b.text,
                        "annotations": []
                    }
                ]
            }
        ],
        "usage": b.usage
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_event_sequence_is_monotonic() {
        let req = CreateResponseRequest {
            model: Some("m".into()),
            input: Some(Value::String("hi".into())),
            instructions: None,
            stream: Some(true),
        };
        let mut builder = ResponseStreamBuilder::new(&req);
        let created = builder.created_event();
        let delta = builder.push_delta("hello".into());
        let completed = builder.completed_event();

        assert_eq!(created["type"], "response.created");
        assert_eq!(created["sequence_number"], 0);
        assert_eq!(delta["type"], "response.output_text.delta");
        assert_eq!(delta["sequence_number"], 1);
        assert_eq!(completed["type"], "response.completed");
        assert_eq!(completed["sequence_number"], 2);
    }

    #[test]
    fn tool_call_stream_events_and_completed_output() {
        let req = CreateResponseRequest {
            model: Some("m".into()),
            input: Some(Value::String("hi".into())),
            instructions: None,
            stream: Some(true),
        };
        let mut builder = ResponseStreamBuilder::new(&req);
        let _ = builder.created_event();
        let evs = builder.push_tool_call_delta(
            0,
            Some("call_1".into()),
            Some("get_weather".into()),
            Some("{\"x\":1}".into()),
        );
        assert!(evs
            .iter()
            .any(|e| e["type"] == "response.output_item.added"));
        assert!(evs
            .iter()
            .any(|e| e["type"] == "response.function_call_arguments.delta"));
        let completed = builder.completed_event();
        let output = completed["response"]["output"].as_array().unwrap();
        assert!(output.iter().any(|o| o["type"] == "function_call"));
        assert_eq!(
            output
                .iter()
                .find(|o| o["type"] == "function_call")
                .unwrap()["arguments"],
            "{\"x\":1}"
        );
    }

    #[test]
    fn response_state_store_rehydrates_and_isolates_by_tenant() {
        let store = ResponseStateStore::default();
        let turn1 = vec![
            json!({"role": "system", "content": "be concise"}),
            json!({"role": "user", "content": "hello"}),
            json!({"role": "assistant", "content": "hi there"}),
        ];
        store.save_response(Some("tenant-a"), "resp_1", None, turn1);

        let turn2 = vec![json!({"role": "user", "content": "follow up"})];
        // Tenant B cannot access Tenant A's previous_response_id
        assert!(store
            .rehydrate_chat_messages(Some("tenant-b"), "resp_1", &turn2)
            .is_none());

        // Tenant A rehydrates full history and keeps root session id
        let (merged, root_sid) = store
            .rehydrate_chat_messages(Some("tenant-a"), "resp_1", &turn2)
            .expect("tenant-a should rehydrate");
        assert_eq!(root_sid, "resp_1");
        assert_eq!(merged.len(), 4);
        assert_eq!(merged[3]["content"], "follow up");
    }
}
