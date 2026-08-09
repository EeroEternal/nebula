#!/usr/bin/env bash
# Example: deploy DeepSeek-V4-Flash locally via Nebula + vLLM Docker.
set -euo pipefail

NEBULA_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${NEBULA_ENV_FILE:-$NEBULA_ROOT/deploy/nebula.env}"
if [ -f "$ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

MODEL_UID="${MODEL_UID:-deepseek_v4_flash_0731}"
MODEL_NAME="${MODEL_NAME:-deepseek-v4-flash-0731}"
MODEL_PATH="${MODEL_PATH:-/home/bodesi/models/DeepSeek-V4-Flash-0731}"
NODE_ID="${NODE_ID:-$(hostname -s)}"
GPU_INDICES="${GPU_INDICES:-0,1,2,3,4,5,6,7}"
TENSOR_PARALLEL="${TENSOR_PARALLEL:-8}"
VLLM_IMAGE="${VLLM_IMAGE:-vllm/vllm-openai:v0.25.0}"

export MODEL_UID MODEL_NAME MODEL_PATH MODEL_SOURCE=local ENGINE_TYPE=vllm
export DOCKER_IMAGE="$VLLM_IMAGE" NODE_ID GPU_INDICES TENSOR_PARALLEL
export GPU_MEMORY_UTIL="${GPU_MEMORY_UTIL:-0.92}"
export MAX_MODEL_LEN="${MAX_MODEL_LEN:-8192}"
export SERVED_MODEL_NAME="$MODEL_NAME"
export KV_CACHE_DTYPE="${KV_CACHE_DTYPE:-fp8}"
export AUTO_START=1

echo "Building Nebula (release)..."
cargo build --release --manifest-path "$NEBULA_ROOT/Cargo.toml"

echo "Starting Nebula stack..."
NEBULA_ENV_FILE="$ENV_FILE" "$NEBULA_ROOT/bin/nebula-up.sh"

echo "Seeding DeepSeek V4 deployment..."
"$NEBULA_ROOT/scripts/seed_deployment.sh"

echo "Done. Gateway: http://127.0.0.1:${GATEWAY_PORT:-8081}"
echo "Test: curl http://127.0.0.1:${GATEWAY_PORT:-8081}/v1/chat/completions -H 'Content-Type: application/json' -d '{\"model\":\"$MODEL_NAME\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"max_tokens\":32}'"
