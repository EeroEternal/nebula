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
# pd_split shared-KV components (optional; only needed for topology=pd_split)
LMCACHE_IMAGE="${LMCACHE_IMAGE:-}"            # e.g. nebula/vllm:0.23.0-lmcache0.5.5
export NEBULA_APPROVED_PD_IMAGES="${NEBULA_APPROVED_PD_IMAGES-${LMCACHE_IMAGE}}"
LMCACHE_SERVER_CONTAINER="${LMCACHE_SERVER_CONTAINER:-nebula-lmcache}"
LMCACHE_CTRL_CONTAINER="${LMCACHE_CTRL_CONTAINER:-nebula-lmcache-ctrl}"
LMCACHE_REMOTE_PORT="${LMCACHE_REMOTE_PORT:-6800}"
LMCACHE_CTRL_PORT="${LMCACHE_CTRL_PORT:-9000}"
KV_HASH_SEED="${NEBULA_KV_HASH_SEED:-42}"
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

start_one_proc() {
  local p="$1"
  mkdir -p "$LOG_DIR"
  if proc_running "$p"; then
    log "$p already running"
    return 0
  fi
  case "$p" in
    nebula-router)
      log "starting router :$ROUTER_PORT"
      nohup "$BIN_DIR/nebula-router" --listen-addr "0.0.0.0:$ROUTER_PORT" \
        --etcd-endpoint "$ETCD_ENDPOINT" >"$LOG_DIR/router.log" 2>&1 &
      ;;
    nebula-gateway)
      log "starting gateway :$GATEWAY_PORT"
      nohup "$BIN_DIR/nebula-gateway" --listen-addr "0.0.0.0:$GATEWAY_PORT" \
        --router-url "http://127.0.0.1:$ROUTER_PORT" \
        --etcd-endpoint "$ETCD_ENDPOINT" >"$LOG_DIR/gateway.log" 2>&1 &
      ;;
    nebula-bff)
      log "starting bff :$BFF_PORT"
      nohup "$BIN_DIR/nebula-bff" --listen-addr "0.0.0.0:$BFF_PORT" \
        --etcd-endpoint "$ETCD_ENDPOINT" \
        --database-url "$BFF_DATABASE_URL" >"$LOG_DIR/bff.log" 2>&1 &
      ;;
    nebula-k8s-controller)
      log "starting k8s-controller (namespace=$K8S_NAMESPACE gpu-node=$GPU_NODE)"
      nohup "$BIN_DIR/nebula-k8s-controller" --etcd-endpoint "$ETCD_ENDPOINT" \
        --namespace "$K8S_NAMESPACE" --gpu-node "$GPU_NODE" >"$LOG_DIR/k8s-controller.log" 2>&1 &
      ;;
  esac
}

start_procs() {
  start_one_proc nebula-router
  sleep 1
  start_one_proc nebula-gateway
  start_one_proc nebula-bff
  start_one_proc nebula-k8s-controller
  sleep 2
}

