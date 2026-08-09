#!/usr/bin/env bash
# Wait for model endpoint ready, then run validate_stack.sh
set -euo pipefail

WAIT_SECS="${WAIT_SECS:-3600}"
POLL_SECS="${POLL_SECS:-20}"
MODEL_UID="${MODEL_UID:-deepseek_v4_flash_0731}"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
ETCDCTL="${ETCDCTL:-etcdctl}"
if ! command -v "$ETCDCTL" >/dev/null 2>&1 && [ -x "$HOME/bin/etcdctl" ]; then
  ETCDCTL="$HOME/bin/etcdctl"
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
deadline=$((SECONDS + WAIT_SECS))

echo "Waiting up to ${WAIT_SECS}s for /endpoints/${MODEL_UID}/0 ready..."
while [ "$SECONDS" -lt "$deadline" ]; do
  raw=$("$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get "/endpoints/${MODEL_UID}/0" 2>/dev/null | tail -1 || true)
  if echo "$raw" | grep -q '"status":"ready"'; then
    echo "Endpoint ready."
    # derive engine URL from endpoint json if possible
    base=$(echo "$raw" | python3 -c "import sys,json; print(json.loads(sys.stdin.read()).get('base_url',''))" 2>/dev/null || true)
    if [ -n "$base" ]; then export ENGINE_URL="$base"; fi
    exec "$SCRIPT_DIR/validate_stack.sh"
  fi
  # also accept replica 1
  raw=$("$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get "/endpoints/${MODEL_UID}/1" 2>/dev/null | tail -1 || true)
  if echo "$raw" | grep -q '"status":"ready"'; then
    echo "Endpoint ready (replica 1)."
    base=$(echo "$raw" | python3 -c "import sys,json; print(json.loads(sys.stdin.read()).get('base_url',''))" 2>/dev/null || true)
    if [ -n "$base" ]; then export ENGINE_URL="$base"; fi
    exec "$SCRIPT_DIR/validate_stack.sh"
  fi
  status=$(echo "$raw" | grep -o '"status":"[^"]*"' || echo "missing")
  echo "$(date +%H:%M:%S) $status"
  sleep "$POLL_SECS"
done
echo "Timeout waiting for endpoint" >&2
exit 1
