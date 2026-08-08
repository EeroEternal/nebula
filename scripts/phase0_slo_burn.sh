#!/usr/bin/env bash
# Phase 0: light SLO burn — traffic, etcd ModelSlo + offline evaluate, abort contract.
#
# Does not require BFF/Postgres. Writes /slos/{MODEL} and /models/{MODEL}/spec,
# scrapes Router metrics, mirrors evaluate_slo low-traffic rules, then runs
# scripts/test_cancel_sse.sh against Router (abort +1, 5xx flat).
#
# Usage:
#   MODEL=qwen35-4b-sglang \
#   GATEWAY_URL=http://127.0.0.1:8081 \
#   ROUTER_URL=http://127.0.0.1:18081 \
#   TOKEN=tokB \
#   ./scripts/phase0_slo_burn.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GATEWAY_URL="${GATEWAY_URL:-http://127.0.0.1:8081}"
ROUTER_URL="${ROUTER_URL:-http://127.0.0.1:18081}"
MODEL="${MODEL:-}"
TOKEN="${TOKEN:-tokB}"
ETCDCTL_BIN="${ETCDCTL_BIN:-etcdctl}"
command -v "${ETCDCTL_BIN}" >/dev/null 2>&1 || ETCDCTL_BIN="${HOME}/bin/etcdctl"
TRAFFIC_N="${TRAFFIC_N:-12}"

if [[ -z "${MODEL}" ]]; then
  echo "MODEL is required" >&2
  exit 2
fi

now_ms() {
  python3 -c 'import time; print(int(time.time()*1000))'
}

ts="$(now_ms)"

echo "== ensure ModelSpec + ModelSlo in etcd =="
spec=$(cat <<EOF
{"model_uid":"${MODEL}","model_name":"${MODEL}","model_source":"local","engine_type":"sglang","updated_at_ms":${ts}}
EOF
)
# ModelSpec may require more fields — use minimal known shape.
# Prefer keep existing spec if present.
if ! "${ETCDCTL_BIN}" get "/models/${MODEL}/spec" --print-value-only 2>/dev/null | grep -q model_uid; then
  "${ETCDCTL_BIN}" put "/models/${MODEL}/spec" "${spec}" >/dev/null
  echo "wrote /models/${MODEL}/spec"
else
  echo "kept existing /models/${MODEL}/spec"
fi

slo=$(cat <<EOF
{"model_uid":"${MODEL}","availability_target":0.99,"ttft_p95_ms":5000.0,"latency_p95_ms":60000.0,"window":"15m","exclude_abort_from_error_budget":true,"exclude_drain_from_error_budget":true,"notes":"phase0 burn","updated_at_ms":${ts}}
EOF
)
"${ETCDCTL_BIN}" put "/slos/${MODEL}" "${slo}" >/dev/null
echo "wrote /slos/${MODEL}"

echo "== generate traffic via Gateway (n=${TRAFFIC_N}) =="
ok=0
for i in $(seq 1 "${TRAFFIC_N}"); do
  code=$(curl -sS -o /tmp/nebula_phase0_slo_body.json -w "%{http_code}" --max-time 90 \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d "{\"model\":\"${MODEL}\",\"stream\":false,\"max_tokens\":16,\"messages\":[{\"role\":\"user\",\"content\":\"phase0 slo ${i}\"}]}" \
    "${GATEWAY_URL}/v1/chat/completions" || echo "000")
  echo "  traffic ${i} -> ${code}"
  if [[ "${code}" == "200" ]]; then
    ok=$((ok + 1))
  fi
done
echo "traffic ok=${ok}/${TRAFFIC_N}"
if [[ "${ok}" -lt 3 ]]; then
  echo "FAIL: need enough successful traffic for SLO sample" >&2
  exit 1
fi

echo "== offline evaluate from Router metrics (abort excluded from 5xx budget) =="
metrics_file="$(mktemp)"
curl -fsS "${ROUTER_URL}/metrics" > "${metrics_file}"
python3 - "$MODEL" "$metrics_file" <<'PY'
import json, sys
model = sys.argv[1]
text = open(sys.argv[2], encoding="utf-8", errors="replace").read()

def metric_sum(name: str) -> float:
    total = 0.0
    for line in text.splitlines():
        if line.startswith("#"):
            continue
        if line.startswith(name + " ") or line.startswith(name + "{"):
            parts = line.rsplit(None, 1)
            if len(parts) == 2:
                try:
                    total += float(parts[1])
                except ValueError:
                    pass
    return total

req = metric_sum("nebula_router_requests_total")
err5 = metric_sum("nebula_router_responses_5xx")
abort = metric_sum("nebula_router_requests_aborted_total")
availability = (1.0 - (err5 / req)) if req > 0 else None
window_secs = 900.0
request_rate = req / window_secs
status = "insufficient_data"
if request_rate >= 0.1 and availability is not None:
    status = "compliant" if availability >= 0.99 else "breaching"

out = {
    "model_uid": model,
    "status": status,
    "requests_total": req,
    "responses_5xx": err5,
    "aborted_total": abort,
    "availability": availability,
    "request_rate_per_15m_window": request_rate,
    "abort_excluded": True,
    "note": "5xx budget uses responses_5xx only; aborted_total is separate",
}
print(json.dumps(out, indent=2))
assert availability is not None and req > 0, "expected traffic on router metrics"
assert err5 <= req, "invalid 5xx vs requests"
print("OK: SLO sample computed; abort counter separate from 5xx budget")
PY
rm -f "${metrics_file}"

echo "== abort contract (test_cancel_sse.sh) =="
# Prefer Router when auth-disabled; Gateway may require token — pass via env.
export ROUTER_URL
export MODEL
export METRICS_URL="${ROUTER_URL}/metrics"
# Auth disabled on router in lab; unset token format for cancel script
unset NEBULA_AUTH_TOKENS || true
bash "${ROOT}/scripts/test_cancel_sse.sh"

echo "OK: phase0 slo burn passed"
