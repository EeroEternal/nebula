#!/usr/bin/env python3
"""Client-side streaming benchmark and replayable evidence protocol (issue #62).

Standard library only. Raw evidence records timings/counts, never response text,
request headers, or credentials. See README.md for measurement definitions.
"""

from __future__ import annotations

import argparse
import concurrent.futures as futures
import hashlib
import http.client
import io
import json
import math
import os
import random
import re
import socket
import ssl
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit
from urllib.request import Request, urlopen

WORKLOAD_DIR = Path(__file__).resolve().parent / "workloads"
MAX_EVENT_BYTES = 1024 * 1024
MAX_REQUESTS = 100_000
QUANT_FORMATS = {"w4a4", "w4a16", "fp8", "w8a8", "int8"}


def percentile(values: list[float], p: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    k = (len(ordered) - 1) * p / 100
    lo = int(k)
    hi = min(lo + 1, len(ordered) - 1)
    return ordered[lo] + (ordered[hi] - ordered[lo]) * (k - lo)


def finite_number(value: Any, *, positive: bool = False) -> bool:
    return (
        isinstance(value, (int, float))
        and not isinstance(value, bool)
        and math.isfinite(value)
        and (value > 0 if positive else value >= 0)
    )


def validate_url(url: str) -> None:
    parsed = urlsplit(url)
    if (
        parsed.scheme not in {"http", "https"}
        or not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
    ):
        raise ValueError("URLs must be HTTP(S), without credentials, query, or fragment")
    _ = parsed.port


def encoded_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, ensure_ascii=False, allow_nan=False).encode()


def hash_plan(plan: list[dict[str, Any]]) -> str:
    """Hash the canonical JSON array without materializing repeated prompts."""
    digest = hashlib.sha256(b"[")
    for index, row in enumerate(plan):
        if index:
            digest.update(b", ")
        digest.update(encoded_json(row))
    digest.update(b"]")
    return digest.hexdigest()


def load_engine_capability(args: argparse.Namespace) -> dict[str, Any] | None:
    if not args.capability_file:
        return None
    if not all(value and value.strip() for value in (args.engine_version, args.image_id, args.platform)):
        raise ValueError("capability evidence requires engine-version, image-id, and platform")
    path = Path(args.capability_file)
    if path.stat().st_size > 64 * 1024:
        raise ValueError("capability file exceeds 64 KiB")
    capability = json.loads(path.read_text())
    if not isinstance(capability, dict):
        raise ValueError("invalid capability snapshot")
    paths = capability.get("native_low_precision_paths", {})
    quantization = capability.get("quantization")
    if (
        capability.get("engine_type") != args.engine
        or capability.get("engine_version") != args.engine_version
        or capability.get("source") not in {"static_table", "runtime_discovery", "manual_override"}
        or (quantization is not None and (not isinstance(quantization, str) or quantization not in QUANT_FORMATS))
        or not isinstance(paths, dict)
        or any(key not in QUANT_FORMATS or value not in {"supported", "unsupported", "unknown"} for key, value in paths.items())
    ):
        raise ValueError("invalid or mismatched low-precision declaration")
    # Archive only the selection-related subset, never arbitrary notes/metadata.
    return {
        "engine_type": args.engine, "engine_version": args.engine_version,
        "source": capability["source"], "quantization": quantization,
        "native_low_precision_paths": paths,
    }


def remaining_budget(deadline: float) -> float:
    remaining = deadline - time.perf_counter()
    if remaining <= 0:
        raise TimeoutError
    return remaining