cmd_status() {
  echo "--- etcd container ---"
  docker ps -a --filter "name=^/${ETCD_CONTAINER}$" --format '{{.Names}}  {{.Status}}' 2>/dev/null || true
  echo "--- lmcache (pd_split) ---"
  docker ps -a --filter "name=^/${LMCACHE_SERVER_CONTAINER}$" --filter "name=^/${LMCACHE_CTRL_CONTAINER}$" \
    --format '{{.Names}}  {{.Status}}' 2>/dev/null || true
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

# pd_split shared-KV components. Neither is required for replicated
# deployments: skip silently unless LMCACHE_IMAGE is set.
ensure_lmcache_up() {
  if [ -z "$LMCACHE_IMAGE" ]; then
    log "LMCACHE_IMAGE unset — skipping lmcache_server / lmcache_controller (replicated topologies only)"
    return 0
  fi
  for spec in "$LMCACHE_SERVER_CONTAINER:lmcache_server:$LMCACHE_REMOTE_PORT" \
              "$LMCACHE_CTRL_CONTAINER:lmcache_controller:$LMCACHE_CTRL_PORT"; do
    name="${spec%%:*}"; rest="${spec#*:}"; cmd="${rest%%:*}"; port="${rest##*:}"
    if docker ps --format '{{.Names}}' | grep -qx "$name"; then
      log "$name already running"
      continue
    fi
    log "starting $name ($cmd, port $port)"
    docker rm -f "$name" >/dev/null 2>&1 || true
    if [ "$cmd" = "lmcache_server" ]; then
      docker run -d --name "$name" --network host -e PYTHONHASHSEED="$KV_HASH_SEED" \
        --entrypoint lmcache_server "$LMCACHE_IMAGE" 0.0.0.0 "$port" cpu >/dev/null
    else
      # The controller must share the engines' hash seed (block-hash domain).
      docker run -d --name "$name" --network host -e PYTHONHASHSEED="$KV_HASH_SEED" \
        --entrypoint lmcache_controller "$LMCACHE_IMAGE" \
        --host 0.0.0.0 --port "$port" --monitor-port "$((port + 1))" >/dev/null
    fi
  done

  # Deployment ordering constraint (pool.md §pd_split): engines crash-loop if
  # lmcache_controller is unreachable at boot, and restart if lmcache_server is
  # not yet listening. Wait for both to report ready before starting controller.
  local i srv_ok=0 ctrl_ok=0
  for i in $(seq 1 30); do
    if docker ps --format '{{.Names}}' 2>/dev/null | grep -qx "$LMCACHE_SERVER_CONTAINER" && \
       (docker logs "$LMCACHE_SERVER_CONTAINER" 2>&1 | grep -q "Server started" || \
        bash -c "</dev/tcp/127.0.0.1/$LMCACHE_REMOTE_PORT" >/dev/null 2>&1); then
      srv_ok=1
    fi
    if curl -sf --max-time 2 -X POST "http://127.0.0.1:${LMCACHE_CTRL_PORT}/lookup" \
         -H 'Content-Type: application/json' -d '{"tokens":[1]}' >/dev/null 2>&1; then
      ctrl_ok=1
    fi
    if [ "$srv_ok" = "1" ] && [ "$ctrl_ok" = "1" ]; then
      log "lmcache_server (:$LMCACHE_REMOTE_PORT) and lmcache_controller (:$LMCACHE_CTRL_PORT) ready"
      return 0
    fi
    sleep 1
  done
  log "ERROR: lmcache components did not become ready within 30s (srv_ok=$srv_ok ctrl_ok=$ctrl_ok)"
  return 1
}

cmd_start() {
  ensure_etcd_up
  ensure_lmcache_up
  start_procs
  local existing=""
  existing="$(etcdctl_ get "/deployments/$MODEL_UID" --print-value-only 2>/dev/null || true)"
  if [ -n "$existing" ] && [ "${FORCE_DEPLOYMENT_RESET:-0}" != "1" ]; then
    log "preserving existing deployment expectation /deployments/$MODEL_UID (set FORCE_DEPLOYMENT_RESET=1 to overwrite)"
  else
    log "seeding default deployment expectation /deployments/$MODEL_UID"
    etcdctl_ put "/deployments/$MODEL_UID" "$DEPLOYMENT_JSON" >/dev/null
  fi
  log "done — k8s-controller will reconcile pod(s) for $MODEL_UID"
  cmd_status
}

# Rolling control-plane binary restart: restarts each binary one at a time,
# waiting for the old process to release its listening port before starting the
# new binary, while keeping etcd, /deployments/$MODEL_UID, lmcache containers,
# and running engine Pods untouched.
cmd_restart_control_plane() {
  ensure_etcd_up
  local p i failed=0
  for p in nebula-router nebula-gateway nebula-bff nebula-k8s-controller; do
    if proc_running "$p"; then
      log "stopping $p for binary upgrade"
      pkill -f "$BIN_DIR/$p" || true
      for i in $(seq 1 20); do
        proc_running "$p" || break
        sleep 0.5
      done
      if proc_running "$p"; then
        log "WARN: $p did not exit within 10s; sending SIGKILL"
        pkill -9 -f "$BIN_DIR/$p" || true
        sleep 0.5
      fi
    fi
    start_one_proc "$p"
    sleep 1
    if ! proc_running "$p"; then
      log "ERROR: $p failed to stay running after restart"
      failed=1
    fi
  done
  cmd_status
  if [ "$failed" -ne 0 ]; then
    log "ERROR: one or more control-plane binaries failed to restart"
    return 1
  fi
  log "control plane restarted (deployments and engine pods untouched)"
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
  # 2b) stop pd_split shared-KV components (KV is a cache; loss is expected)
  for name in "$LMCACHE_CTRL_CONTAINER" "$LMCACHE_SERVER_CONTAINER"; do
    if docker ps --format '{{.Names}}' | grep -qx "$name"; then
      log "stopping $name"
      docker rm -f "$name" >/dev/null 2>&1 || true
    fi
  done

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
  restart-control-plane) cmd_restart_control_plane ;;
  stop) cmd_stop ;;
  status) cmd_status ;;
  *)
    echo "usage: $0 {start|restart-control-plane|stop|status}"
    exit 1
    ;;
esac
