# Benchmark runner v2

Standard-library Python client for OpenAI-compatible streaming chat. It emits a
backward-compatible `BenchmarkRun` summary plus versioned evidence. See
[`docs/dev/benchmark.md`](../../docs/dev/benchmark.md) for the evidence contract.

## Dry-run, no network and never selection evidence

```bash
RUN_DIR="$(mktemp -d)"
python3 scripts/benchmark/run_benchmark.py --dry-run --model demo \
  --out "$RUN_DIR/run.json" --artifacts-dir "$RUN_DIR/evidence"
```

Dry-run returns `insufficient_data`, `evidence.synthetic=true`, and null performance
metrics. Custom notes cannot hide its synthetic status.
Combining `--dry-run` with `--ingest` is rejected before any network I/O.

## Streaming, actual concurrency, optional SLO and GPU normalization

```bash
RUN_DIR="$(mktemp -d)"
python3 scripts/benchmark/run_benchmark.py \
  --base-url http://127.0.0.1:8081/v1 --model demo --engine vllm \
  --workload short-chat-v1 --concurrency 4 --requests 40 --max-tokens 64 \
  --seed 42 --gpu-count 1 --cache-state warm \
  --slo-ttft-ms 500 --slo-tpot-ms 50 --slo-e2e-ms 5000 \
  --out "$RUN_DIR/run.json" --artifacts-dir "$RUN_DIR/evidence"
```

`--cache-state` is an operator declaration, not automatic warming or cache clearing.
Only pass `warm` after explicit unmeasured warm-up. Omit `--gpu-count` when unknown:
per-GPU throughput and GPU-time cost then stay null. GPU-time cost is a proxy, not
dollars. Use `--token-env BENCH_TOKEN` for authentication; tokens are not archived.

`--workload-file` accepts a custom workload with `id`, `class`, `prompt`,
`max_tokens`, `concurrency`, and `request_count`. Built-in workloads are under
`workloads/`. The runner requests real usage; missing usage is unknown, never an
estimated token count.

## Open-loop arrivals and replay

Add `--arrival constant --arrival-rate 4` or
`--arrival poisson --arrival-rate 4`. Poisson offsets are reproducible for a fixed
seed. When concurrency is exhausted, the generator records `client_overloaded`
rather than waiting and silently changing the offered load.

Alternatively pass `--trace trace.jsonl`, with one request per line:

```json
{"offset_s":0.0,"prompt":"Say hello.","max_tokens":32}
{"offset_s":0.25,"prompt":"Name a primary color.","max_tokens":16}
```

Offsets must be ordered and nonnegative. Trace entries replace the workload's
request count/token limits; do not combine with `--requests`, `--max-tokens`, or
open-loop generation options. Preserve the source trace for replay: the manifest
stores its normalized SHA-256, not a copy of potentially sensitive prompts.

## Evidence, cache deltas, and ABBA

- `samples.jsonl`: per-request timings, authoritative usage, finish/error reason,
  and scheduling delay. No generated text, headers, or bearer tokens.
- `manifest.json`: summary, sampling settings, cache snapshots, image/version
  labels, seed, load fingerprint, source/sample hashes, and comparison labels.
- Output files are created exclusively; existing evidence is never overwritten.
  With no `--artifacts-dir`, a fresh temporary directory is reported on stderr.
- Optional cache measurement:
  `--cache-metrics-url http://127.0.0.1:8000/metrics --cache-counter vllm:prefix_cache_hits_total`.
  Repeat `--cache-counter` for other counters. Labels are summed, so choose a
  dedicated endpoint. Missing samples or counter resets produce a null delta.
- For an externally orchestrated ABBA comparison, run four times with a shared
  `--comparison-group`, `--round`, and `--abba-position 0`, `1`, `2`, `3`.
  Positions map to A/B/B/A; choose the appropriate endpoint/config for each run.
  Labels do not switch endpoints or establish statistical significance.

Optional `--ingest http://127.0.0.1:18090/api/v2/benchmarks/runs` stores the summary
and evidence metadata in BFF Postgres JSONB. Archive raw files separately; the BFF
does not upload them. Synthetic/failed runs are excluded from profiles, and stale
profiles containing known synthetic evidence are sanitized before recommendation.

## Optional low-precision declaration

`--capability-file capability.json` archives a selection-related `EngineCapability`
subset, for example:

```json
{
  "engine_type": "vllm",
  "engine_version": "0.19.0",
  "source": "manual_override",
  "quantization": "w4a4",
  "native_low_precision_paths": {"w4a4": "supported"}
}
```

This is an example assertion, not a claim that a particular GPU/engine supports
W4A4. Only supply a declaration verified for this model and runtime. Also set
nonempty `--engine-version`, `--image-id`, and `--platform`; the snapshot engine
and version must match. The file is capped at 64 KiB and validated before network
I/O. Accepted formats: `w4a4`, `w4a16`, `fp8`, `w8a8`, `int8`; path states:
`supported`, `unsupported`, `unknown`. Absent paths are unknown, not unsupported.

Only the five fields above are archived, never arbitrary notes. The declaration
is included in the load fingerprint and is not sent to the inference engine.
Selection applies it only within matching run evidence, not across platforms or
images. There is no automatic hardware/semantic probe in this option. Unknown or
conflicting declarations receive no Roofline adjustment; the remaining numeric
rules are uncalibrated ranking heuristics, not measured speedups.

## Tests and exit codes

```bash
python3 -m unittest discover -s scripts/benchmark -p 'test_*.py'
```

These tests also run in `scripts/ci.sh`. Exit `0`: real succeeded run or explicit
dry-run; `1`: failed/insufficient real run, with evidence saved; `2`: invalid
configuration, trace, output, or ingestion failure. Any failed request makes a
real run `failed`, preventing a partially failed benchmark from becoming selection
evidence. Successful responses without usable usage/output are `insufficient_data`.

All address connection attempts, the TLS handshake, and each underlying socket
read/write (including response headers) share one remaining request budget.
A slowly dribbling header cannot reset it. OS hostname
resolution is not interruptible by Python's socket timeout; use an IP endpoint or
a resolver with its own bounded policy when a hard DNS deadline is required.
