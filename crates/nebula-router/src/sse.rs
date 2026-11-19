//! SSE response scanning and error-frame helpers for Nebula Router.

/// `choices[]` of an OpenAI-shaped body paused for a tool call.
fn json_has_tool_calls(v: &serde_json::Value) -> bool {
    let Some(choices) = v.get("choices").and_then(|c| c.as_array()) else {
        return false;
    };
    choices.iter().any(|c| {
        c.get("finish_reason").and_then(|r| r.as_str()) == Some("tool_calls")
            || c.pointer("/message/tool_calls")
                .and_then(|tc| tc.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false)
            || c.pointer("/delta/tool_calls")
                .and_then(|tc| tc.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false)
    })
}

/// JSON bodies of an SSE chunk (`data: {...}` lines, empty / `[DONE]` dropped).
/// Falls back to the whole input when the input carries no SSE framing.
fn sse_data_payloads(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        let Some(rest) = line.trim_ascii().strip_prefix(b"data:") else {
            continue;
        };
        let payload = rest.trim_ascii();
        if payload.is_empty() || payload == b"[DONE]" {
            continue;
        }
        out.push(payload);
    }
    if out.is_empty() {
        out.push(bytes);
    }
    out
}

/// Inspect an upstream response payload or SSE chunk to detect whether the
/// model paused for a tool call (`finish_reason == "tool_calls"` or non-empty
/// `tool_calls` array).
///
/// SSE framing is stripped before the JSON parse: a `data: `-prefixed chunk never
/// parses as JSON, so a streamed `delta.tool_calls` used to be invisible here.
/// A chunk cut mid-object still cannot be parsed — engines emit a final chunk with
/// `finish_reason == "tool_calls"`, which is the reliable signal for streaming.
///
/// For streamed responses use [`ToolCallScanner`], which also handles a marker
/// split across chunk boundaries and skips the parse when no marker bytes appear.
pub fn response_has_tool_calls(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    if !text.contains("tool_calls") {
        return false;
    };
    if text.contains("\"finish_reason\":\"tool_calls\"")
        || text.contains("\"finish_reason\": \"tool_calls\"")
    {
        return true;
    }
    sse_data_payloads(bytes).into_iter().any(|payload| {
        serde_json::from_slice::<serde_json::Value>(payload)
            .map(|v| json_has_tool_calls(&v))
            .unwrap_or(false)
    })
}

const TOOL_CALLS_MARKER: &[u8] = b"tool_calls";
/// Bytes kept from the previous chunk so a marker split across a chunk boundary
/// is still detected (`tool_calls` minus one byte).
const TOOL_CALLS_SCAN_CARRY: usize = TOOL_CALLS_MARKER.len() - 1;

fn contains_marker(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Streaming `tool_calls` detector for SSE bodies.
///
/// Two things a bare per-chunk [`response_has_tool_calls`] call gets wrong for
/// streamed responses:
/// 1. Chunk boundaries fall wherever the transport decides — including inside the
///    `data:` framing or inside the `tool_calls` literal — so a per-chunk check
///    misses the event and the session silently loses its continuation lease.
///    Chunks are therefore assembled into complete SSE lines before analysis.
/// 2. UTF-8 validation + JSON parse ran on every chunk of every stream. A raw
///    byte prefilter means a stream without tool calls pays only a byte scan.
#[derive(Debug, Clone, Default)]
pub struct ToolCallScanner {
    /// Incomplete trailing line carried across chunks.
    buf: Vec<u8>,
    /// True if `tool_calls` was already spotted in the prefix of an oversized
    /// incomplete SSE line before its leading bytes were drained.
    seen_in_current_line: bool,
}

/// Cap on a single carried line. A JSON event larger than this is not something
/// the pause detector needs to parse; only the marker tail is kept so a later
/// marker split is still seen.
const TOOL_CALLS_SCAN_LINE_CAP: usize = 8 * 1024;

impl ToolCallScanner {
    /// Feed one streamed chunk; returns `true` once a `tool_calls` pause is seen.
    pub fn feed(&mut self, chunk: &[u8]) -> bool {
        self.buf.extend_from_slice(chunk);
        let mut hit = false;
        let mut consumed = 0;
        while let Some(nl) = self.buf[consumed..].iter().position(|b| *b == b'\n') {
            let line = &self.buf[consumed..consumed + nl];
            if self.seen_in_current_line
                || (contains_marker(line, TOOL_CALLS_MARKER) && response_has_tool_calls(line))
            {
                hit = true;
            }
            self.seen_in_current_line = false;
            consumed += nl + 1;
        }
        self.buf.drain(..consumed);
        if self.buf.len() > TOOL_CALLS_SCAN_LINE_CAP {
            if contains_marker(&self.buf, TOOL_CALLS_MARKER) {
                self.seen_in_current_line = true;
            }
            let keep = self.buf.len() - TOOL_CALLS_SCAN_CARRY;
            self.buf.drain(..keep);
        }
        hit
    }
}

/// OpenAI-shaped SSE error frame emitted when a stream ends before `[DONE]`.
///
/// Without it a deadline hit or a mid-stream upstream failure closed the response
/// body silently: the client saw a truncated `200` and could not tell a finished
/// turn from an aborted one (issue #37). Clients that already parse OpenAI error
/// frames (including our own `scripts/verify_bench.py`) surface it as an error
/// instead of a short answer.
pub fn sse_error_frame(code: &str, message: &str) -> bytes::Bytes {
    let payload = serde_json::json!({
        "error": { "message": message, "type": "upstream_error", "code": code }
    });
    bytes::Bytes::from(format!("data: {payload}\n\n"))
}
