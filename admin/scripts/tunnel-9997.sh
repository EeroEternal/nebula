#!/usr/bin/env bash
# 本地 9997 -> 远端 PowerLLM 9997（供 frontend dev proxy 使用）
set -euo pipefail

REMOTE_HOST="${POWERLLM_REMOTE_HOST:-61.163.103.118}"
REMOTE_PORT="${POWERLLM_REMOTE_SSH_PORT:-60001}"
REMOTE_USER="${POWERLLM_REMOTE_USER:-bodesi}"
LOCAL_PORT="${POWERLLM_LOCAL_PORT:-9997}"

if lsof -iTCP:"${LOCAL_PORT}" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "端口 ${LOCAL_PORT} 已有监听，隧道可能已存在。"
  NO_PROXY=127.0.0.1,localhost curl --noproxy '*' -s -o /dev/null -w "health: HTTP %{http_code}\n" \
    --connect-timeout 5 --max-time 10 "http://127.0.0.1:${LOCAL_PORT}/v1/sso/providers" || true
  exit 0
fi

pkill -f "ssh.*${LOCAL_PORT}:127.0.0.1:${LOCAL_PORT}" 2>/dev/null || true

ssh -f -N \
  -o ServerAliveInterval=30 \
  -o ServerAliveCountMax=3 \
  -L "${LOCAL_PORT}:127.0.0.1:${LOCAL_PORT}" \
  -p "${REMOTE_PORT}" \
  "${REMOTE_USER}@${REMOTE_HOST}"

sleep 1
NO_PROXY=127.0.0.1,localhost curl --noproxy '*' -s -o /dev/null -w "health: HTTP %{http_code}\n" \
  --connect-timeout 5 --max-time 10 "http://127.0.0.1:${LOCAL_PORT}/v1/sso/providers"

echo "SSH 隧道已建立: 127.0.0.1:${LOCAL_PORT} -> ${REMOTE_HOST}:${LOCAL_PORT}"
