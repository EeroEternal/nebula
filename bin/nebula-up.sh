#!/bin/bash
set -e

# Paths (adjust these if running on a different machine)
NEBULA_ROOT=$(cd "$(dirname "$0")/.." && pwd)
LOG_DIR="$NEBULA_ROOT/logs"
ENV_FILE="${NEBULA_ENV_FILE:-$NEBULA_ROOT/deploy/nebula.env}"

# Load centralized environment config when present.
if [ -f "$ENV_FILE" ]; then
    set -a
    . "$ENV_FILE"
    set +a
fi

# Configuration
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
GATEWAY_PORT="${GATEWAY_PORT:-8081}"
BFF_PORT="${BFF_PORT:-18090}"
ROUTER_PORT="${ROUTER_PORT:-18081}"
NODE_PORT="${NODE_PORT:-10824}"
MODEL_UID="${MODEL_UID:-qwen2_5_0_5b}"
MODEL_NAME="${MODEL_NAME:-Qwen/Qwen2.5-0.5B-Instruct}"
NODE_ID="${NODE_ID:-node_gpu0}"
VLLM_IMAGE="${VLLM_IMAGE:-vllm/vllm-openai:v0.11.0}"
VLLM_MODEL_DIR="${VLLM_MODEL_DIR:-/DATA/Model}"
SEED_DEPLOYMENT="${SEED_DEPLOYMENT:-0}"
NEBULA_AUTH_DISABLED="${NEBULA_AUTH_DISABLED:-0}"

# xtrace config for observability/audit APIs.
# OBSERVE_TOKEN is the bearer token used when Nebula calls xtrace.
# Example: export OBSERVE_TOKEN="nebula-xtrace-token-2026"
OBSERVE_URL="${OBSERVE_URL:-http://127.0.0.1:8742}"
OBSERVE_TOKEN="${OBSERVE_TOKEN:-}"
OBSERVE_AUTH_MODE="${OBSERVE_AUTH_MODE:-internal}"
BFF_DATABASE_URL="${BFF_DATABASE_URL:-postgresql://postgres:postgres@127.0.0.1:5432/nebula}"

# Set START_BFF=1 to run nebula-bff locally (recommended for frontend /api + /api/v2).
# Example: START_BFF=1 OBSERVE_TOKEN=nebula-xtrace-token-2026 ./bin/nebula-up.sh
START_BFF="${START_BFF:-0}"

# Preflight checks to avoid frequent xtrace auth misconfiguration.
if [ "$START_BFF" = "1" ] && [ "$OBSERVE_AUTH_MODE" = "service" ] && [ -z "$OBSERVE_TOKEN" ]; then
    echo "ERROR: OBSERVE_AUTH_MODE=service but OBSERVE_TOKEN is empty."
    echo "Fix: set OBSERVE_TOKEN in $ENV_FILE (or export it) before starting."
    echo "Tip: OBSERVE_TOKEN=\$(grep -E '^API_BEARER_TOKEN=' ~/github/xtrace/.env | head -n1 | cut -d= -f2-)"
    exit 1
fi

# BFF auth/session storage must use Nebula's own database, not observe (xtrace) database.
if [ "$START_BFF" = "1" ] && echo "$BFF_DATABASE_URL" | grep -Eiq '/(observe|xtrace)([/?].*)?$'; then
    echo "ERROR: BFF_DATABASE_URL points to observe/xtrace database."
    echo "Fix: use a dedicated Nebula DB, e.g. postgresql://<user>:<pass>@127.0.0.1:5432/nebula"
    exit 1
fi

if [ "$START_BFF" = "1" ] && [ ! -f "$ENV_FILE" ]; then
    echo "WARN: $ENV_FILE not found; using process env only."
    echo "      Missing OBSERVE_TOKEN often causes /api/audit-logs Unauthorized."
fi

mkdir -p "$LOG_DIR"

echo "Starting Nebula Service Stack..."
echo "Logs will be written to $LOG_DIR"
echo "env file: $ENV_FILE"
echo "xtrace url: $OBSERVE_URL"
echo "xtrace auth mode: $OBSERVE_AUTH_MODE"
echo "bff database: $BFF_DATABASE_URL"
if [ -z "$OBSERVE_TOKEN" ]; then
    echo "xtrace token: (empty)"
else
    echo "xtrace token: (configured)"
fi

# 1. Start Etcd
if pgrep -x "etcd" > /dev/null; then
    echo "Etcd is already running."
