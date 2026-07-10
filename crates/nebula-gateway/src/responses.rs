use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateResponseRequest {
    pub model: Option<String>,
    pub input: Option<Value>,
    #[allow(dead_code)]
    pub instructions: Option<Value>,
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

pub struct ResponseStreamBuilder {
    response_id: String,
    message_id: String,
    created: u64,
    model: String,
    seq: u64,
    deltas: Vec<String>,
    input_tokens: u32,
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

    pub fn completed_event(&mut self) -> Value {
        let full_text = self.deltas.join("");
        let output_tokens = estimate_tokens(&full_text);
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
            usage,
        };
        let completed_response = build_non_stream_json(&b);

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
}
