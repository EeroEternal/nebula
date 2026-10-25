#!/usr/bin/env python3
"""Minimal OpenAI-compatible HTTP server for CI / local gateway smoke tests."""
from __future__ import annotations

import json
import sys
import os
import time

# MOCK_STALL_FIRST=1 makes this instance stall before any frame on every request, so a
# second instance on another port can play the healthy replica.
STALL_FIRST_ALWAYS = os.environ.get("MOCK_STALL_FIRST") == "1"

STALL_SENTINEL = "__stall__"

# --- transport A/B knobs (scripts/ab_upstream_transport.sh) -------------------------
# Chunked + keep-alive is the shape a real engine serves: it lets the proxy *reuse* the
# connection, which is the thing a transport swap can actually change. The default stays
# close-delimited because ci_smoke.sh asserts on that framing.
CHUNKED = os.environ.get("MOCK_CHUNKED") == "1"
FIRST_BYTE_MS = float(os.environ.get("MOCK_FIRST_BYTE_MS", "0"))
CHUNK_MS = float(os.environ.get("MOCK_CHUNK_MS", "50"))
TOKENS = int(os.environ.get("MOCK_TOKENS", "200"))

import threading

_STATS = {"requests": 0, "streams": 0, "aborts": 0, "bytes": 0}
_STATS_LOCK = threading.Lock()


def _bump(key: str, amount: int = 1) -> None:
    with _STATS_LOCK:
        _STATS[key] += amount


def _snapshot() -> dict:
    with _STATS_LOCK:
        return dict(_STATS)
# Stalls *before* the first frame: exercises the pre-first-byte retry (issue #42).
STALL_FIRST_SENTINEL = "__stall_first__"
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class MockServer(ThreadingHTTPServer):
    """`socketserver`'s default accept backlog is 5, which is far below any real engine
    (vLLM/SGLang listen with >= 2048). Under a burst of concurrent dials — exactly what a
    transport A/B produces — a backlog of 5 makes the kernel drop SYNs, the peers retry
    after a ~1 s RTO, and the measurement then reports *the mock's* queue as transport
    latency. Found by xlite while reviewing the first A/B round (issue #96)."""

    request_queue_size = 128
    daemon_threads = True


class MockOpenAIHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt: str, *args) -> None:
        sys.stderr.write("%s - %s\n" % (self.address_string(), fmt % args))

    def _send_json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _send_text(self, status: int, body: bytes, content_type: str = "text/plain") -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self) -> None:
        if self.path.split("?", 1)[0] in ("/health", "/healthz"):
            self._send_text(200, b"ok")
            return
        if self.path.split("?", 1)[0] == "/__mock_stats":
            self._send_json(200, _snapshot())
            return
        if self.path.split("?", 1)[0] == "/metrics":
            self._send_text(404, b"# mock metrics unavailable\n")
            return
        self.send_response(404)
        self.end_headers()

    def do_POST(self) -> None:
        _bump("requests")
        path = self.path.split("?", 1)[0]
        length = int(self.headers.get("Content-Length", "0") or "0")
        raw = self.rfile.read(length) if length else b"{}"
        try:
            req = json.loads(raw.decode() or "{}")
        except json.JSONDecodeError:
            self._send_json(400, {"error": {"message": "invalid json", "type": "invalid_request_error"}})
            return

        model = req.get("model", "mock-model")
        if path == "/v1/chat/completions":
            if req.get("stream"):
                payload = json.dumps(req.get("messages") or req.get("prompt") or "")
                stall = STALL_SENTINEL in payload
                stall_first = STALL_FIRST_ALWAYS or STALL_FIRST_SENTINEL in payload
                self._stream_chat(model, stall=stall, stall_first=stall_first)
                return
            self._send_json(
                200,
                {
                    "id": "chatcmpl-mock",
                    "object": "chat.completion",
                    "created": int(time.time()),
                    "model": model,
                    "choices": [
                        {
                            "index": 0,
                            "message": {"role": "assistant", "content": "mock chat ok"},
                            "finish_reason": "stop",
                        }
                    ],
                    "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3},
                },
            )
            return

        if path == "/v1/completions":
            self._send_json(
                200,
                {
                    "id": "cmpl-mock",
                    "object": "text_completion",
                    "created": int(time.time()),
                    "model": model,
                    "choices": [{"index": 0, "text": "mock completion ok", "finish_reason": "length"}],
                },
            )
            return

        self.send_response(404)
        self.end_headers()

    def _write_frame(self, payload: bytes) -> bool:
        """Write one SSE frame; False when the client went away (abort)."""
        frame = b"data: " + payload + b"\n\n"
        try:
            if CHUNKED:
                self.wfile.write(b"%x\r\n" % len(frame) + frame + b"\r\n")
            else:
                self.wfile.write(frame)
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            return False
        _bump("bytes", len(frame))
        return True

    def _stream_chat(self, model: str, stall: bool = False, stall_first: bool = False) -> None:
        _bump("streams")
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        if CHUNKED:
            self.send_header("Transfer-Encoding", "chunked")
            self.send_header("Connection", "keep-alive")
        self.end_headers()
        if stall_first:
            # Headers only, then silence: a replica that accepted the request and never
            # produced a byte. The proxy must retry elsewhere, not commit a status.
            time.sleep(300)
            return
        if FIRST_BYTE_MS:
            time.sleep(FIRST_BYTE_MS / 1000.0)
        frames = 2 if stall else TOKENS
        for i in range(frames):
            chunk = {
                "id": "chatcmpl-mock-stream",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": model,
                "choices": [{"index": 0, "delta": {"content": f"{i} "}, "finish_reason": None}],
            }
            if not self._write_frame(json.dumps(chunk).encode()):
                _bump("aborts")
                return
            time.sleep(CHUNK_MS / 1000.0)
        if stall:
            # Sentinel for the stalled-stream gate: keep the socket open without ever
            # sending [DONE] so the proxy's idle deadline is what ends the response.
            time.sleep(300)
            return
        if CHUNKED:
            try:
                done = b"data: [DONE]\n\n"
                self.wfile.write(b"%x\r\n" % len(done) + done + b"\r\n")
                self.wfile.write(b"0\r\n\r\n")
                self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                _bump("aborts")
                return
            # Keep-alive on purpose: the next request may reuse this socket.
            return
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()
        # A real engine closes the response after [DONE]. Without this the handler keeps
        # the socket open for a next request on the same HTTP/1.1 connection, so the
        # proxy never sees EOF and its idle deadline fires *after* a perfectly good
        # stream (found while adding the stalled-stream gate).
        self.close_connection = True


def main() -> None:
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 18999
    host = sys.argv[2] if len(sys.argv) > 2 else "127.0.0.1"
    server = MockServer((host, port), MockOpenAIHandler)
    print(f"mock OpenAI engine listening on http://{host}:{port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
