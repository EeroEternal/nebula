#!/usr/bin/env bash
# verify-embedded-router.sh — A/B/C benchmark that confirms the gateway→router
# hop is removed by NEBULA_ROUTER_MODE=embedded.
#
#   A) direct    client -> engine :ENGINE_PORT
#   B) remote    client -> gateway(:REMOTE_PORT, NEBULA_ROUTER_MODE=remote) -> router -> engine
#   C) embedded  client -> gateway(:EMBED_PORT, NEBULA_ROUTER_MODE=embedded) -> engine
#
# The client is the official `vllm bench serve` run *inside the engine Pod* via
# kubectl exec (same method as docs/dev/hw/vllm-serving-perf.md). Runs must be
# interleaved to cancel warm-up/ordering drift.
#
# Run on the control node (has kubectl + etcd + nebula binaries). Assumes the
# etcd container + router + k8s-controller + engine Pod are already up; this
# script starts only the two gateway instances it needs.
#
# Env overrides (defaults match the xinference control node):
#   BIN_DIR=~/nebula-src/target/release  LOG_DIR=~/nebula-stack-logs
#   ETCD_CONTAINER=nebula-etcd  ETCD_ENDPOINT=http://127.0.0.1:12379
#   NAMESPACE=powerllm  MODEL_UID=qwen2.5-7b-nebula  ENGINE_POD=nebula-<MODEL_UID>
#   ENGINE_PORT=43537   ROUTER_PORT=18081
#   REMOTE_PORT=8081    EMBED_PORT=8082
#   CONTROL_HOST=<control-node IP reachable from the Pod>  (required)
#   TOKENIZER=/models/weights  CONCURRENCY=32  NUM_PROMPTS=256
#   INPUT_LEN=128  OUTPUT_LEN=32  ROUNDS=3  WARMUPS=4
#
# Usage:  CONTROL_HOST=10.x.x.x scripts/verify-embedded-router.sh
set -euo pipefail

BIN_DIR="${BIN_DIR:-$HOME/nebula-src/target/release}"
LOG_DIR="${LOG_DIR:-$HOME/nebula-stack-logs}"
ETCD_CONTAINER="${ETCD_CONTAINER:-nebula-etcd}"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:12379}"
NAMESPACE="${NAMESPACE:-powerllm}"
MODEL_UID="${MODEL_UID:-qwen2.5-7b-nebula}"
ENGINE_POD="${ENGINE_POD:-nebula-${MODEL_UID}}"
ENGINE_PORT="${ENGINE_PORT:-43537}"
ROUTER_PORT="${ROUTER_PORT:-18081}"
REMOTE_PORT="${REMOTE_PORT:-8081}"
EMBED_PORT="${EMBED_PORT:-8082}"
CONTROL_HOST="${CONTROL_HOST:-}"
TOKENIZER="${TOKENIZER:-/models/weights}"
CONCURRENCY="${CONCURRENCY:-32}"
NUM_PROMPTS="${NUM_PROMPTS:-256}"
INPUT_LEN="${INPUT_LEN:-128}"
OUTPUT_LEN="${OUTPUT_LEN:-32}"
ROUNDS="${ROUNDS:-3}"
WARMUPS="${WARMUPS:-4}"

log() { printf '[verify-embedded] %s\n' "$*"; }
die() { printf '[verify-embedded] ERROR: %s\n' "$*" >&2; exit 1; }

require() { command -v "$1" >/dev/null 2>&1 || die "missing command: $1"; }
require kubectl
require curl

[ -n "$CONTROL_HOST" ] || die "set CONTROL_HOST to the control-node IP reachable from the Pod"
[ -x "$BIN_DIR/nebula-gateway" ] || die "nebula-gateway not found at $BIN_DIR/nebula-gateway"

DIRECT_URL="http://127.0.0.1:${ENGINE_PORT}"
REMOTE_URL="http://${CONTROL_HOST}:${REMOTE_PORT}"
EMBED_URL="http://${CONTROL_HOST}:${EMBED_PORT}"

