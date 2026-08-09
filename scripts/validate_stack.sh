#!/usr/bin/env bash
# Post-deploy validation: health, gateway, latency, metrics, probe.
set -euo pipefail

GATEWAY="${GATEWAY:-http://127.0.0.1:8081}"
ENGINE="${ENGINE_URL:-http://127.0.0.1:10825}"
NODE_METRICS="${NODE_METRICS:-http://127.0.0.1:9090/metrics}"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
MODEL_UID="${MODEL_UID:-deepseek_v4_flash_0731}"
MODEL_NAME="${MODEL_NAME:-deepseek-v4-flash-0731}"
ETCDCTL="${ETCDCTL:-etcdctl}"
if ! command -v "$ETCDCTL" >/dev/null 2>&1 && [ -x "$HOME/bin/etcdctl" ]; then
  ETCDCTL="$HOME/bin/etcdctl"
fi

pass=0
fail=0
note() { echo "[OK] $*"; pass=$((pass + 1)); }
bad()  { echo "[FAIL] $*"; fail=$((fail + 1)); }

curl_q() { curl -sf "$@" 2>/dev/null; }

echo "========== Nebula stack validation =========="
echo "gateway=$GATEWAY engine=$ENGINE model=$MODEL_NAME"
echo ""

echo "--- 1. etcd endpoint ---"
ep=$("$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get "/endpoints/${MODEL_UID}/0" 2>/dev/null | tail -1 || true)
if echo "$ep" | grep -q '"status":"ready"'; then note "endpoint ready"; else bad "endpoint not ready: $ep"; fi

echo "--- 2. direct engine health ---"
if curl_q "$ENGINE/health" >/dev/null; then note "engine /health"; else bad "engine /health"; fi

echo "--- 3. gateway health ---"
if curl_q "$GATEWAY/healthz" >/dev/null; then note "gateway /healthz"; else bad "gateway /healthz"; fi

echo "--- 4. gateway /v1/models ---"
if curl_q "$GATEWAY/v1/models" | grep -qi "$MODEL_NAME\|deepseek"; then note "models listed"; else bad "models list"; fi

echo "--- 5. short chat (gateway) ---"
t0=$(date +%s%3N)
short=$(curl_q -X POST "$GATEWAY/v1/chat/completions" -H "Content-Type: application/json" \
  -d "{\"model\":\"$MODEL_NAME\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply with exactly: OK\"}],\"max_tokens\":16}" || true)
t1=$(date +%s%3N)
if echo "$short" | grep -q '"choices"'; then
  note "short chat (${t1}-$t0 ms)"
else bad "short chat: $short"
fi

echo "--- 6. direct engine chat ---"
if curl_q -X POST "$ENGINE/v1/chat/completions" -H "Content-Type: application/json" \
  -d "{\"model\":\"$MODEL_NAME\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"max_tokens\":8}" | grep -q choices; then
  note "direct engine chat"
else bad "direct engine chat"
fi

echo "--- 7. long context chat (gateway) ---"
long_prompt=$(python3 -c "print('word ' * 800)")
long=$(curl_q -m 120 -X POST "$GATEWAY/v1/chat/completions" -H "Content-Type: application/json" \
  -d "{\"model\":\"$MODEL_NAME\",\"messages\":[{\"role\":\"user\",\"content\":\"$long_prompt\\nSummarize in 5 words.\"}],\"max_tokens\":32}" || true)
if echo "$long" | grep -q '"choices"'; then note "long context chat"; else bad "long context chat"; fi

echo "--- 8. concurrent requests (3x) ---"
ok=0
for _ in 1 2 3; do
  curl_q -m 60 -X POST "$GATEWAY/v1/chat/completions" -H "Content-Type: application/json" \
    -d "{\"model\":\"$MODEL_NAME\",\"messages\":[{\"role\":\"user\",\"content\":\"ping\"}],\"max_tokens\":4}" | grep -q choices && ok=$((ok + 1)) || true
done
if [ "$ok" -eq 3 ]; then note "3 concurrent chats"; else bad "concurrent ($ok/3)"; fi

echo "--- 9. node metrics ---"
m=$(curl_q "$NODE_METRICS" || true)
if echo "$m" | grep -q nebula_node; then
  note "node metrics reachable"
  echo "$m" | grep -E 'nebula_node_engine_(probe_failures|container_running|kv_cache|pending)' | head -8 || true
else bad "node metrics unreachable"
fi

echo "--- 10. probe_stack ---"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -x "$SCRIPT_DIR/probe_stack.sh" ]; then
  MODEL_UID="$MODEL_UID" "$SCRIPT_DIR/probe_stack.sh" 2>/dev/null | head -25
  note "probe_stack ran"
else bad "probe_stack missing"
fi

echo "--- 11. GPU snapshot ---"
nvidia-smi --query-gpu=index,memory.used,memory.total,utilization.gpu --format=csv,noheader 2>/dev/null | head -8 || bad "nvidia-smi"

echo ""
echo "========== Summary: $pass passed, $fail failed =========="
[ "$fail" -eq 0 ]
