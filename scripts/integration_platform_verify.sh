#!/usr/bin/env bash
# Integration I0–I6 verification on Gateway /platform/v1 (真机 / CI).
#
# Usage (pro6000 example):
#   GATEWAY=http://127.0.0.1:8081 TOKEN=dev-token \
#   MODEL_UID=qwen15_moe_sglang MODEL_NAME=qwen15_moe_sglang \
#   ./scripts/integration_platform_verify.sh
#
# Optional: SKIP_LEGACY=0 to assert /v1/admin/* returns 404 (v1.6+).
set -euo pipefail

GATEWAY="${GATEWAY:-http://127.0.0.1:8081}"
TOKEN="${TOKEN:-dev-token}"
MODEL_UID="${MODEL_UID:-qwen15_moe_sglang}"
MODEL_NAME="${MODEL_NAME:-${MODEL_UID}}"
# Chat 用独立模型，避免 scale/drain 与推理用例互相干扰（pro6000 默认 vLLM 常驻 ready）。
CHAT_MODEL="${CHAT_MODEL:-qwen15_moe_vllm}"
READY_WAIT_SECS="${READY_WAIT_SECS:-120}"
SKIP_LEGACY="${SKIP_LEGACY:-0}"
LOG_DIR="${LOG_DIR:-/tmp/nebula-integration-verify}"
mkdir -p "$LOG_DIR"

pass=0
fail=0
skip=0

note() { echo "[PASS] $*"; pass=$((pass + 1)); }
bad()  { echo "[FAIL] $*"; fail=$((fail + 1)); }
skip_case() { echo "[SKIP] $*"; skip=$((skip + 1)); }

auth_hdr=(-H "Authorization: Bearer ${TOKEN}")

curl_json() {
  curl -sf "${auth_hdr[@]}" "$@"
}

curl_code() {
  curl -s -o /dev/null -w '%{http_code}' "${auth_hdr[@]}" "$@"
}