def connect_with_deadline(host, port, deadline, context=None):
    # The OS resolver is not interruptible; once it returns, all address
    # attempts and the optional TLS handshake share the same remaining budget.
    addresses = socket.getaddrinfo(host, port, 0, socket.SOCK_STREAM)
    connected = None
    last_error = None
    for family, kind, protocol, _, address in addresses:
        remaining = remaining_budget(deadline)
        candidate = None
        try:
            candidate = socket.socket(family, kind, protocol)
            candidate.settimeout(remaining)
            candidate.connect(address)
            candidate.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            connected = candidate
            break
        except OSError as error:
            last_error = error
            if candidate is not None:
                candidate.close()
    if connected is None:
        remaining_budget(deadline)
        raise last_error or OSError("no usable endpoint address")
    try:
        if context is not None:
            connected.settimeout(remaining_budget(deadline))
            connected = context.wrap_socket(connected, server_hostname=host)
        remaining_budget(deadline)
        return connected
    except Exception:
        connected.close()
        raise


class DeadlineReader(io.RawIOBase):
    """Reapply an absolute deadline on *every* underlying header/body read."""

    def __init__(self, raw, sock, deadline):
        self.raw = raw
        self.sock = sock
        self.deadline = deadline

    def readable(self):
        return True

    def readinto(self, buffer):
        self.sock.settimeout(remaining_budget(self.deadline))
        return self.raw.readinto(buffer)

    def close(self):
        try:
            self.raw.close()
        finally:
            super().close()


class DeadlineSocket:
    def __init__(self, sock, deadline):
        self.sock = sock
        self.deadline = deadline

    def __getattr__(self, name):
        return getattr(self.sock, name)

    def sendall(self, data):
        self.sock.settimeout(remaining_budget(self.deadline))
        return self.sock.sendall(data)

    def makefile(self, mode):
        if mode != "rb":
            raise ValueError("only HTTP response reads are supported")
        return io.BufferedReader(DeadlineReader(
            self.sock.makefile("rb", buffering=0), self.sock, self.deadline,
        ))


def request_plan(args: argparse.Namespace, workload: dict[str, Any]) -> list[dict[str, Any]]:
    if args.trace:
        path = Path(args.trace)
        if path.stat().st_size > 32 * 1024 * 1024:
            raise ValueError("trace exceeds 32 MiB")
        rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
        if not rows or len(rows) > MAX_REQUESTS:
            raise ValueError("trace must contain 1..100000 requests")
    else:
        rng = random.Random(args.seed)
        offset = 0.0
        rows = []
        for _ in range(workload["request_count"]):
            rows.append({
                "offset_s": offset,
                "prompt": workload["prompt"],
                "max_tokens": workload["max_tokens"],
            })
            if args.arrival != "closed":
                offset += (
                    rng.expovariate(args.arrival_rate)
                    if args.arrival == "poisson" else 1 / args.arrival_rate
                )
    previous = 0.0
    for i, row in enumerate(rows):
        if not isinstance(row, dict):
            raise ValueError("trace entries must be objects")
        offset = row.get("offset_s")
        tokens = row.get("max_tokens")
        if not finite_number(offset) or offset < previous:
            raise ValueError("trace offsets must be finite, nonnegative, and ordered")
        if not isinstance(row.get("prompt"), str):
            raise ValueError("trace prompt must be a string")
        if not isinstance(tokens, int) or isinstance(tokens, bool) or tokens <= 0:
            raise ValueError("trace max_tokens must be a positive integer")
        previous = offset
        # No arbitrary headers or request body overrides from replay files.
        rows[i] = {
            "request_id": i, "offset_s": float(offset),
            "prompt": row["prompt"], "max_tokens": tokens,
        }
    return rows


def stream_events(response: http.client.HTTPResponse, sock: socket.socket, deadline: float):
    """Parse bounded SSE events, across arbitrary TCP chunks, with a total deadline."""
    buffer = b""
    data: list[str] = []
    size = 0
    while True:
        remaining = deadline - time.perf_counter()
        if remaining <= 0:
            raise TimeoutError
        sock.settimeout(remaining)
        chunk = response.read1(65536)
        if not chunk:
            return
        buffer += chunk
        while b"\n" in buffer:
            line, buffer = buffer.split(b"\n", 1)
            size += len(line)
            if size > MAX_EVENT_BYTES:
                raise ValueError("event_too_large")
            line = line.rstrip(b"\r")
            if not line:
                if data:
                    yield "\n".join(data)
                data = []
                size = 0
            elif line.startswith(b"data:"):
                data.append(line[5:].lstrip(b" ").decode("utf-8"))
        if size + len(buffer) > MAX_EVENT_BYTES:
            raise ValueError("event_too_large")


