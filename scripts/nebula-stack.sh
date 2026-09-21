#!/usr/bin/env bash
# nebula-stack.sh — start / stop / status for the Nebula control plane,
# the etcd container, and the k8s-controller-managed model Pod.
#
# Run this on the control-plane host (etcd + gateway/router/bff + k8s-controller).
# Defaults match the "xinference" control node; override via env vars as needed.
#
#   ./scripts/nebula-stack.sh status   # show current state (safe, read-only)
#   ./scripts/nebula-stack.sh stop     # drop pod expectation -> delete pod -> stop control plane -> stop etcd
#   ./scripts/nebula-stack.sh start    # start etcd -> start control plane -> restore pod expectation
#
# Why delete the etcd deployment first on stop: nebula-k8s-controller reconciles
# `/deployments/{model_uid}` into Pods, so deleting the Pod alone gets recreated.
set -euo pipefail

# ---- config (override with env vars) -----------------------------------------
BIN_DIR="${BIN_DIR:-$HOME/nebula-src/target/release}"
LOG_DIR="${LOG_DIR:-$HOME/nebula-stack-logs}"
ETCD_CONTAINER="${ETCD_CONTAINER:-nebula-etcd}"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:12379}"   # host -> etcd (docker -p 12379:2379)
ETCDCTL_ENDPOINT="${ETCDCTL_ENDPOINT:-127.0.0.1:2379}"     # inside the etcd container
GATEWAY_PORT="${GATEWAY_PORT:-8081}"
ROUTER_PORT="${ROUTER_PORT:-18081}"
BFF_PORT="${BFF_PORT:-18090}"
BFF_DATABASE_URL="${BFF_DATABASE_URL:-postgresql://postgres:postgres@10.244.0.140:5432/nebula}"
K8S_NAMESPACE="${K8S_NAMESPACE:-powerllm}"
GPU_NODE="${GPU_NODE:-xgateway}"
MODEL_UID="${MODEL_UID:-qwen2.5-7b-nebula}"
MODEL_POD="${MODEL_POD:-nebula-${MODEL_UID}}"
DEPLOYMENT_JSON="${DEPLOYMENT_JSON:-{\"model_uid\":\"${MODEL_UID}\",\"desired_state\":\"running\",\"replicas\":1,\"version\":1}}"

