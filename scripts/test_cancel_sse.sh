#!/usr/bin/env bash
# A3: Cancel / SSE abort contract check (manual or CI with a live Router).
#
# SLO: client disconnect during SSE increments nebula_router_requests_aborted_total
# and must NOT increment status_5xx / model 5xx counters.
#
# Uses a hard TCP close mid-stream (curl --max-time often loses the race on fast
# engines that finish before the deadline).
#
# Usage:
#   ROUTER_URL=http://127.0.0.1:18081 MODEL=my-model ./scripts/test_cancel_sse.sh
set -euo pipefail

ROUTER_URL="${ROUTER_URL:-http://127.0.0.1:18081}"
MODEL="${MODEL:-}"
TOKEN="${NEBULA_AUTH_TOKENS:-}"
METRICS_URL="${METRICS_URL:-${ROUTER_URL}/metrics}"

if [[ -z "${MODEL}" ]]; then
  echo "MODEL is required (model_uid or model_name served by the cluster)" >&2
  exit 2
fi

metric() {
  local name="$1"
  curl -fsS "${METRICS_URL}" | awk -v n="$name" '$1==n {print $2; found=1} END{if(!found) print 0}'
}

before_abort="$(metric nebula_router_requests_aborted_total)"
before_5xx="$(metric nebula_router_responses_5xx)"

echo "before abort=${before_abort} 5xx=${before_5xx}"

# Hard-close after ~4KiB so we abort while the engine is still producing tokens.
python3 - "$ROUTER_URL" "$MODEL" "${TOKEN%%,*}" <<'PY'
import http.client, json, sys, time
from urllib.parse import urlparse

base, model, token = sys.argv[1], sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else ""
u = urlparse(base)
host, port = u.hostname or "127.0.0.1", u.port or (443 if u.scheme == "https" else 80)
path = (u.path or "").rstrip("/") + "/v1/chat/completions"
body = json.dumps({
    "model": model,
    "stream": True,
    "max_tokens": 2048,
    "messages": [{"role": "user", "content": "count from 1 to 5000 slowly, one number per line"}],
}).encode()
conn = http.client.HTTPConnection(host, port, timeout=120)
conn.putrequest("POST", path)
conn.putheader("Content-Type", "application/json")
conn.putheader("Content-Length", str(len(body)))
if token:
    conn.putheader("Authorization", f"Bearer {token}")
conn.endheaders()
conn.send(body)
resp = conn.getresponse()
ct = resp.getheader("content-type") or ""
if resp.status != 200 or "text/event-stream" not in ct:
    snippet = resp.read(400)
    raise SystemExit(f"expected SSE 200, got {resp.status} ct={ct!r} body={snippet!r}")
n = 0
t0 = time.time()
while True:
    chunk = resp.read(128)
    if not chunk:
        raise SystemExit(f"upstream finished before abort (bytes={n})")
    n += len(chunk)
    if n >= 4096:
        conn.close()
        print(f"hard-closed after {n} bytes in {time.time()-t0:.3f}s", flush=True)
        break
time.sleep(1.0)
PY

after_abort="$(metric nebula_router_requests_aborted_total)"
after_5xx="$(metric nebula_router_responses_5xx)"

echo "after  abort=${after_abort} 5xx=${after_5xx}"

python3 - <<PY
before_abort=float("${before_abort}")
after_abort=float("${after_abort}")
before_5xx=float("${before_5xx}")
after_5xx=float("${after_5xx}")
assert after_abort >= before_abort + 1, f"abort metric did not increase: {before_abort} -> {after_abort}"
assert after_5xx <= before_5xx, f"5xx must not increase on abort: {before_5xx} -> {after_5xx}"
print("OK: abort +1 and 5xx unchanged (SLO)")
PY