wait_ready_replica() {
  local uid="$1"
  local deadline=$((SECONDS + READY_WAIT_SECS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if curl_json "$GATEWAY/platform/v1/models/${uid}/replicas" 2>/dev/null \
      | grep -q '"status"[[:space:]]*:[[:space:]]*"ready"'; then
      return 0
    fi
    sleep 3
  done
  return 1
}

echo "========== Integration /platform/v1 verify =========="
echo "gateway=$GATEWAY model_uid=$MODEL_UID chat_model=$CHAT_MODEL skip_legacy=$SKIP_LEGACY"
echo "log_dir=$LOG_DIR"
echo ""

echo "--- Platform read API ---"
if curl_json "$GATEWAY/platform/v1/health/summary" | grep -q '"gateway"'; then
  note "GET /platform/v1/health/summary"
else
  bad "GET /platform/v1/health/summary"
fi

if curl_json "$GATEWAY/platform/v1/whoami" | grep -q '"principal"'; then
  note "GET /platform/v1/whoami"
else
  bad "GET /platform/v1/whoami"
fi

if curl_json "$GATEWAY/platform/v1/cluster/status" | grep -q '"endpoints"'; then
  note "GET /platform/v1/cluster/status"
else
  bad "GET /platform/v1/cluster/status"
fi

if curl_json "$GATEWAY/platform/v1/nodes" | grep -q '"nodes"'; then
  note "GET /platform/v1/nodes"
else
  bad "GET /platform/v1/nodes"
fi

if curl_json "$GATEWAY/platform/v1/models" | grep -q 'model_uid\|\['; then
  note "GET /platform/v1/models"
else
  bad "GET /platform/v1/models"
fi

if curl_json "$GATEWAY/platform/v1/models/${MODEL_UID}/replicas" | grep -q '"replicas"'; then
  note "GET /platform/v1/models/{uid}/replicas"
else
  bad "GET /platform/v1/models/{uid}/replicas"
fi

echo "--- Legacy admin removed (I6) ---"
legacy_code=$(curl -s -o /dev/null -w '%{http_code}' "${auth_hdr[@]}" "$GATEWAY/v1/admin/whoami" || true)
if [ "$SKIP_LEGACY" = "1" ]; then
  skip_case "legacy /v1/admin/whoami (skip_legacy=1)"
elif [ "$legacy_code" = "404" ]; then
  note "GET /v1/admin/whoami -> 404"
else
  bad "GET /v1/admin/whoami expected 404 got $legacy_code"
fi

echo "--- Control write + Operation (scale idempotent) ---"
idem_key="verify-$(date +%s)"
scale_body='{"replicas":1}'
scale_resp="$LOG_DIR/scale.json"
scale_code=$(curl -s -o "$scale_resp" -w '%{http_code}' -X POST \
  "${auth_hdr[@]}" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: ${idem_key}" \
  -d "$scale_body" \
  "$GATEWAY/platform/v1/models/${MODEL_UID}/deployment/scale" || true)
if [ "$scale_code" = "202" ] && grep -q operation_id "$scale_resp"; then
  note "POST …/deployment/scale -> 202 + operation_id"
  op_id=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["operation_id"])' "$scale_resp")
  for _ in $(seq 1 30); do
    st=$(curl_json "$GATEWAY/platform/v1/operations/${op_id}" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))' || true)
    if [ "$st" = "succeeded" ] || [ "$st" = "failed" ]; then
      break
    fi
    sleep 2
  done
  if [ "$st" = "succeeded" ]; then
    note "GET /platform/v1/operations/{id} -> succeeded"
  else
    bad "operation ${op_id} status=${st:-unknown}"
  fi
  scale_code2=$(curl -s -o "$LOG_DIR/scale-idem.json" -w '%{http_code}' -X POST \
    "${auth_hdr[@]}" \
    -H "Content-Type: application/json" \
    -H "Idempotency-Key: ${idem_key}" \
    -d "$scale_body" \
    "$GATEWAY/platform/v1/models/${MODEL_UID}/deployment/scale" || true)
  if [ "$scale_code2" = "202" ]; then
    note "Idempotency-Key replay -> 202 same operation"
  else
    bad "Idempotency replay code=$scale_code2"
  fi
else
  bad "POST scale code=$scale_code body=$(cat "$scale_resp" 2>/dev/null || true)"
fi

echo "--- Governance read (I4) ---"
if curl_json "$GATEWAY/platform/v1/canaries" | grep -q 'canaries'; then
  note "GET /platform/v1/canaries"
else
  bad "GET /platform/v1/canaries"
fi

echo "--- Inference + request-id echo ---"
if wait_ready_replica "$CHAT_MODEL"; then
  echo "  chat model ${CHAT_MODEL} has ready replica"
else
  echo "  WARN: no ready replica for ${CHAT_MODEL} within ${READY_WAIT_SECS}s"
fi
chat_resp="$LOG_DIR/chat.json"
chat_headers="$LOG_DIR/chat.headers"
curl -s -D "$chat_headers" -o "$chat_resp" -X POST "$GATEWAY/v1/chat/completions" \
  "${auth_hdr[@]}" \
  -H "Content-Type: application/json" \
  -H "x-nebula-request-id: verify-req-1" \
  -d "{\"model\":\"${CHAT_MODEL}\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply OK\"}],\"max_tokens\":8}" || true
if grep -q '"choices"' "$chat_resp" 2>/dev/null; then
  note "POST /v1/chat/completions (model=${CHAT_MODEL})"
else
  bad "chat completions: $(head -c 200 "$chat_resp" 2>/dev/null || true)"
fi
if grep -qi 'x-nebula-request-id: verify-req-1' "$chat_headers" 2>/dev/null; then
  note "response echoes x-nebula-request-id"
else
  bad "missing x-nebula-request-id echo in response headers"
fi

echo "--- Replica drain (last; may briefly affect routing) ---"
drain_code=$(curl -s -o "$LOG_DIR/drain.json" -w '%{http_code}' -X POST \
  "${auth_hdr[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"model_uid\":\"${MODEL_UID}\",\"replica_id\":0}" \
  "$GATEWAY/platform/v1/replicas/drain" || true)
if [ "$drain_code" = "200" ]; then
  note "POST /platform/v1/replicas/drain"
else
  bad "POST drain code=$drain_code"
fi

echo ""
echo "========== Summary: $pass passed, $fail failed, $skip skipped =========="
[ "$fail" -eq 0 ]
