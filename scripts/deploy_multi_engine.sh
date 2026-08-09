#!/usr/bin/env bash
# Deploy vLLM + SGLang models on the same node with non-overlapping GPU indices.
# Example (pro6000): DeepSeek V4 TP=4 on GPU 0-3 + Qwen MoE SGLang TP=2 on GPU 6-7.
# DeepSeek V4 has 64 attention heads — TP must divide 64 (1/2/4/8 only).
# DeepSeek V4 Flash (~156GB) typically needs TP=8 on 8×98GB; TP=4 often OOM.
# For multi-engine demo on one node, use a smaller vLLM model (see qwen_vllm_demo below)
# or run SGLang on a disjoint GPU set while vLLM holds the rest.
set -euo pipefail

NEBULA_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${NEBULA_ENV_FILE:-$NEBULA_ROOT/deploy/nebula.env}"
if [ -f "$ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
NODE_ID="${NODE_ID:-$(hostname -s)}"
VLLM_IMAGE="${VLLM_IMAGE:-vllm/vllm-openai:v0.25.0}"
SGLANG_IMAGE="${SGLANG_IMAGE:-lmsysorg/sglang:latest}"
MODEL_DIR="${MODEL_DIR:-/home/bodesi/models}"

# vLLM model (small — e.g. Qwen1.5-MoE ~27GB)
VLLM_UID="${VLLM_UID:-qwen15_moe_vllm}"
VLLM_NAME="${VLLM_NAME:-qwen15-moe-vllm}"
VLLM_PATH="${VLLM_PATH:-$MODEL_DIR/Qwen1.5-MoE-A2.7B-Chat}"
VLLM_GPUS="${VLLM_GPUS:-0,1}"
VLLM_TP="${VLLM_TP:-2}"
VLLM_GPU_UTIL="${VLLM_GPU_UTIL:-0.85}"
VLLM_MAX_LEN="${VLLM_MAX_LEN:-8192}"
VLLM_SERVED_NAME="${VLLM_SERVED_NAME:-qwen15-moe-vllm}"
SEED_VLLM="${SEED_VLLM:-1}"

# SGLang model (same or different small model; disjoint GPUs)
SGLANG_UID="${SGLANG_UID:-qwen15_moe_sglang}"
SGLANG_NAME="${SGLANG_NAME:-qwen15-moe-sglang}"
SGLANG_PATH="${SGLANG_PATH:-$MODEL_DIR/Qwen1.5-MoE-A2.7B-Chat}"
SGLANG_GPUS="${SGLANG_GPUS:-6,7}"
SGLANG_TP="${SGLANG_TP:-2}"
SGLANG_MEM_FRACTION="${SGLANG_MEM_FRACTION:-0.85}"
SGLANG_MAX_LEN="${SGLANG_MAX_LEN:-8192}"
SGLANG_SERVED_NAME="${SGLANG_SERVED_NAME:-qwen15-moe-sglang}"
SEED_SGLANG="${SEED_SGLANG:-1}"

# Optional: remove legacy DeepSeek deployment keys before seeding
CLEANUP_UIDS="${CLEANUP_UIDS:-deepseek_v4_flash_0731}"

ETCDCTL="${ETCDCTL:-etcdctl}"
if ! command -v "$ETCDCTL" >/dev/null 2>&1 && [ -x "$HOME/bin/etcdctl" ]; then
  ETCDCTL="$HOME/bin/etcdctl"
fi

echo "=== Multi-engine deploy on node $NODE_ID ==="
echo "vLLM:   $VLLM_UID GPUs=$VLLM_GPUS TP=$VLLM_TP"
echo "SGLang: $SGLANG_UID GPUs=$SGLANG_GPUS TP=$SGLANG_TP"
echo ""
echo "Node must be started with BOTH engine images, e.g.:"
echo "  nebula-node --node-id $NODE_ID \\"
echo "    --vllm-docker-image $VLLM_IMAGE --vllm-model-dir $MODEL_DIR \\"
echo "    --sglang-docker-image $SGLANG_IMAGE --sglang-model-dir $MODEL_DIR \\"
echo "    --etcd-endpoint $ETCD_ENDPOINT"
echo ""

if [ -n "$CLEANUP_UIDS" ]; then
  echo "--- Cleaning up old deployments ---"
  for uid in $(echo "$CLEANUP_UIDS" | tr ',' ' '); do
    "$ETCDCTL" --endpoints="$ETCD_ENDPOINT" del "/deployments/$uid" 2>/dev/null || true
    "$ETCDCTL" --endpoints="$ETCD_ENDPOINT" del --prefix "/placements/$uid/" 2>/dev/null || true
    "$ETCDCTL" --endpoints="$ETCD_ENDPOINT" del --prefix "/endpoints/$uid/" 2>/dev/null || true
    "$ETCDCTL" --endpoints="$ETCD_ENDPOINT" del "/models/$uid/spec" 2>/dev/null || true
    docker rm -f "nebula-${uid}-0" 2>/dev/null || true
    echo "removed $uid"
  done
fi

if [ "$SEED_VLLM" = "1" ]; then
  echo "--- Seeding vLLM model ---"
  MODEL_UID="$VLLM_UID" MODEL_NAME="$VLLM_NAME" MODEL_SOURCE=local MODEL_PATH="$VLLM_PATH" \
    ENGINE_TYPE=vllm DOCKER_IMAGE="$VLLM_IMAGE" NODE_ID="$NODE_ID" \
    GPU_INDICES="$VLLM_GPUS" TENSOR_PARALLEL="$VLLM_TP" \
    GPU_MEMORY_UTIL="$VLLM_GPU_UTIL" MAX_MODEL_LEN="$VLLM_MAX_LEN" \
    SERVED_MODEL_NAME="$VLLM_SERVED_NAME" AUTO_START=1 \
    ETCD_ENDPOINT="$ETCD_ENDPOINT" \
    "$NEBULA_ROOT/scripts/seed_deployment.sh"
fi

if [ "$SEED_SGLANG" = "1" ]; then
  echo "--- Seeding SGLang model ---"
  MODEL_UID="$SGLANG_UID" MODEL_NAME="$SGLANG_NAME" MODEL_SOURCE=local MODEL_PATH="$SGLANG_PATH" \
    ENGINE_TYPE=sglang DOCKER_IMAGE="$SGLANG_IMAGE" NODE_ID="$NODE_ID" \
    GPU_INDICES="$SGLANG_GPUS" TENSOR_PARALLEL="$SGLANG_TP" \
    GPU_MEMORY_UTIL="$SGLANG_MEM_FRACTION" MAX_MODEL_LEN="$SGLANG_MAX_LEN" \
    SERVED_MODEL_NAME="$SGLANG_SERVED_NAME" AUTO_START=1 \
    ETCD_ENDPOINT="$ETCD_ENDPOINT" \
    "$NEBULA_ROOT/scripts/seed_deployment.sh"
fi

echo ""
echo "Done. Scheduler will reconcile placements; engines may take several minutes to load."
echo "Run: ./scripts/test_multi_engine.sh"