def empty_sample(spec: dict[str, Any], error: str) -> dict[str, Any]:
    return {
        "request_id": spec["request_id"], "offset_s": spec["offset_s"],
        "ok": False, "error": error, "ttft_ms": None, "tpot_ms": None,
        "e2e_ms": None, "completion_tokens": None, "schedule_delay_ms": None,
    }


def chat_once(
    args: argparse.Namespace, spec: dict[str, Any], scheduled: float,
) -> dict[str, Any]:
    sample = empty_sample(spec, "incomplete_stream")
    started = time.perf_counter()
    sample["schedule_delay_ms"] = max(0.0, (started - scheduled) * 1000)
    deadline = started + args.timeout
    url = urlsplit(args.base_url)
    context = ssl.create_default_context() if url.scheme == "https" else None
    if context is not None:
        context.set_alpn_protocols(["http/1.1"])
    connection = http.client.HTTPConnection(
        url.hostname, url.port or (443 if context is not None else 80), timeout=args.timeout,
    )
    headers = {"Content-Type": "application/json", "Accept": "text/event-stream"}
    if args.token:
        headers["Authorization"] = f"Bearer {args.token}"
    body = encoded_json({
        "model": args.model,
        "messages": [{"role": "user", "content": spec["prompt"]}],
        "max_tokens": spec["max_tokens"], "stream": True,
        "stream_options": {"include_usage": True},
        "temperature": 0, "seed": args.seed,
    })
    first = last = None
    finished = False
    try:
        connection.sock = DeadlineSocket(
            connect_with_deadline(connection.host, connection.port, deadline, context), deadline,
        )
        connection.request("POST", url.path.rstrip("/") + "/chat/completions", body, headers)
        sock = connection.sock
        assert sock is not None
        remaining = deadline - time.perf_counter()
        if remaining <= 0:
            raise TimeoutError
        sock.settimeout(remaining)
        response = connection.getresponse()
        with response:
            if response.status != 200:
                sample["error"] = f"http_{response.status}"
                return sample
            for event in stream_events(response, sock, deadline):
                observed = time.perf_counter()
                if observed > deadline:
                    raise TimeoutError
                if event == "[DONE]":
                    if not finished:
                        raise ValueError("missing_finish_reason")
                    sample["ok"] = True
                    sample["error"] = None
                    break
                payload = json.loads(event)
                if not isinstance(payload, dict) or "error" in payload:
                    raise ValueError("upstream_error")
                usage = payload.get("usage")
                if usage is not None:
                    tokens = usage.get("completion_tokens") if isinstance(usage, dict) else None
                    if not isinstance(tokens, int) or isinstance(tokens, bool) or tokens < 0:
                        raise ValueError("invalid_usage")
                    sample["completion_tokens"] = tokens
                choices = payload.get("choices", [])
                if not isinstance(choices, list):
                    raise ValueError("invalid_choices")
                for choice in choices:
                    if not isinstance(choice, dict):
                        raise ValueError("invalid_choice")
                    delta = choice.get("delta") or {}
                    if not isinstance(delta, dict):
                        raise ValueError("invalid_delta")
                    if any(delta.get(key) for key in ("content", "reasoning_content", "reasoning", "tool_calls")):
                        if first is None:
                            first = observed
                        last = observed
                    if choice.get("finish_reason") is not None:
                        if choice["finish_reason"] not in (
                            "stop", "length", "tool_calls", "function_call", "content_filter",
                        ):
                            raise ValueError("invalid_finish_reason")
                        finished = True
                        sample["finish_reason"] = choice["finish_reason"]
        sample["e2e_ms"] = (time.perf_counter() - scheduled) * 1000
        if first is not None:
            sample["ttft_ms"] = (first - scheduled) * 1000
        tokens = sample["completion_tokens"]
        if first is not None and last is not None and tokens is not None and tokens > 1:
            sample["tpot_ms"] = (last - first) * 1000 / (tokens - 1)
    except (TimeoutError, socket.timeout):
        sample["error"] = "timeout"
    except (json.JSONDecodeError, UnicodeDecodeError, ValueError):
        sample["error"] = "invalid_stream"
    except (OSError, http.client.HTTPException):
        sample["error"] = "transport_error"
    finally:
        connection.close()
        if sample["e2e_ms"] is None:
            sample["e2e_ms"] = (time.perf_counter() - scheduled) * 1000
    return sample


