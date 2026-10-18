#!/usr/bin/env python3
"""Minimal OpenAI-compatible HTTP server for CI / local gateway smoke tests."""
from __future__ import annotations

import json
import sys
import time

STALL_SENTINEL = "__stall__"
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


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
        if self.path.split("?", 1)[0] == "/metrics":
            self._send_text(404, b"# mock metrics unavailable\n")
            return
        self.send_response(404)
        self.end_headers()

    def do_POST(self) -> None:
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
                stall = STALL_SENTINEL in json.dumps(req.get("messages") or req.get("prompt") or "")
                self._stream_chat(model, stall=stall)
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

    def _stream_chat(self, model: str, stall: bool = False) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        frames = 2 if stall else 200
        for i in range(frames):
            chunk = {
                "id": "chatcmpl-mock-stream",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": model,
                "choices": [{"index": 0, "delta": {"content": f"{i} "}, "finish_reason": None}],
            }
            self.wfile.write(f"data: {json.dumps(chunk)}\n\n".encode())
            self.wfile.flush()
            time.sleep(0.05)
        if stall:
            # Sentinel for the stalled-stream gate: keep the socket open without ever
            # sending [DONE] so the proxy's idle deadline is what ends the response.
            time.sleep(300)
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
    server = ThreadingHTTPServer((host, port), MockOpenAIHandler)
    print(f"mock OpenAI engine listening on http://{host}:{port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
