# Benchmark runner (P5)

`run_benchmark.py` exercises an OpenAI-compatible `/v1/chat/completions` endpoint and emits a `BenchmarkRun` JSON matching `nebula_common::BenchmarkRun`.

## Workloads

- `workloads/short-chat-v1.json`
- `workloads/long-context-v1.json`

## Dry-run (no GPU / no server)

```bash
python3 scripts/benchmark/run_benchmark.py --dry-run \
  --model Qwen/Qwen2.5-0.5B-Instruct --engine vllm --workload short-chat-v1 \
  --out /tmp/bench-run.json
```

## Live + ingest to BFF

```bash
python3 scripts/benchmark/run_benchmark.py \
  --base-url http://127.0.0.1:8081/v1 \
  --model demo --engine vllm --workload short-chat-v1 \
  --token dev-token \
  --ingest http://127.0.0.1:18090/api/v2/benchmarks/runs
```

BFF rebuilds the performance profile and exposes it via `POST /api/v2/benchmarks/recommend`.