def run_requests(args: argparse.Namespace, plan: list[dict[str, Any]]) -> dict[str, Any]:
    started_at_ms = int(time.time() * 1000)
    origin = time.perf_counter()
    samples: list[dict[str, Any]] = []
    with futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        pending: set[futures.Future] = set()
        for spec in plan:
            if args.arrival == "closed" and not args.trace:
                if len(pending) >= args.concurrency:
                    done, pending = futures.wait(pending, return_when=futures.FIRST_COMPLETED)
                    samples.extend(task.result() for task in done)
                scheduled = time.perf_counter()
            else:
                scheduled = origin + spec["offset_s"]
                delay = scheduled - time.perf_counter()
                if delay > 0:
                    time.sleep(delay)
                done = {task for task in pending if task.done()}
                pending -= done
                samples.extend(task.result() for task in done)
                if len(pending) >= args.concurrency:
                    # A bounded open-loop generator drops rather than quietly
                    # waiting and turning the offered arrival rate into closed-loop.
                    samples.append(empty_sample(spec, "client_overloaded"))
                    continue
            pending.add(pool.submit(chat_once, args, spec, scheduled))
        samples.extend(task.result() for task in futures.as_completed(pending))
    return {
        "samples": sorted(samples, key=lambda row: row["request_id"]),
        "wall_time_s": time.perf_counter() - origin,
        "started_at_ms": started_at_ms, "finished_at_ms": int(time.time() * 1000),
    }


def cache_snapshot(args: argparse.Namespace) -> dict[str, float] | None:
    if not args.cache_metrics_url:
        return None
    try:
        headers = {"Authorization": f"Bearer {args.token}"} if args.token else {}
        with urlopen(Request(args.cache_metrics_url, headers=headers), timeout=args.timeout) as response:
            body = response.read(4 * 1024 * 1024 + 1)
        if len(body) > 4 * 1024 * 1024:
            return None
        values: dict[str, float] = {}
        for line in body.decode().splitlines():
            match = re.fullmatch(r'([a-zA-Z_:][a-zA-Z0-9_:]*)(?:\{.*\})?\s+(\S+)(?:\s+\S+)?', line)
            if match and match[1] in args.cache_counter:
                value = float(match[2])
                if not finite_number(value):
                    return None
                values[match[1]] = values.get(match[1], 0.0) + value
        return values if all(name in values for name in args.cache_counter) else None
    except (OSError, ValueError, http.client.HTTPException):
        return None


