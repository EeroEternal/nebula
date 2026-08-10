#!/usr/bin/env bash
# Smoke-test multi-engine serving via Gateway (vLLM + SGLang endpoints).
set -euo pipefail

GATEWAY="${GATEWAY:-http://127.0.0.1:8081}"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
ETCDCTL="${ETCDCTL:-etcdctl}"
if ! command -v "$ETCDCTL" >/dev/null 2>&1 && [ -x "$HOME/bin/etcdctl" ]; then
  ETCDCTL="$HOME/bin/etcdctl"
fi
AUTH_HEADER="${AUTH_HEADER:-}"
if [ -z "$AUTH_HEADER" ] && [ -n "${NEBULA_AUTH_TOKEN:-}" ]; then
  AUTH_HEADER="Authorization: Bearer $NEBULA_AUTH_TOKEN"
fi
if [ -z "$AUTH_HEADER" ] && [ "${NEBULA_AUTH_DISABLED:-}" != "1" ]; then
  AUTH_HEADER="Authorization: Bearer dev-token"
fi

VLLM_UID="${VLLM_UID:-qwen15_moe_vllm}"
VLLM_MODEL="${VLLM_MODEL:-qwen15_moe_vllm}"
VLLM_SERVED="${VLLM_SERVED:-qwen15-moe-vllm}"
SGLANG_UID="${SGLANG_UID:-qwen15_moe_sglang}"
SGLANG_MODEL="${SGLANG_MODEL:-qwen15_moe_sglang}"
SGLANG_SERVED="${SGLANG_SERVED:-qwen15-moe-sglang}"
WAIT_SECS="${WAIT_SECS:-900}"
POLL_SECS="${POLL_SECS:-15}"

curl_auth() {
  if [ -n "$AUTH_HEADER" ]; then
    curl -sf -H "$AUTH_HEADER" "$@"
  else
    curl -sf "$@"
  fi
}

wait_endpoint() {
  local uid="$1" rid="${2:-0}"
  local key="/endpoints/${uid}/${rid}"
  local deadline=$((SECONDS + WAIT_SECS))
  while [ "$SECONDS" -lt "$deadline" ]; do
    local raw
    raw=$("$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get "$key" 2>/dev/null | tail -1 || true)
    if [ -n "$raw" ] && echo "$raw" | grep -q '"status":"ready"'; then
      echo "ready: $key"
      return 0
    fi
    echo "waiting: $key ($(echo "$raw" | grep -o '"status":"[^"]*"' || echo 'missing'))"
    sleep "$POLL_SECS"
  done
  echo "timeout waiting for $key" >&2
  return 1
}

chat_once() {
  local model="$1" label="$2"
  echo ""
  echo "=== $label (model=$model) ==="
  local resp
  resp=$(curl_auth -X POST "$GATEWAY/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d "{\"model\":\"$model\",\"messages\":[{\"role\":\"user\",\"content\":\"Say hi in one short sentence.\"}],\"max_tokens\":32}" \
    2>&1) || { echo "FAIL: $resp" >&2; return 1; }
  echo "$resp" | head -c 500
  echo ""
}

completion_once() {
  local model="$1" label="$2"
  echo ""
  echo "=== $label (model=$model) ==="
  local resp
  resp=$(curl_auth -X POST "$GATEWAY/v1/completions" \
    -H "Content-Type: application/json" \
    -d "{\"model\":\"$model\",\"prompt\":\"Say hi in one short sentence.\",\"max_tokens\":32}" \
    2>&1) || { echo "FAIL: $resp" >&2; return 1; }
  echo "$resp" | head -c 500
  echo ""
}

echo "=== etcd endpoints ==="
"$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get --prefix /endpoints/ 2>/dev/null || true

echo ""
echo "=== Docker engine containers ==="
docker ps --filter 'name=nebula-' --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}' 2>/dev/null || true

echo ""
echo "=== Waiting for endpoints ==="
wait_endpoint "$VLLM_UID" 0 || true
wait_endpoint "$SGLANG_UID" 0 || true

echo ""
echo "=== Gateway inference ==="
chat_once "$VLLM_MODEL" "Qwen vLLM (model_uid)"
chat_once "$SGLANG_MODEL" "Qwen SGLang (model_uid)"
chat_once "$VLLM_SERVED" "Qwen vLLM (served_model_name)"
chat_once "$SGLANG_SERVED" "Qwen SGLang (served_model_name)"
completion_once "$VLLM_SERVED" "Qwen vLLM /v1/completions"

echo ""
echo "=== Probe stack ==="
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -x "$SCRIPT_DIR/probe_stack.sh" ]; then
  "$SCRIPT_DIR/probe_stack.sh"
fi

echo ""
echo "=== Done ==="
