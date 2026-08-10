#!/usr/bin/env bash
# Local Vite dev server + SSH tunnel to remote BFF (e.g. pro6000).
#
# Usage:
#   ./scripts/frontend_dev_remote.sh
#   SSH_HOST=bodesi@39.183.171.3 SSH_PORT=2208 ./scripts/frontend_dev_remote.sh
#
# Open http://127.0.0.1:5173/login  (admin / admin123 on remote BFF)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SSH_HOST="${SSH_HOST:-bodesi@39.183.171.3}"
SSH_PORT="${SSH_PORT:-2208}"
LOCAL_BFF_PORT="${LOCAL_BFF_PORT:-18090}"
REMOTE_BFF_PORT="${REMOTE_BFF_PORT:-18090}"
VITE_PORT="${VITE_PORT:-5173}"

curl_noproxy() {
  curl --noproxy '*' "$@"
}

TUNNEL_PID=""

cleanup() {
  if [ -n "$TUNNEL_PID" ]; then
    kill "$TUNNEL_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

echo "Opening SSH tunnel localhost:${LOCAL_BFF_PORT} -> ${SSH_HOST}:${REMOTE_BFF_PORT}"
ssh -N -o ExitOnForwardFailure=yes \
  -L "${LOCAL_BFF_PORT}:127.0.0.1:${REMOTE_BFF_PORT}" \
  -p "$SSH_PORT" "$SSH_HOST" &
TUNNEL_PID=$!

for _ in $(seq 1 30); do
  if curl_noproxy -sf --max-time 2 "http://127.0.0.1:${LOCAL_BFF_PORT}/api/healthz" >/dev/null 2>&1; then
    echo "BFF reachable via tunnel"
    break
  fi
  sleep 1
done

if ! curl_noproxy -sf --max-time 2 "http://127.0.0.1:${LOCAL_BFF_PORT}/api/healthz" >/dev/null 2>&1; then
  echo "WARN: BFF not responding on :${LOCAL_BFF_PORT}; continuing anyway" >&2
fi

cd "$ROOT/frontend"
if [ ! -d node_modules ]; then
  echo "Installing frontend dependencies..."
  npm install
fi

echo "Starting Vite on http://127.0.0.1:${VITE_PORT} (proxy /api -> :${LOCAL_BFF_PORT})"
export VITE_BFF_PROXY_TARGET="http://127.0.0.1:${LOCAL_BFF_PORT}"
exec npm run dev -- --host 127.0.0.1 --port "$VITE_PORT"