proc_running() { pgrep -f "$BIN_DIR/$1" >/dev/null 2>&1; }

start_gateways() {
  mkdir -p "$LOG_DIR"
  if proc_running nebula-gateway && [ -n "${GATEWAYS_STARTED:-}" ]; then
    log "gateways already started by this run"
    return
  fi
  log "starting remote gateway  :$REMOTE_PORT"
  NEBULA_ROUTER_MODE=remote nohup "$BIN_DIR/nebula-gateway" \
    --listen-addr "0.0.0.0:$REMOTE_PORT" \
    --router-url "http://127.0.0.1:$ROUTER_PORT" \
    --etcd-endpoint "$ETCD_ENDPOINT" >"$LOG_DIR/gw-remote.log" 2>&1 &
  log "starting embedded gateway :$EMBED_PORT"
  NEBULA_ROUTER_MODE=embedded nohup "$BIN_DIR/nebula-gateway" \
    --listen-addr "0.0.0.0:$EMBED_PORT" \
    --etcd-endpoint "$ETCD_ENDPOINT" >"$LOG_DIR/gw-embedded.log" 2>&1 &
  GATEWAYS_STARTED=1
  sleep 2
}

wait_http() {
  local url="$1" name="$2" i
  for i in $(seq 1 30); do
    curl -fsS -m 2 "$url" >/dev/null 2>&1 && return 0
    sleep 1
  done
  die "$name not ready at $url"
}

wait_engine() {
  local i code
  for i in $(seq 1 60); do
    code="$(kubectl exec -n "$NAMESPACE" "$ENGINE_POD" -- \
      curl -sS -m 3 -o /dev/null -w '%{http_code}' "http://127.0.0.1:${ENGINE_PORT}/v1/models" 2>/dev/null || true)"
    [ "$code" = "200" ] && return 0
    sleep 5
  done
  die "engine Pod $NAMESPACE/$ENGINE_POD not ready on :$ENGINE_PORT"
}

# One official bench run against $1; prints compact metrics on one line.
bench() {
  local url="$1"
  kubectl exec -n "$NAMESPACE" "$ENGINE_POD" -- vllm bench serve \
    --backend openai-chat --base-url "$url" --endpoint /v1/chat/completions \
    --model "$MODEL_UID" --tokenizer "$TOKENIZER" --trust-remote-code \
    --dataset-name random --random-input-len "$INPUT_LEN" --random-output-len "$OUTPUT_LEN" \
    --random-range-ratio 0 --num-prompts "$NUM_PROMPTS" --max-concurrency "$CONCURRENCY" \
    --num-warmups "$WARMUPS" --disable-tqdm \
    --percentile-metrics ttft,itl --metric-percentiles 50,99 2>/dev/null \
    | grep -E 'Request throughput|Mean TTFT|P99 TTFT|P99 ITL' \
    | sed -E 's/[A-Za-z()\/]//g; s/ +/ /g' | tr '\n' ' '
  echo
}

main() {
  docker start "$ETCD_CONTAINER" >/dev/null 2>&1 || true
  wait_engine
  start_gateways
  wait_http "$REMOTE_URL/healthz" "remote gateway"
  wait_http "$EMBED_URL/healthz" "embedded gateway"

  log "workload: C=$CONCURRENCY N=$NUM_PROMPTS in=$INPUT_LEN out=$OUTPUT_LEN rounds=$ROUNDS"
  log "warmup"
  bench "$DIRECT_URL" >/dev/null
  bench "$REMOTE_URL" >/dev/null
  bench "$EMBED_URL" >/dev/null

  local r
  for r in $(seq 1 "$ROUNDS"); do
    echo "== round $r =="
    printf '  A direct   : '; bench "$DIRECT_URL"
    printf '  B remote   : '; bench "$REMOTE_URL"
    printf '  C embedded : '; bench "$EMBED_URL"
  done

  echo
  log "expected: C(embedded) P99 TTFT well below B(remote) and close to A(direct)."
}

main "$@"
