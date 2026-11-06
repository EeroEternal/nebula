#!/usr/bin/env bash
# Transport A/B: the same workload against the same mock engine, once per implementation
# of the engine seam (`reqwest` vs `xlite`), then a diff.
#
# What this proves: the transport swap does not change semantics and does not cost more
# per stream (TTFT, throughput, RSS, FDs, dials/redials).
# What this cannot prove: anything about real prefill/decode behaviour or KV reuse — that
# needs a real vLLM/SGLang, and a two-deployment run of the gateway (transport selection
# is per process).
#
# Usage:
#   ./scripts/ab_upstream_transport.sh                # 300 requests, 16 concurrent
#   AB_REQUESTS=1000 AB_CONCURRENCY=32 ./scripts/ab_upstream_transport.sh
#   AB_TRANSPORTS="reqwest" ./scripts/ab_upstream_transport.sh   # single baseline
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PORT="${AB_PORT:-0}"   # 0 = pick a free port; a stale mock on a fixed port silently hijacks the run
AB_REQUESTS="${AB_REQUESTS:-300}"
AB_CONCURRENCY="${AB_CONCURRENCY:-16}"
AB_TRANSPORTS="${AB_TRANSPORTS:-reqwest xlite}"
# 64 tokens @ 2 ms = a ~128 ms stream: short enough to run hundreds, long enough that
# first-byte and per-chunk behaviour both show up in the percentiles.
MOCK_TOKENS="${MOCK_TOKENS:-64}"
MOCK_CHUNK_MS="${MOCK_CHUNK_MS:-2}"
MOCK_FIRST_BYTE_MS="${MOCK_FIRST_BYTE_MS:-20}"

LOG="$(mktemp -t ab-mock.XXXXXX.log)"
MOCK_PID=""
cleanup() {
  [ -n "$MOCK_PID" ] && kill "$MOCK_PID" 2>/dev/null || true
}
trap cleanup EXIT

if [ "$PORT" = "0" ]; then
  PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
fi
MOCK_CHUNKED=1 MOCK_TOKENS="$MOCK_TOKENS" MOCK_CHUNK_MS="$MOCK_CHUNK_MS" \
  MOCK_FIRST_BYTE_MS="$MOCK_FIRST_BYTE_MS" \
  python3 scripts/mock_openai_engine.py "$PORT" 127.0.0.1 >"$LOG" 2>&1 &
MOCK_PID=$!
for _ in $(seq 50); do
  curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break
  if ! kill -0 "$MOCK_PID" 2>/dev/null; then
    echo "mock engine exited (port $PORT taken?); see $LOG" >&2
    cat "$LOG" >&2
    exit 1
  fi
  sleep 0.1
done
curl -sf "http://127.0.0.1:$PORT/health" >/dev/null || { echo "mock engine did not start; see $LOG" >&2; exit 1; }
# Nothing may have been served yet: a non-zero counter here means we are talking to
# something else (a stale mock from an earlier run).
served="$(curl -s "http://127.0.0.1:$PORT/__mock_stats")"
case "$served" in
  *'"requests": 0'*) ;;
  *) echo "mock on port $PORT already served requests ($served) — not a fresh instance" >&2; exit 1 ;;
esac

echo "== transport A/B =="
echo "   mock: port=$PORT tokens=$MOCK_TOKENS chunk_ms=$MOCK_CHUNK_MS first_byte_ms=$MOCK_FIRST_BYTE_MS"
echo "   load: requests=$AB_REQUESTS concurrency=$AB_CONCURRENCY transports=[$AB_TRANSPORTS]"
echo

RESULTS=()
for transport in $AB_TRANSPORTS; do
  before="$(curl -s "http://127.0.0.1:$PORT/__mock_stats")"
  out="$(AB_TRANSPORT="$transport" AB_BASE_URL="http://127.0.0.1:$PORT" \
        AB_REQUESTS="$AB_REQUESTS" AB_CONCURRENCY="$AB_CONCURRENCY" \
        cargo test -q -p nebula-common --features xlite --test upstream_ab -- \
          --ignored --nocapture 2>&1)"
  line="$(printf '%s\n' "$out" | grep -o 'AB_RESULT .*' | head -1 || true)"
  if [ -z "$line" ]; then
    echo "!! $transport probe produced no result" >&2
    printf '%s\n' "$out" | tail -20 >&2
    exit 1
  fi
  after="$(curl -s "http://127.0.0.1:$PORT/__mock_stats")"
  RESULTS+=("${line#AB_RESULT }")
  echo "  $transport mock-side counters: before=$before after=$after"
done

echo
echo "== result =="
printf '%s\n' "${RESULTS[@]}" | python3 -c '
import json, sys
rows = [json.loads(l) for l in sys.stdin if l.strip()]
fields = [
    ("ttft_ms_p50", "TTFT p50 ms", "{:.1f}"),
    ("ttft_ms_p99", "TTFT p99 ms", "{:.1f}"),
    ("total_ms_p50", "stream p50 ms", "{:.1f}"),
    ("total_ms_p99", "stream p99 ms", "{:.1f}"),
    ("rps", "throughput rps", "{:.1f}"),
    ("rss_peak_mb", "RSS peak MB", "{:.1f}"),
    ("fds_peak_growth", "FD growth", "{}"),
    ("pool_dials", "pool dials", "{}"),
    ("pool_connections", "pool conns", "{}"),
    ("pool_redials", "redials", "{}"),
]
width = max(len(label) for _, label, _ in fields) + 2
header = "metric".ljust(width) + "".join(r["transport"].rjust(14) for r in rows)
print(header)
print("-" * len(header))
for key, label, fmt in fields:
    cells = []
    for r in rows:
        v = r.get(key)
        cells.append(("-" if v is None else fmt.format(v)).rjust(14))
    print(label.ljust(width) + "".join(cells))
'
