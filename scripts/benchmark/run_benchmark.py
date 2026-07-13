#!/usr/bin/env python3
"""Nebula benchmark runner (P5).

Runs a workload against an OpenAI-compatible endpoint and emits a BenchmarkRun JSON
compatible with `nebula_common::BenchmarkRun` / BFF ingest.

Examples:
  # Dry-run (no network) — synthesizes a succeeded run for schema validation
  python3 scripts/benchmark/run_benchmark.py --dry-run \\
    --model Qwen/Qwen2.5-0.5B-Instruct --engine vllm --workload short-chat-v1

  # Live run against a local mock / gateway
  python3 scripts/benchmark/run_benchmark.py \\
    --base-url http://127.0.0.1:8081/v1 \\
    --model demo --engine vllm --workload short-chat-v1 \\
    --token dev-token --out /tmp/run.json
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
import time
import uuid
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
WORKLOAD_DIR = Path(__file__).resolve().parent / "workloads"


def load_workload(workload_id: str) -> dict[str, Any]:
    path = WORKLOAD_DIR / f"{workload_id}.json"
    if not path.exists():
        raise SystemExit(f"workload not found: {path}")
    return json.loads(path.read_text())


def percentile(values: list[float], p: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    k = (len(ordered) - 1) * (p / 100.0)
    f = int(k)
    c = min(f + 1, len(ordered) - 1)
    if f == c:
        return ordered[f]
    return ordered[f] + (ordered[c] - ordered[f]) * (k - f)


def chat_once(
    base_url: str,
    model: str,
    prompt: str,
    max_tokens: int,
    token: str | None,
    timeout_s: float,
) -> tuple[float, float, int, bool, str | None]:
    """Returns (ttft_ms, total_ms, completion_tokens, ok, err)."""
    url = base_url.rstrip("/") + "/chat/completions"
    body = {
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
        "stream": False,
    }
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    data = json.dumps(body).encode()
    req = Request(url, data=data, headers=headers, method="POST")
    t0 = time.perf_counter()
    try:
        with urlopen(req, timeout=timeout_s) as resp:
            raw = resp.read()
        t1 = time.perf_counter()
        payload = json.loads(raw.decode())
        usage = payload.get("usage") or {}
        tokens = int(usage.get("completion_tokens") or max_tokens // 2)
        total_ms = (t1 - t0) * 1000.0
        # Non-stream: approximate TTFT as total latency (documented limitation).
        return total_ms, total_ms, tokens, True, None
    except HTTPError as e:
        t1 = time.perf_counter()
        return (t1 - t0) * 1000.0, (t1 - t0) * 1000.0, 0, False, f"http_{e.code}"
    except (URLError, TimeoutError, json.JSONDecodeError) as e:
        t1 = time.perf_counter()
        return (t1 - t0) * 1000.0, (t1 - t0) * 1000.0, 0, False, str(e)


def dry_run_metrics(request_count: int) -> dict[str, Any]:
    # Deterministic synthetic numbers for CI / schema checks.
    ttfts = [120.0 + i * 3.0 for i in range(request_count)]
    totals = [tt + 80.0 for tt in ttfts]
    tokens = [48 for _ in range(request_count)]
    return {
        "ttfts": ttfts,
        "totals": totals,
        "tokens": tokens,
        "errors": 0,
        "ok": request_count,
    }


def build_run(args: argparse.Namespace, workload: dict[str, Any], metrics: dict[str, Any]) -> dict[str, Any]:
    now = int(time.time() * 1000)
    ok = metrics["ok"]
    err = metrics["errors"]
    n = max(ok + err, 1)
    total_tokens = sum(metrics["tokens"])
    duration_s = max(sum(metrics["totals"]) / 1000.0, 1e-6)
    throughput = total_tokens / duration_s if ok else None
    ttft_p50 = percentile(metrics["ttfts"], 50)
    ttft_p95 = percentile(metrics["ttfts"], 95)
    # Approximate TPOT from (total - ttft) / tokens
    tpots: list[float] = []
    for ttft, total, tok in zip(metrics["ttfts"], metrics["totals"], metrics["tokens"]):
        if tok > 0:
            tpots.append(max(total - ttft, 0.0) / tok)
    tpot_p95 = percentile(tpots, 95) if tpots else None
    cost = None
    if throughput and throughput > 0:
        # GPU-second per 1k tokens proxy (assumes 1 GPU fully busy during wall time).
        cost = (duration_s / max(total_tokens, 1)) * 1000.0

    status = "succeeded" if ok > 0 and err / n < 1.0 else "failed"
    if ok == 0:
        status = "insufficient_data"

    return {
        "run_id": args.run_id or f"bench-{uuid.uuid4().hex[:12]}",
        "profile_key": {
            "model_name": args.model,
            "engine_type": args.engine,
            "engine_version": args.engine_version,
            "platform": args.platform,
            "gpu_name": args.gpu_name,
            "workload_id": workload["id"],
            "param_fingerprint": args.param_fingerprint,
        },
        "workload": workload,
        "status": status,
        "base_url": args.base_url,
        "image_id": args.image_id,
        "software_version": args.software_version,
        "ttft_p50_ms": ttft_p50,
        "ttft_p95_ms": ttft_p95,
        "tpot_p95_ms": tpot_p95,
        "throughput_tps": throughput,
        "error_rate": err / n,
        "peak_vram_mb": args.peak_vram_mb,
        "cost_per_1k_tokens": cost,
        "started_at_ms": now - int(duration_s * 1000),
        "finished_at_ms": now,
        "error_message": None if ok else "all requests failed",
        "evidence_notes": args.notes
        or ("dry-run synthetic metrics" if args.dry_run else "non-stream TTFT≈E2E"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Nebula P5 benchmark runner")
    parser.add_argument("--workload", default="short-chat-v1")
    parser.add_argument("--model", required=True)
    parser.add_argument("--engine", default="vllm", choices=["vllm", "sglang"])
    parser.add_argument("--engine-version", default=None)
    parser.add_argument("--platform", default="nvidia-cuda")
    parser.add_argument("--gpu-name", default=None)
    parser.add_argument("--base-url", default="http://127.0.0.1:8081/v1")
    parser.add_argument("--token", default=None)
    parser.add_argument("--image-id", default=None)
    parser.add_argument("--software-version", default="nebula-dev")
    parser.add_argument("--param-fingerprint", default=None)
    parser.add_argument("--peak-vram-mb", type=int, default=None)
    parser.add_argument("--timeout", type=float, default=60.0)
    parser.add_argument("--run-id", default=None)
    parser.add_argument("--notes", default=None)
    parser.add_argument("--out", default=None, help="Write BenchmarkRun JSON to path")
    parser.add_argument("--ingest", default=None, help="POST run JSON to BFF, e.g. http://127.0.0.1:18090/api/v2/benchmarks/runs")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    workload = load_workload(args.workload)
    request_count = int(workload.get("request_count", 20))
    max_tokens = int(workload.get("max_tokens", 64))
    prompt = workload["prompt"]

    if args.dry_run:
        metrics = dry_run_metrics(request_count)
    else:
        ttfts: list[float] = []
        totals: list[float] = []
        tokens: list[int] = []
        errors = 0
        ok = 0
        for _ in range(request_count):
            ttft, total, tok, success, _err = chat_once(
                args.base_url, args.model, prompt, max_tokens, args.token, args.timeout
            )
            if success:
                ok += 1
                ttfts.append(ttft)
                totals.append(total)
                tokens.append(tok)
            else:
                errors += 1
                totals.append(total)
        metrics = {
            "ttfts": ttfts,
            "totals": totals,
            "tokens": tokens,
            "errors": errors,
            "ok": ok,
        }

    run = build_run(args, workload, metrics)
    text = json.dumps(run, indent=2)
    if args.out:
        Path(args.out).write_text(text + "\n")
        print(f"wrote {args.out}", file=sys.stderr)
    else:
        print(text)

    if args.ingest:
        data = json.dumps(run).encode()
        headers = {"Content-Type": "application/json"}
        if args.token:
            headers["Authorization"] = f"Bearer {args.token}"
        req = Request(args.ingest, data=data, headers=headers, method="POST")
        try:
            with urlopen(req, timeout=30) as resp:
                print(f"ingest HTTP {resp.status}", file=sys.stderr)
        except Exception as e:  # noqa: BLE001
            print(f"ingest failed: {e}", file=sys.stderr)
            return 2
    return 0 if run["status"] in ("succeeded", "insufficient_data") else 1


if __name__ == "__main__":
    raise SystemExit(main())