def build_run(
    args: argparse.Namespace, workload: dict[str, Any], metrics: dict[str, Any],
    trace_sha256: str, samples_sha256: str,
) -> dict[str, Any]:
    samples = metrics["samples"]
    successful = [row for row in samples if row["ok"]]
    failures = len(samples) - len(successful)
    known_tokens = bool(successful) and all(row["completion_tokens"] is not None for row in successful)
    total_tokens = sum(row["completion_tokens"] for row in successful) if known_tokens else None
    wall = metrics["wall_time_s"]
    throughput = total_tokens / wall if total_tokens is not None and wall > 0 else None
    ttfts = [row["ttft_ms"] for row in successful if row["ttft_ms"] is not None]
    tpots = [row["tpot_ms"] for row in successful if row["tpot_ms"] is not None]
    e2es = [row["e2e_ms"] for row in successful if row["e2e_ms"] is not None]
    targets = {
        key: getattr(args, f"slo_{key}")
        for key in ("ttft_ms", "tpot_ms", "e2e_ms")
        if getattr(args, f"slo_{key}") is not None
    }
    compliant = [
        row for row in successful
        if all(row[key] is not None and row[key] <= limit for key, limit in targets.items())
        and row["completion_tokens"] is not None
    ]
    status = "succeeded"
    if args.dry_run or not total_tokens or len(ttfts) != len(successful):
        status = "insufficient_data"
    if failures and not args.dry_run:
        status = "failed"
    evidence = {
        "schema_version": 2, "synthetic": args.dry_run, "seed": args.seed,
        "arrival": "trace" if args.trace else args.arrival,
        "arrival_rate_rps": args.arrival_rate, "gpu_count": args.gpu_count,
        "cache_state": args.cache_state, "cache_state_source": "operator_declared",
        "request_count": len(samples), "successful_requests": len(successful),
        "wall_time_s": wall, "trace_sha256": trace_sha256, "samples_sha256": samples_sha256,
        "e2e_p50_ms": percentile(e2es, 50), "e2e_p95_ms": percentile(e2es, 95),
        "e2e_p99_ms": percentile(e2es, 99), "ttft_p99_ms": percentile(ttfts, 99),
        "throughput_per_gpu_tps": throughput / args.gpu_count if throughput is not None and args.gpu_count else None,
        "slo": targets,
        "goodput_rps": len(compliant) / wall if targets and wall > 0 and known_tokens else None,
        "goodput_tps": sum(row["completion_tokens"] for row in compliant) / wall if targets and wall > 0 and known_tokens else None,
        "comparison_group": args.comparison_group, "abba_position": args.abba_position,
        "variant": "ABBA"[args.abba_position] if args.abba_position is not None else None,
        "round": args.round,
        "engine_capability": args.engine_capability,
    }
    return {
        "run_id": args.run_id,
        "profile_key": {
            "model_name": args.model, "engine_type": args.engine,
            "engine_version": args.engine_version, "platform": args.platform,
            "gpu_name": args.gpu_name, "workload_id": workload["id"],
            # Load generation and SLO policy are part of comparable evidence.
            "param_fingerprint": hashlib.sha256(encoded_json({
                "engine_params": args.param_fingerprint, "trace_sha256": trace_sha256,
                "seed": args.seed,
                "image_id": args.image_id, "software_version": args.software_version,
                "concurrency": args.concurrency, "arrival": evidence["arrival"],
                "arrival_rate": args.arrival_rate, "gpu_count": args.gpu_count,
                "cache_state": args.cache_state, "slo": targets,
                "engine_capability": args.engine_capability,
            })).hexdigest(),
        },
        "workload": workload, "status": status, "base_url": args.base_url,
        "image_id": args.image_id, "software_version": args.software_version,
        "ttft_p50_ms": percentile(ttfts, 50), "ttft_p95_ms": percentile(ttfts, 95),
        "tpot_p95_ms": percentile(tpots, 95), "throughput_tps": throughput,
        "error_rate": failures / len(samples) if samples and not args.dry_run else None,
        "peak_vram_mb": args.peak_vram_mb,
        "cost_per_1k_tokens": args.gpu_count * 1000 / throughput if throughput and args.gpu_count else None,
        "started_at_ms": metrics["started_at_ms"], "finished_at_ms": metrics["finished_at_ms"],
        "error_message": None if status == "succeeded" else status,
        "evidence_notes": args.notes, "evidence": evidence,
    }


