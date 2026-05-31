#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
LOG_DIR="${ROOT_DIR}/logs"
LOG_FILE="${LOG_DIR}/frontend.log"
FRONTEND_DIR="${ROOT_DIR}/frontend"

cd "${FRONTEND_DIR}"

if ! command -v npm >/dev/null 2>&1; then
	if [[ -d /home/ai/local/node-current/bin ]]; then
		export PATH="/home/ai/local/node-current/bin:${PATH}"
	elif [[ -d /home/lipeng/node/bin ]]; then
		export PATH="/home/lipeng/node/bin:${PATH}"
	fi
fi

if [[ ! -f node_modules/vite/dist/node/chunks/dist.js ]]; then
	rm -rf node_modules
	npm ci
fi

VITE_PIDS="$(pgrep -f "vite .*--port 5173" || true)"
if [[ -n "${VITE_PIDS}" ]]; then
	kill ${VITE_PIDS} || true
fi

nohup npm run dev -- --host 0.0.0.0 --port 5173 > "${LOG_FILE}" 2>&1 &
