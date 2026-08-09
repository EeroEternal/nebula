#!/usr/bin/env bash
# Seed /models/{uid}/spec and /deployments/{uid} into etcd for declarative scheduling.
# Requires etcdctl on PATH (or ~/bin/etcdctl) and a running etcd.
set -euo pipefail

ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
ETCDCTL="${ETCDCTL:-etcdctl}"
if ! command -v "$ETCDCTL" >/dev/null 2>&1 && [ -x "$HOME/bin/etcdctl" ]; then
  ETCDCTL="$HOME/bin/etcdctl"
fi

MODEL_UID="${MODEL_UID:-}"
MODEL_NAME="${MODEL_NAME:-}"
MODEL_SOURCE="${MODEL_SOURCE:-huggingface}"
MODEL_PATH="${MODEL_PATH:-}"
ENGINE_TYPE="${ENGINE_TYPE:-vllm}"
DOCKER_IMAGE="${DOCKER_IMAGE:-}"
REPLICAS="${REPLICAS:-1}"
NODE_ID="${NODE_ID:-}"
GPU_INDICES="${GPU_INDICES:-}"
TENSOR_PARALLEL="${TENSOR_PARALLEL:-}"
GPU_MEMORY_UTIL="${GPU_MEMORY_UTIL:-}"
MAX_MODEL_LEN="${MAX_MODEL_LEN:-}"
SERVED_MODEL_NAME="${SERVED_MODEL_NAME:-}"
KV_CACHE_DTYPE="${KV_CACHE_DTYPE:-}"
AUTO_START="${AUTO_START:-1}"

if [ -z "$MODEL_UID" ] || [ -z "$MODEL_NAME" ]; then
  echo "Usage: MODEL_UID=... MODEL_NAME=... [MODEL_PATH=...] [GPU_INDICES=0,1,2,3] $0" >&2
  exit 1
fi

NOW_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

config_fields=()
[ -n "$TENSOR_PARALLEL" ] && config_fields+=("\"tensor_parallel_size\": $TENSOR_PARALLEL")
[ -n "$GPU_MEMORY_UTIL" ] && config_fields+=("\"gpu_memory_utilization\": $GPU_MEMORY_UTIL")
[ -n "$MAX_MODEL_LEN" ] && config_fields+=("\"max_model_len\": $MAX_MODEL_LEN")
[ -n "$SERVED_MODEL_NAME" ] && config_fields+=("\"served_model_name\": \"$SERVED_MODEL_NAME\"")
[ -n "$KV_CACHE_DTYPE" ] && config_fields+=("\"kv_cache_dtype\": \"$KV_CACHE_DTYPE\"")
if [ "$KV_CACHE_DTYPE" = "fp8" ] || [[ "${MODEL_NAME,,}" == *deepseek* ]]; then
  config_fields+=("\"trust_remote_code\": true")
  config_fields+=("\"enable_expert_parallel\": true")
  config_fields+=("\"block_size\": 256")
  config_fields+=("\"tokenizer_mode\": \"deepseek_v4\"")
fi

config_json="null"
if [ "${#config_fields[@]}" -gt 0 ]; then
  config_json="{ $(IFS=,; echo "${config_fields[*]}") }"
fi

model_path_json="null"
if [ -n "$MODEL_PATH" ]; then
  model_path_json="\"$MODEL_PATH\""
fi

docker_image_json="null"
if [ -n "$DOCKER_IMAGE" ]; then
  docker_image_json="\"$DOCKER_IMAGE\""
fi

SPEC=$(cat <<EOF
{
  "model_uid": "$MODEL_UID",
  "model_name": "$MODEL_NAME",
  "model_source": "$MODEL_SOURCE",
  "model_path": $model_path_json,
  "engine_type": "$ENGINE_TYPE",
  "docker_image": $docker_image_json,
  "config": $config_json,
  "labels": {},
  "created_at_ms": $NOW_MS,
  "updated_at_ms": $NOW_MS,
  "created_by": "seed_deployment.sh"
}
EOF
)

"$ETCDCTL" --endpoints="$ETCD_ENDPOINT" put "/models/$MODEL_UID/spec" "$SPEC"
echo "seeded /models/$MODEL_UID/spec"

if [ "$AUTO_START" != "1" ]; then
  exit 0
fi

node_json="null"
if [ -n "$NODE_ID" ]; then
  node_json="\"$NODE_ID\""
fi

gpu_json="null"
if [ -n "$GPU_INDICES" ]; then
  gpu_json="[$(echo "$GPU_INDICES" | sed 's/,/, /g')]"
fi

DEPLOY=$(cat <<EOF
{
  "model_uid": "$MODEL_UID",
  "desired_state": "running",
  "replicas": $REPLICAS,
  "node_affinity": $node_json,
  "gpu_affinity": $gpu_json,
  "config_overrides": $config_json,
  "compat_rule_ids": [],
  "version": 1,
  "updated_at_ms": $NOW_MS
}
EOF
)

"$ETCDCTL" --endpoints="$ETCD_ENDPOINT" put "/deployments/$MODEL_UID" "$DEPLOY"
echo "seeded /deployments/$MODEL_UID (desired_state=running)"