def parse_args(argv: list[str] | None = None) -> tuple[argparse.Namespace, dict[str, Any]]:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workload", default="short-chat-v1")
    parser.add_argument("--workload-file")
    parser.add_argument("--model", required=True)
    parser.add_argument("--engine", default="vllm", choices=["vllm", "sglang", "tokenspeed"])
    parser.add_argument("--engine-version")
    parser.add_argument("--capability-file", help="Run-scoped EngineCapability JSON, not an automatic probe")
    parser.add_argument("--platform", default="nvidia-cuda")
    parser.add_argument("--gpu-name")
    parser.add_argument("--gpu-count", type=int)
    parser.add_argument("--base-url", default="http://127.0.0.1:8081/v1")
    parser.add_argument("--token")
    parser.add_argument("--token-env", help="Read the bearer token from this environment variable")
    parser.add_argument("--image-id")
    parser.add_argument("--software-version", default="nebula-dev")
    parser.add_argument("--param-fingerprint")
    parser.add_argument("--peak-vram-mb", type=int)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--run-id")
    parser.add_argument("--notes")
    parser.add_argument("--out")
    parser.add_argument("--artifacts-dir")
    parser.add_argument("--ingest")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--concurrency", type=int)
    parser.add_argument("--requests", type=int)
    parser.add_argument("--max-tokens", type=int)
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--arrival", choices=["closed", "constant", "poisson"], default="closed")
    parser.add_argument("--arrival-rate", type=float)
    parser.add_argument("--trace", help="JSONL: offset_s, prompt, max_tokens")
    parser.add_argument("--cache-state", choices=["unknown", "cold", "warm"], default="unknown")
    parser.add_argument("--cache-metrics-url")
    parser.add_argument("--cache-counter", action="append", default=[])
    parser.add_argument("--comparison-group")
    parser.add_argument("--abba-position", type=int, choices=range(4))
    parser.add_argument("--round", type=int, default=1)
    for key in ("ttft", "tpot", "e2e"):
        parser.add_argument(f"--slo-{key}-ms", type=float)
    args = parser.parse_args(argv)
    try:
        if args.dry_run and args.ingest:
            raise ValueError("dry-run cannot ingest or perform network I/O")
        if args.trace and (args.requests is not None or args.max_tokens is not None):
            raise ValueError("trace supplies its own request count and max_tokens")
        path = Path(args.workload_file) if args.workload_file else WORKLOAD_DIR / f"{args.workload}.json"
        workload = json.loads(path.read_text())
        if (
            not isinstance(workload, dict)
            or not isinstance(workload.get("id"), str) or not workload["id"]
            or workload.get("class") not in {"short_chat", "long_context", "code_completion", "custom"}
            or not isinstance(workload.get("prompt"), str)
        ):
            raise ValueError("invalid workload identity/class/prompt")
        for option, field in (("concurrency", "concurrency"), ("requests", "request_count"), ("max_tokens", "max_tokens")):
            value = getattr(args, option)
            if value is None:
                value = workload[field]
            if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
                raise ValueError(f"{option} must be a positive integer")
            setattr(args, option, value)
            workload[field] = value
        if args.requests > MAX_REQUESTS or args.concurrency > MAX_REQUESTS:
            raise ValueError("requests/concurrency exceed 100000")
        if args.gpu_count is not None and args.gpu_count <= 0:
            raise ValueError("gpu-count must be positive")
        if args.peak_vram_mb is not None and args.peak_vram_mb < 0:
            raise ValueError("peak-vram-mb must be nonnegative")
        if args.seed < 0 or args.seed > 2**63 - 1 or args.round <= 0:
            raise ValueError("seed must be 0..2^63-1; round must be positive")
        if not finite_number(args.timeout, positive=True):
            raise ValueError("timeout must be finite and positive")
        if args.arrival == "closed" and args.arrival_rate is not None:
            raise ValueError("arrival-rate requires constant or poisson arrivals")
        if args.arrival != "closed" and not finite_number(args.arrival_rate, positive=True):
            raise ValueError("open-loop arrivals require a finite positive arrival-rate")
        if args.trace and args.arrival != "closed":
            raise ValueError("trace offsets replace the arrival generator")
        if args.abba_position is not None and not args.comparison_group:
            raise ValueError("abba-position requires comparison-group")
        if bool(args.cache_metrics_url) != bool(args.cache_counter):
            raise ValueError("cache-metrics-url and cache-counter must be supplied together")
        for key in ("ttft", "tpot", "e2e"):
            value = getattr(args, f"slo_{key}_ms")
            if value is not None and not finite_number(value):
                raise ValueError("SLO targets must be finite and nonnegative")
        for url in (args.base_url, args.ingest, args.cache_metrics_url):
            if url:
                validate_url(url)
        if args.token and args.token_env:
            raise ValueError("choose token or token-env")
        if args.token_env:
            args.token = os.environ.get(args.token_env)
            if not args.token:
                raise ValueError("token environment variable is empty")
        args.run_id = args.run_id or f"bench-{uuid.uuid4().hex[:12]}"
        args.engine_capability = load_engine_capability(args)
        return args, workload
    except (OSError, ValueError, KeyError, TypeError):
        parser.error("invalid benchmark configuration; check workload, URLs, counts, and arrival/SLO options")


