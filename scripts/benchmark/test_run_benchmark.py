"""Deterministic protocol tests. Mock timings prove contracts, not GPU benefits."""

import contextlib
import hashlib
import io
import json
import socket
import tempfile
import threading
import time
import tracemalloc
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

import run_benchmark as bench


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        with self.server.lock:
            self.server.active += 1
            self.server.peak = max(self.server.peak, self.server.active)
            self.server.bodies.append(body)
        try:
            if self.server.mode == "slow_headers":
                self.wfile.write(b"HTTP/1.1 200 OK\r\n")
                self.wfile.flush()
                for _ in range(20):
                    time.sleep(0.03)
                    self.wfile.write(b"X-Slow: value\r\n")
                    self.wfile.flush()
                self.wfile.write(b"\r\n")
                self.wfile.flush()
                return
            self.send_response(429 if self.server.mode == "http_error" else 200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            if self.server.mode == "http_error":
                return
            if self.server.mode == "stall":
                time.sleep(0.15)
            events = [
                {"choices": [{"delta": {"role": "assistant"}, "finish_reason": None}]},
                {"choices": [{"delta": {"content": "你"}, "finish_reason": None}]},
                {"choices": [{"delta": {"content": "好"}, "finish_reason": None}]},
                {"choices": [{"delta": {"content": "。"}, "finish_reason": None}]},
                {"choices": [{"delta": {}, "finish_reason": "stop"}]},
            ]
            if self.server.mode == "error_frame":
                events = [{"error": {"message": "must not be copied to evidence"}}]
            if self.server.mode == "no_finish":
                events.pop()
            if self.server.mode == "cancelled":
                events[-1]["choices"][0]["finish_reason"] = "abort"
            if self.server.mode == "oversized":
                self.wfile.write(b"data: " + b"x" * (bench.MAX_EVENT_BYTES + 1))
                self.wfile.flush()
                return
            if self.server.mode == "invalid_json":
                self.wfile.write(b"data: not-json\n\n")
                self.wfile.flush()
                return
            for event in events:
                # Split UTF-8 bytes across writes and use CRLF SSE framing.
                wire = b"data: " + json.dumps(event, ensure_ascii=False).encode() + b"\r\n\r\n"
                for part in (wire[:3], wire[3:]):
                    self.wfile.write(part)
                    self.wfile.flush()
                time.sleep(0.015)
            if self.server.mode != "missing_usage":
                tokens = 0 if self.server.mode == "zero_usage" else 3
                wire = json.dumps({"choices": [], "usage": {"completion_tokens": tokens}})
                self.wfile.write(f"data: {wire}\n\n".encode())
            # Usage/DONE trailer must not inflate TPOT.
            time.sleep(0.05)
            if self.server.mode != "eof":
                self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass
        finally:
            with self.server.lock:
                self.server.active -= 1


class BenchmarkTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.server.lock = threading.Lock()
        cls.server.active = cls.server.peak = 0
        cls.server.bodies = []
        cls.server.mode = "normal"
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.server.mode = "normal"
        self.server.peak = 0
        self.server.bodies = []

    def args(self, *extra):
        return bench.parse_args([
            "--model", "test", "--base-url", f"http://127.0.0.1:{self.server.server_port}/v1",
            "--requests", "4", "--concurrency", "2", *extra,
        ])

    def run_summary(self, *extra):
        args, workload = self.args(*extra)
        metrics = bench.run_requests(args, bench.request_plan(args, workload))
        run = bench.build_run(args, workload, metrics, "trace", "samples")
        return args, metrics, run

    def test_real_concurrency_and_streaming_measurements(self):
        _, metrics, run = self.run_summary("--gpu-count", "2", "--slo-e2e-ms", "1000")
        self.assertEqual(self.server.peak, 2)
        self.assertEqual(run["status"], "succeeded")
        self.assertEqual(len(metrics["samples"]), 4)
        self.assertTrue(all(body["stream"] for body in self.server.bodies))
        self.assertTrue(all(body["stream_options"]["include_usage"] for body in self.server.bodies))
        self.assertTrue(all(body["seed"] == 42 for body in self.server.bodies))
        for row in metrics["samples"]:
            self.assertLess(row["ttft_ms"], row["e2e_ms"])
            self.assertLess(row["tpot_ms"], (row["e2e_ms"] - row["ttft_ms"]) / 2)
        self.assertAlmostEqual(run["throughput_tps"], 12 / metrics["wall_time_s"])
        self.assertAlmostEqual(run["evidence"]["throughput_per_gpu_tps"], run["throughput_tps"] / 2)
        self.assertAlmostEqual(run["evidence"]["goodput_rps"], 4 / metrics["wall_time_s"])
        self.assertAlmostEqual(run["cost_per_1k_tokens"], 2000 / run["throughput_tps"])

    def test_open_loop_overload_does_not_wait_and_change_arrival_rate(self):
        _, metrics, run = self.run_summary(
            "--concurrency", "1", "--arrival", "constant", "--arrival-rate", "1000",
        )
        self.assertEqual(self.server.peak, 1)
        rejected = [row for row in metrics["samples"] if row["error"] == "client_overloaded"]
        self.assertEqual(len(rejected), 3)
        self.assertEqual([row["offset_s"] for row in metrics["samples"]], [0, 0.001, 0.002, 0.003])
        self.assertEqual(run["status"], "failed")
        self.assertEqual(run["error_rate"], 0.75)

    def test_seeded_arrivals_and_trace_replay(self):
        args, workload = self.args("--arrival", "poisson", "--arrival-rate", "10")
        first = bench.request_plan(args, workload)
        self.assertEqual(first, bench.request_plan(args, workload))
        args.seed += 1
        self.assertNotEqual(first, bench.request_plan(args, workload))
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.jsonl"
            trace.write_text("".join(json.dumps(row) + "\n" for row in first))
            args.trace = str(trace)
            self.assertEqual(first, bench.request_plan(args, workload))
            trace.write_text('{"offset_s": -1, "prompt": "x", "max_tokens": 1}\n')
            with self.assertRaises(ValueError):
                bench.request_plan(args, workload)

    def test_sse_parser_handles_bytewise_utf8_and_multiline_data(self):
        wire = 'data: {"text":\r\ndata: "你好"}\r\n\r\ndata: [DONE]\n\n'.encode()
        response = unittest.mock.MagicMock()
        response.read1.side_effect = [bytes([byte]) for byte in wire] + [b""]
        sock = unittest.mock.MagicMock()
        events = list(bench.stream_events(response, sock, time.perf_counter() + 2))
        self.assertEqual(json.loads(events[0]), {"text": "你好"})
        self.assertEqual(events[1], "[DONE]")

    def test_plan_hash_is_canonical_and_has_bounded_transient_memory(self):
        args, workload = self.args("--requests", "100")
        workload["prompt"] = "x" * 65536
        plan = bench.request_plan(args, workload)
        expected = hashlib.sha256(bench.encoded_json(plan)).hexdigest()
        tracemalloc.start()
        try:
            actual = bench.hash_plan(plan)
            _, peak = tracemalloc.get_traced_memory()
        finally:
            tracemalloc.stop()
        self.assertEqual(actual, expected)
        self.assertLess(peak, 1024 * 1024)

    def test_slow_headers_cannot_extend_absolute_deadline(self):
        self.server.mode = "slow_headers"
        args, workload = self.args("--timeout", "0.12")
        start = time.perf_counter()
        row = bench.chat_once(args, bench.request_plan(args, workload)[0], start)
        elapsed = time.perf_counter() - start
        self.assertFalse(row["ok"])
        self.assertEqual(row["error"], "timeout")
        self.assertLess(elapsed, 0.3)

    def test_multiple_connect_attempts_share_one_deadline(self):
        clock = [0.0]
        attempted = []

        class TimeoutSocket:
            def settimeout(self, timeout):
                self.timeout = timeout

            def connect(self, address):
                attempted.append(address)
                clock[0] += self.timeout
                raise TimeoutError

            def close(self):
                pass

        addresses = [
            (socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", port))
            for port in (1234, 1235)
        ]
        with patch.object(bench.socket, "getaddrinfo", return_value=addresses), patch.object(
            bench.socket, "socket", side_effect=lambda *args: TimeoutSocket()
        ), patch.object(bench.time, "perf_counter", side_effect=lambda: clock[0]):
            with self.assertRaises(TimeoutError):
                bench.connect_with_deadline("test", 80, 0.12)
        self.assertEqual(len(attempted), 1)
        self.assertAlmostEqual(clock[0], 0.12)

    def test_tls_handshake_receives_only_remaining_connect_budget(self):
        clock = [0.0]
        connected = unittest.mock.MagicMock()
        connected.connect.side_effect = lambda address: clock.__setitem__(0, 0.04)
        context = unittest.mock.MagicMock()
        context.wrap_socket.return_value = connected
        addresses = [(socket.AF_INET, socket.SOCK_STREAM, 6, "", ("127.0.0.1", 443))]
        with patch.object(bench.socket, "getaddrinfo", return_value=addresses), patch.object(
            bench.socket, "socket", return_value=connected
        ), patch.object(bench.time, "perf_counter", side_effect=lambda: clock[0]):
            self.assertIs(bench.connect_with_deadline("test", 443, 0.12, context), connected)
        self.assertAlmostEqual(connected.settimeout.call_args.args[0], 0.08)
        connected.setsockopt.assert_called_once_with(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        context.wrap_socket.assert_called_once_with(connected, server_hostname="test")
        context.wrap_socket.side_effect = TimeoutError
        with patch.object(bench.socket, "getaddrinfo", return_value=addresses), patch.object(
            bench.socket, "socket", return_value=connected
        ), patch.object(bench.time, "perf_counter", return_value=0):
            with self.assertRaises(TimeoutError):
                bench.connect_with_deadline("test", 443, 0.12, context)
        connected.close.assert_called_once()

    def test_eof_error_timeout_and_oversize_never_become_success(self):
        for mode in ("eof", "error_frame", "http_error", "no_finish", "cancelled", "invalid_json", "oversized", "stall"):
            with self.subTest(mode=mode):
                self.server.mode = mode
                args, workload = self.args("--timeout", "0.04" if mode == "stall" else "2")
                spec = bench.request_plan(args, workload)[0]
                row = bench.chat_once(args, spec, time.perf_counter())
                self.assertFalse(row["ok"])
                self.assertIsNotNone(row["error"])
                self.assertIsNotNone(row["e2e_ms"])
                self.assertNotIn("must not be copied", json.dumps(row))
        # Let the deliberately timed-out handler finish before other tests.
        time.sleep(0.2)

    def test_missing_and_zero_usage_are_not_estimated(self):
        for mode, expected in (("missing_usage", None), ("zero_usage", 0)):
            with self.subTest(mode=mode):
                self.server.mode = mode
                args, workload = self.args("--requests", "1")
                metrics = bench.run_requests(args, bench.request_plan(args, workload))
                row = metrics["samples"][0]
                self.assertTrue(row["ok"])
                self.assertEqual(row["completion_tokens"], expected)
                run = bench.build_run(args, workload, metrics, "trace", "samples")
                self.assertEqual(run["status"], "insufficient_data")
                self.assertIsNone(run["cost_per_1k_tokens"])

    def test_unknown_gpu_count_does_not_fabricate_normalized_cost(self):
        _, _, run = self.run_summary("--requests", "1")
        self.assertIsNone(run["cost_per_1k_tokens"])
        self.assertIsNone(run["evidence"]["throughput_per_gpu_tps"])
        self.assertIsNone(run["evidence"]["goodput_rps"])

    def test_wall_time_goodput_and_failed_sample_alignment(self):
        args, workload = self.args("--gpu-count", "2", "--slo-ttft-ms", "100")
        samples = [
            {"ok": True, "completion_tokens": 3, "ttft_ms": 50, "tpot_ms": 20, "e2e_ms": 1000},
            {"ok": False, "completion_tokens": None, "ttft_ms": None, "tpot_ms": None, "e2e_ms": 10},
            {"ok": True, "completion_tokens": 5, "ttft_ms": 150, "tpot_ms": 30, "e2e_ms": 1000},
        ]
        metrics = {"samples": samples, "wall_time_s": 2, "started_at_ms": 1, "finished_at_ms": 2001}
        run = bench.build_run(args, workload, metrics, "trace", "samples")
        self.assertEqual(run["throughput_tps"], 4)
        self.assertEqual(run["tpot_p95_ms"], 29.5)
        self.assertEqual(run["error_rate"], 1 / 3)
        self.assertEqual(run["evidence"]["goodput_rps"], 0.5)
        self.assertEqual(run["evidence"]["goodput_tps"], 1.5)
        self.assertEqual(run["status"], "failed")

    def test_dry_run_artifacts_no_network_no_credentials_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "run.json"
            artifacts = Path(directory) / "evidence"
            with patch.object(bench, "run_requests", side_effect=AssertionError("network")), patch.object(
                bench, "cache_snapshot", side_effect=AssertionError("network")
            ), contextlib.redirect_stderr(io.StringIO()):
                code = bench.main([
                    "--model", "test", "--dry-run", "--token", "not-a-real-token",
                    "--notes", "custom notes cannot hide synthetic status",
                    "--out", str(out), "--artifacts-dir", str(artifacts),
                    "--comparison-group", "comparison", "--abba-position", "2",
                ])
            self.assertEqual(code, 0)
            run = json.loads(out.read_text())
            manifest = (artifacts / "manifest.json").read_text()
            raw = (artifacts / "samples.jsonl").read_bytes()
            self.assertEqual(run["status"], "insufficient_data")
            self.assertTrue(run["evidence"]["synthetic"])
            self.assertEqual(run["evidence"]["variant"], "B")
            self.assertEqual(run["evidence"]["samples_sha256"], hashlib.sha256(raw).hexdigest())
            self.assertNotIn("not-a-real-token", manifest)
            self.assertNotIn("Authorization", manifest)
            self.assertIsNone(run["throughput_tps"])
            with contextlib.redirect_stderr(io.StringIO()):
                code = bench.main([
                    "--model", "test", "--dry-run",
                    "--out", str(out), "--artifacts-dir", str(artifacts),
                ])
            self.assertEqual(code, 2)

    def test_invalid_numeric_and_url_options_fail_before_network(self):
        for options in (
            ["--concurrency", "0"], ["--requests", "-1"], ["--gpu-count", "0"],
            ["--timeout", "nan"], ["--arrival", "constant"],
            ["--arrival", "poisson", "--arrival-rate", "inf"],
            ["--slo-ttft-ms", "-1"], ["--base-url", "http://user:secret@localhost/v1"],
            ["--abba-position", "0"], ["--cache-metrics-url", "http://localhost/metrics"],
        ):
            with self.subTest(options=options), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    self.args(*options)

    def test_dry_run_with_ingest_is_rejected_without_network(self):
        with patch.object(bench, "urlopen", side_effect=AssertionError("network")) as network:
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                bench.main([
                    "--model", "test", "--dry-run", "--ingest", "http://localhost/ingest",
                    "--token", "not-a-real-token",
                ])
            network.assert_not_called()

    def test_cache_counter_snapshot_and_unknowns(self):
        args, _ = self.args(
            "--cache-metrics-url", "http://localhost/metrics", "--cache-counter", "hits_total",
        )
        response = unittest.mock.MagicMock()
        response.__enter__.return_value.read.return_value = b'hits_total{worker="a"} 2\nhits_total{worker="b"} 3\n'
        with patch.object(bench, "urlopen", return_value=response):
            self.assertEqual(bench.cache_snapshot(args), {"hits_total": 5})
            response.__enter__.return_value.read.return_value = b"other_total 3\n"
            self.assertIsNone(bench.cache_snapshot(args))


if __name__ == "__main__":
    unittest.main()
