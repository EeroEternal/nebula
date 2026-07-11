#!/usr/bin/env bash
# A3: Cancel / SSE abort contract check (manual or CI with a live Router).
#
# SLO: client disconnect during SSE increments nebula_router_requests_aborted_total
# and must NOT increment status_5xx / model 5xx counters.
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

auth_hdr=()
if [[ -n "${TOKEN}" ]]; then
  # Use first token if comma-separated
  t="${TOKEN%%,*}"
  auth_hdr=(-H "Authorization: Bearer ${t}")
fi

metric() {
  local name="$1"
  curl -fsS "${METRICS_URL}" | awk -v n="$name" '$1==n {print $2; found=1} END{if(!found) print 0}'
}

before_abort="$(metric nebula_router_requests_aborted_total)"
before_5xx="$(metric nebula_router_responses_5xx)"

echo "before abort=${before_abort} 5xx=${before_5xx}"

# Long streaming request; kill client after 2s to force abort.
set +e
curl -NsS --max-time 2 \
  "${auth_hdr[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"model\":\"${MODEL}\",\"stream\":true,\"max_tokens\":2048,\"messages\":[{\"role\":\"user\",\"content\":\"count slowly to 200\"}]}" \
  "${ROUTER_URL}/v1/chat/completions" >/dev/null 2>&1
set -e

sleep 1
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