else
    echo "Starting Etcd..."
    ETCD_DATA_DIR="/tmp/nebula-etcd"
    mkdir -p "$ETCD_DATA_DIR"
    nohup ~/bin/etcd --data-dir "$ETCD_DATA_DIR" --advertise-client-urls http://0.0.0.0:2379 --listen-client-urls http://0.0.0.0:2379 > "$LOG_DIR/etcd.log" 2>&1 &
    sleep 2
fi

# 2. Start Router
echo "Starting Nebula Router..."
if [ "$NEBULA_AUTH_DISABLED" = "1" ]; then
	nohup env NEBULA_AUTH_DISABLED=1 "$NEBULA_ROOT/target/release/nebula-router" \
		--listen-addr "0.0.0.0:$ROUTER_PORT" \
		--etcd-endpoint "$ETCD_ENDPOINT" > "$LOG_DIR/router.log" 2>&1 &
else
	nohup "$NEBULA_ROOT/target/release/nebula-router" \
		--listen-addr "0.0.0.0:$ROUTER_PORT" \
		--etcd-endpoint "$ETCD_ENDPOINT" > "$LOG_DIR/router.log" 2>&1 &
fi

# 2.5 Start Scheduler
echo "Starting Nebula Scheduler..."
nohup "$NEBULA_ROOT/target/release/nebula-scheduler" \
    --etcd-endpoint "$ETCD_ENDPOINT" \
    --default-node-id "$NODE_ID" \
    --default-port "$NODE_PORT" > "$LOG_DIR/scheduler.log" 2>&1 &

# 2.6 Start BFF (optional)
if [ "$START_BFF" = "1" ]; then
    echo "Starting Nebula BFF..."
    nohup "$NEBULA_ROOT/target/release/nebula-bff" \
        --listen-addr "0.0.0.0:$BFF_PORT" \
        --etcd-endpoint "$ETCD_ENDPOINT" \
        --router-url "http://127.0.0.1:$ROUTER_PORT" \
        --database-url "$BFF_DATABASE_URL" \
        --xtrace-url "$OBSERVE_URL" \
        --xtrace-token "$OBSERVE_TOKEN" \
        --xtrace-auth-mode "$OBSERVE_AUTH_MODE" > "$LOG_DIR/bff.log" 2>&1 &
fi

# 3. Start Gateway
if [ "$NEBULA_AUTH_DISABLED" = "1" ]; then
	nohup env NEBULA_AUTH_DISABLED=1 "$NEBULA_ROOT/target/release/nebula-gateway" \
		--listen-addr "0.0.0.0:$GATEWAY_PORT" \
		--router-url "http://127.0.0.1:$ROUTER_PORT" \
		--bff-url "http://127.0.0.1:$BFF_PORT" \
		--xtrace-url "$OBSERVE_URL" \
		--xtrace-token "$OBSERVE_TOKEN" > "$LOG_DIR/gateway.log" 2>&1 &
else
	nohup "$NEBULA_ROOT/target/release/nebula-gateway" \
		--listen-addr "0.0.0.0:$GATEWAY_PORT" \
		--router-url "http://127.0.0.1:$ROUTER_PORT" \
		--bff-url "http://127.0.0.1:$BFF_PORT" \
		--xtrace-url "$OBSERVE_URL" \
		--xtrace-token "$OBSERVE_TOKEN" > "$LOG_DIR/gateway.log" 2>&1 &
fi

# 4. Start Node (docker mode — all vLLM runs inside containers)
echo "Starting Nebula Node ($NODE_ID) [docker: $VLLM_IMAGE]..."
nohup "$NEBULA_ROOT/target/release/nebula-node" \
    --node-id "$NODE_ID" \
    --etcd-endpoint "$ETCD_ENDPOINT" \
    --vllm-docker-image "$VLLM_IMAGE" \
    --vllm-model-dir "$VLLM_MODEL_DIR" \
    --vllm-port "$NODE_PORT" \
    --vllm-use-modelscope \
    --ready-timeout-secs 3600 > "$LOG_DIR/node_$NODE_ID.log" 2>&1 &

if [ "$SEED_DEPLOYMENT" = "1" ]; then
	echo "Seeding model spec + deployment for $MODEL_UID..."
	MODEL_UID="$MODEL_UID" MODEL_NAME="$MODEL_NAME" \
		"$NEBULA_ROOT/scripts/seed_deployment.sh" || \
		echo "WARN: seed_deployment.sh failed (is etcdctl installed?)"
fi

echo "All services started!"
echo "Gateway: http://127.0.0.1:$GATEWAY_PORT"
if [ "$START_BFF" = "1" ]; then
    echo "BFF:     http://127.0.0.1:$BFF_PORT"
fi
echo "Router:  http://127.0.0.1:$ROUTER_PORT"
echo "Node:    $NODE_ID (Model: $MODEL_NAME)"