def main(argv: list[str] | None = None) -> int:
    args, workload = parse_args(argv)
    try:
        if args.out and Path(args.out).exists():
            raise ValueError("output already exists")
        if args.artifacts_dir and any(
            (Path(args.artifacts_dir) / filename).exists()
            for filename in ("samples.jsonl", "manifest.json")
        ):
            raise ValueError("evidence already exists")
        plan = request_plan(args, workload)
        workload["request_count"] = len(plan)
        trace_hash = hash_plan(plan)
        before = None if args.dry_run else cache_snapshot(args)
        if args.dry_run:
            now = int(time.time() * 1000)
            metrics = {
                "samples": [empty_sample(spec, "synthetic") for spec in plan],
                "wall_time_s": 0.0, "started_at_ms": now, "finished_at_ms": now,
            }
        else:
            metrics = run_requests(args, plan)
        after = None if args.dry_run else cache_snapshot(args)
        raw = b"".join(encoded_json(row) + b"\n" for row in metrics["samples"])
        run = build_run(args, workload, metrics, trace_hash, hashlib.sha256(raw).hexdigest())
        delta = None
        if before is not None and after is not None and all(after[key] >= before[key] for key in before):
            delta = {key: after[key] - before[key] for key in before}
        run["evidence"]["cache_counter_delta"] = delta
        artifacts = Path(args.artifacts_dir) if args.artifacts_dir else Path(tempfile.mkdtemp(prefix="nebula-bench-"))
        artifacts.mkdir(parents=True, exist_ok=True)
        with (artifacts / "samples.jsonl").open("xb") as file:
            file.write(raw)
        manifest = {
            "run": run, "cache_counters_before": before, "cache_counters_after": after,
            "sampling": {"temperature": 0, "seed": args.seed},
            "measurement": "client-arrival; TTFT=first-generated-delta; TPOT=(last-first)/(tokens-1)",
        }
        with (artifacts / "manifest.json").open("xb") as file:
            file.write(encoded_json(manifest) + b"\n")
        text = json.dumps(run, indent=2, allow_nan=False)
        if args.out:
            with Path(args.out).open("x") as file:
                file.write(text + "\n")
        else:
            print(text)
        print(f"evidence: {artifacts}", file=sys.stderr)
        if args.ingest:
            headers = {"Content-Type": "application/json"}
            if args.token:
                headers["Authorization"] = f"Bearer {args.token}"
            with urlopen(Request(args.ingest, data=encoded_json(run), headers=headers), timeout=args.timeout) as response:
                print(f"ingest HTTP {response.status}", file=sys.stderr)
        return 0 if args.dry_run or run["status"] == "succeeded" else 1
    except (OSError, ValueError, KeyError, TypeError, http.client.HTTPException):
        print("benchmark failed: invalid trace, network/ingest failure, or unwritable/existing output", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