# ---- helpers -----------------------------------------------------------------
log() { printf '[nebula-stack] %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
etcd_container_running() { docker ps --format '{{.Names}}' 2>/dev/null | grep -qx "$ETCD_CONTAINER"; }
etcdctl_() { docker exec "$ETCD_CONTAINER" etcdctl --endpoints="$ETCDCTL_ENDPOINT" "$@"; }
proc_running() { pgrep -f "$BIN_DIR/$1" >/dev/null 2>&1; }

ensure_etcd_up() {
  if ! etcd_container_running; then
    log "starting etcd container ($ETCD_CONTAINER)"
    docker start "$ETCD_CONTAINER" >/dev/null
  fi
  local i
  for i in $(seq 1 30); do
    etcdctl_ endpoint health >/dev/null 2>&1 && return 0
    sleep 1
  done
  log "WARN: etcd did not report healthy within 30s"
  return 1
}

start_procs() {
  mkdir -p "$LOG_DIR"
  if proc_running nebula-router; then
    log "router already running"
  else
    log "starting router :$ROUTER_PORT"
    nohup "$BIN_DIR/nebula-router" --listen-addr "0.0.0.0:$ROUTER_PORT" \
      --etcd-endpoint "$ETCD_ENDPOINT" >"$LOG_DIR/router.log" 2>&1 &
  fi
  sleep 1
  if proc_running nebula-gateway; then
    log "gateway already running"
  else
    log "starting gateway :$GATEWAY_PORT"
    nohup "$BIN_DIR/nebula-gateway" --listen-addr "0.0.0.0:$GATEWAY_PORT" \
      --router-url "http://127.0.0.1:$ROUTER_PORT" \
      --etcd-endpoint "$ETCD_ENDPOINT" >"$LOG_DIR/gateway.log" 2>&1 &
  fi
  if proc_running nebula-bff; then
    log "bff already running"
  else
    log "starting bff :$BFF_PORT"
    nohup "$BIN_DIR/nebula-bff" --listen-addr "0.0.0.0:$BFF_PORT" \
      --etcd-endpoint "$ETCD_ENDPOINT" \
      --database-url "$BFF_DATABASE_URL" >"$LOG_DIR/bff.log" 2>&1 &
  fi
  if proc_running nebula-k8s-controller; then
    log "k8s-controller already running"
  else
    log "starting k8s-controller (namespace=$K8S_NAMESPACE gpu-node=$GPU_NODE)"
    nohup "$BIN_DIR/nebula-k8s-controller" --etcd-endpoint "$ETCD_ENDPOINT" \
      --namespace "$K8S_NAMESPACE" --gpu-node "$GPU_NODE" >"$LOG_DIR/k8s-controller.log" 2>&1 &
  fi
  sleep 2
}

cmd_status() {
  echo "--- etcd container ---"
  docker ps -a --filter "name=^/${ETCD_CONTAINER}$" --format '{{.Names}}  {{.Status}}' 2>/dev/null || true
  echo "--- control plane ---"
  pgrep -af "$BIN_DIR/nebula-" || echo "(none)"
  echo "--- pods ($K8S_NAMESPACE) ---"
  if have kubectl; then
    kubectl get pods -n "$K8S_NAMESPACE" 2>/dev/null | grep -E "NAME|nebula" || echo "(none)"
  else
    echo "(kubectl not available)"
  fi
  echo "--- /deployments/$MODEL_UID ---"
  if etcd_container_running; then
    etcdctl_ get "/deployments/$MODEL_UID" --print-value-only 2>/dev/null || echo "(unset)"
  else
    echo "(etcd stopped)"
  fi
}

cmd_start() {
  ensure_etcd_up
  start_procs
  log "restoring deployment expectation /deployments/$MODEL_UID"
  etcdctl_ put "/deployments/$MODEL_UID" "$DEPLOYMENT_JSON" >/dev/null
  log "done — k8s-controller will (re)create pod $MODEL_POD"
  cmd_status
}

cmd_stop() {
  # 1) drop desired state first so the controller does not recreate the pod
  if ensure_etcd_up; then
    log "deleting deployment expectation /deployments/$MODEL_UID"
    etcdctl_ del "/deployments/$MODEL_UID" >/dev/null || true
  fi
  # 2) delete all replica pods (label selector) + the legacy single pod name
  if have kubectl; then
    log "deleting model pods $K8S_NAMESPACE (label nebula.model_uid=$MODEL_UID)"
    kubectl delete pod -n "$K8S_NAMESPACE" "$MODEL_POD" --ignore-not-found --wait=false >/dev/null 2>&1 || true
    kubectl delete pod -n "$K8S_NAMESPACE" -l "nebula.model_uid=$MODEL_UID" --ignore-not-found --wait=false >/dev/null 2>&1 || true
  fi
  # 3) stop the control plane
  local p
  for p in nebula-k8s-controller nebula-bff nebula-gateway nebula-router; do
    if proc_running "$p"; then
      log "stopping $p"
      pkill -f "$BIN_DIR/$p" || true
    fi
  done
  sleep 2
  # 4) stop the etcd container
  if etcd_container_running; then
    log "stopping etcd container ($ETCD_CONTAINER)"
    docker stop "$ETCD_CONTAINER" >/dev/null || true
  fi
  cmd_status
}

case "${1:-status}" in
  start) cmd_start ;;
  stop) cmd_stop ;;
  status) cmd_status ;;
  *)
    echo "usage: $0 {start|stop|status}"
    exit 1
    ;;
esac
