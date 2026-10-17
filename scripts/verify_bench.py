#!/usr/bin/env python3
"""verify_bench.py — Client-side streaming benchmark for Nebula Gateway/Router.

Measures client-observed TTFT, end-to-end latency, HTTP rejection rate (429/5xx),
and SLO-compliant goodput across prefix-sharing workloads (replicated vs
prefix_affinity vs pd_split).

Example:
  python3 scripts/verify_bench.py \\
    --url http://127.0.0.1:8081/v1/completions \\
    --model qwen2.5-7b-nebula \\
    --prefixes 96 --prefix-words 1500 --suffix-words 96 \\
    --requests 384 --concurrency 32 \\
    --slo-ttft-ms 2000 --slo-e2e-ms 5000 \\
    --raw-out /tmp/bench_samples.jsonl
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import time
import urllib.error
import urllib.request
from typing import Any

VOCAB = (
    "capacity planning gpu kv cache transfer disaggregated prefill decode "
    "lmcache remote tcp router affinity block hash replica scheduling "
    "throughput latency benchmark slo admission queue fairness token stream "
).split()


def build_prompt(prefix_idx: int, prefix_words: int, suffix_idx: int, suffix_words: int) -> str:
    prefix = [f"p{prefix_idx}"] + [
        VOCAB[(prefix_idx * 31 + i) % len(VOCAB)] for i in range(max(0, prefix_words - 1))
    ]
    suffix = [f"s{suffix_idx}"] + [
        VOCAB[(suffix_idx * 17 + i) % len(VOCAB)] for i in range(max(0, suffix_words - 1))
    ]
    return " ".join(prefix + suffix)


def percentile(sorted_vals: list[float], pct: float) -> float:
    if not sorted_vals:
        return float("nan")
    idx = min(len(sorted_vals) - 1, max(0, math.ceil((pct / 100.0) * len(sorted_vals)) - 1))
    return sorted_vals[idx]


def run_one(
    req_id: int,
    url: str,
    model: str,
    prompt: str,
    max_tokens: int,
    api_key: str | None,
    timeout_s: float,
) -> dict[str, Any]:
    payload = json.dumps(
        {
            "model": model,
            "prompt": prompt,
            "max_tokens": max_tokens,
            "temperature": 0,
            "stream": True,
        }
    ).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"

    req = urllib.request.Request(url, data=payload, headers=headers, method="POST")
    t0 = time.perf_counter()
    ttft_ms: float | None = None
    status = 0
    error_detail: str | None = None

    try:
        saw_done = False
        with urllib.request.urlopen(req, timeout=timeout_s) as resp:
            status = resp.status
            for raw_line in resp:
                line = raw_line.decode("utf-8", errors="replace").strip()
                if not line or not line.startswith("data:"):
                    continue
                data_str = line[5:].strip()
                if data_str == "[DONE]":
                    saw_done = True
                    break
                try:
                    frame = json.loads(data_str)
                except json.JSONDecodeError:
                    status = 502
                    error_detail = "Malformed SSE JSON frame"
                    break
                if isinstance(frame, dict) and frame.get("error"):
                    status = 502
                    error_detail = f"SSE error frame: {frame['error']}"
                    break
                if ttft_ms is None:
                    ttft_ms = (time.perf_counter() - t0) * 1000.0
        if status == 200 and not saw_done and error_detail is None:
            status = 502
            error_detail = "SSE stream ended without [DONE]"
    except urllib.error.HTTPError as e:
        status = e.code
        error_detail = f"HTTP {e.code}"
    except Exception as e:  # noqa: BLE001
        status = 599
        error_detail = f"{type(e).__name__}: {e}"

    e2e_ms = (time.perf_counter() - t0) * 1000.0

    return {
        "req_id": req_id,
        "status": status,
        "ttft_ms": round(ttft_ms, 2) if ttft_ms is not None else None,
        "e2e_ms": round(e2e_ms, 2),
        "error": error_detail,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--url", default="http://127.0.0.1:8081/v1/completions")
    ap.add_argument("--model", default="qwen2.5-7b-nebula")
    ap.add_argument("--api-key", default="")
    ap.add_argument("--prefixes", type=int, default=96)
    ap.add_argument(
        "--prefix-tokens",
        "--prefix-words",
        dest="prefix_words",
        type=int,
        default=2048,
        help="Prefix length in single-token vocabulary words (default: 2048)",
    )
    ap.add_argument(
        "--suffix-tokens",
        "--suffix-words",
        dest="suffix_words",
        type=int,
        default=128,
        help="Suffix length in single-token vocabulary words (default: 128)",
    )
    ap.add_argument("--max-tokens", type=int, default=16)
    ap.add_argument("--requests", type=int, default=384)
    ap.add_argument("--concurrency", type=int, default=32)
    ap.add_argument("--timeout-s", type=float, default=60.0)
    ap.add_argument(
        "--slo-ttft-ms",
        type=float,
        default=3000.0,
        help="Max TTFT (ms) for a request to count toward goodput (default: 3000)",
    )
    ap.add_argument(
        "--slo-e2e-ms",
        type=float,
        default=10000.0,
        help="Max end-to-end latency (ms) for a request to count toward goodput (default: 10000)",
    )
    ap.add_argument("--raw-out", default="", help="Optional path to write per-request JSONL samples")
    args = ap.parse_args()

    prompts = [
        build_prompt(i % args.prefixes, args.prefix_words, i, args.suffix_words)
        for i in range(args.requests)
    ]

    wall_t0 = time.perf_counter()
    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        futs = [
            pool.submit(
                run_one,
                i,
                args.url,
                args.model,
                prompts[i],
                args.max_tokens,
                args.api_key or None,
                args.timeout_s,
            )
            for i in range(args.requests)
        ]
        for fut in concurrent.futures.as_completed(futs):
            results.append(fut.result())
    wall_s = max(1e-6, time.perf_counter() - wall_t0)
    results.sort(key=lambda r: r["req_id"])

    if args.raw_out:
        with open(args.raw_out, "w", encoding="utf-8") as f:
            for row in results:
                f.write(json.dumps(row) + "\n")

    ok_rows = [
        r
        for r in results
        if r["status"] == 200 and r["ttft_ms"] is not None and r["error"] is None
    ]
    rejected_429 = sum(1 for r in results if r["status"] == 429)
    failed_other = len(results) - len(ok_rows) - rejected_429
    slo_ok = [
        r
        for r in ok_rows
        if r["ttft_ms"] <= args.slo_ttft_ms and r["e2e_ms"] <= args.slo_e2e_ms
    ]

    ttfts = sorted(r["ttft_ms"] for r in ok_rows)
    e2es = sorted(r["e2e_ms"] for r in ok_rows)

    summary = {
        "requests": len(results),
        "ok": len(ok_rows),
        "rejected_429": rejected_429,
        "failed_other": failed_other,
        "rejection_rate_pct": round((rejected_429 + failed_other) * 100.0 / max(1, len(results)), 2),
        "wall_s": round(wall_s, 2),
        "throughput_rps": round(len(ok_rows) / wall_s, 2),
        "goodput_rps": round(len(slo_ok) / wall_s, 2),
        "slo_ttft_ms": args.slo_ttft_ms,
        "slo_e2e_ms": args.slo_e2e_ms,
        "ttft_p50_ms": round(percentile(ttfts, 50), 1),
        "ttft_p99_ms": round(percentile(ttfts, 99), 1),
        "ttft_p999_ms": round(percentile(ttfts, 99.9), 1),
        "e2e_p50_ms": round(percentile(e2es, 50), 1),
        "e2e_p99_ms": round(percentile(e2es, 99), 1),
    }
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
