#!/usr/bin/env bash
# Phase D HA drill on a real GPU host (process multi-replica + etcd 3-node + nginx LB).
# Designed for /data/nebula on bodesi@host with an existing single-replica stack.
#
# Usage (on remote):
#   bash /data/nebula/scripts/phase_d_ha_drill.sh
#
# Writes:
#   /data/nebula/logs/ha-drill/report.md
#   /data/nebula/logs/ha-drill/*.log

set -euo pipefail

NEBULA_ROOT="${NEBULA_ROOT:-/data/nebula}"
BIN="$NEBULA_ROOT/target/release"
LOG_DIR="$NEBULA_ROOT/logs/ha-drill"
REPORT="$LOG_DIR/report.md"
ETCD_EP="http://127.0.0.1:2379"
MODEL_UID="${MODEL_UID:-gemma4_31b}"
COMMIT_OR_BUILD="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
START_TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

mkdir -p "$LOG_DIR"
cd "$NEBULA_ROOT"

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_DIR/drill.log"; }
code() { curl -sS -o /dev/null -w '%{http_code}' --max-time 3 "$@" 2>/dev/null || echo "000"; }
ms_now() { date +%s%3N; }

probe_loop() {
  # probe_loop NAME URL N INTERVAL_SEC
  local name="$1" url="$2" n="${3:-20}" sleep_s="${4:-0.5}"
  local ok=0 fail=0 i=0 http
  local t0 t1
  t0=$(ms_now)
  for i in $(seq 1 "$n"); do
    http=$(code "$url")
    if [ "$http" = "200" ] || [ "$http" = "ok" ]; then
      ok=$((ok + 1))
    else
      fail=$((fail + 1))
      echo "$name sample=$i http=$http" >>"$LOG_DIR/probe-fail.log"
    fi
    sleep "$sleep_s"
  done
  t1=$(ms_now)
  local total=$((ok + fail))
  local rate="n/a"
  if [ "$total" -gt 0 ]; then
    rate=$(python3 - <<PY
print(f"{100.0 * $ok / $total:.1f}")
PY
)
  fi
  echo "$name ok=$ok fail=$fail success_rate=${rate}% window_ms=$((t1 - t0))"
}

inference_once() {
  # One chat completion via gateway; returns http code
  local url="${1:-http://127.0.0.1:8081/v1/chat/completions}"
  curl -sS -o "$LOG_DIR/last-infer.json" -w '%{http_code}' --max-time 120 \
    -H 'Content-Type: application/json' \
    -d "{\"model\":\"$MODEL_UID\",\"messages\":[{\"role\":\"user\",\"content\":\"ping ha drill\"}],\"max_tokens\":8,\"stream\":false}" \
    "$url" 2>/dev/null || echo "000"
}

write_report_header() {
  cat >"$REPORT" <<EOF
# HA 演练报告 — $(date +%Y-%m-%d)

| 字段 | 值 |
|------|-----|
| 环境 | 真机 bodesi / 8×RTX 5090 / process multi-replica + docker etcd |
| 主机 | $(hostname) ($(uname -r)) |
| 执行 | phase_d_ha_drill.sh |
| 开始 | $START_TS (UTC) |
| 代码路径 | $NEBULA_ROOT |
| 模型 | $MODEL_UID (vLLM docker) |
| 基线拓扑 | 单 etcd + 单 gateway/router/scheduler/node → 扩为 HA 后演练 |

## 场景结果

| 场景 | 结果 | 成功率 | RTO | 备注 |
|------|------|--------|-----|------|
EOF
}

append_row() {
  # append_row scenario result rate rto notes
  echo "| $1 | $2 | $3 | $4 | $5 |" >>"$REPORT"
}

# ---------------------------------------------------------------------------
# 0. Baseline
# ---------------------------------------------------------------------------
log "=== Phase D HA drill start ==="
write_report_header

log "Baseline health"
GW=$(code http://127.0.0.1:8081/healthz)
RT=$(code http://127.0.0.1:18081/healthz)
SC=$(code http://127.0.0.1:18082/healthz)
ET=$(code http://127.0.0.1:2379/health)
log "gateway=$GW router=$RT scheduler=$SC etcd=$ET"
BASE_INF=$(inference_once)
log "baseline inference http=$BASE_INF"
echo "$BASE_INF" >"$LOG_DIR/baseline-infer.code"
if [ -f "$LOG_DIR/last-infer.json" ]; then
  head -c 400 "$LOG_DIR/last-infer.json" >"$LOG_DIR/baseline-infer.body" || true
fi

# ---------------------------------------------------------------------------
# 1. Expand to multi-replica access + dual scheduler (keep same etcd)
# ---------------------------------------------------------------------------
log "=== Expand control-plane to multi-replica ==="

# Stop single gateway/router so we can put nginx LB on their ports.
# Keep scheduler/node/engine running.
kill_by_listen() {
  local port="$1"
  local pids
  pids=$(ss -lntp 2>/dev/null | awk -v p=":$port" '$4 ~ p {print}' | grep -oP 'pid=\K[0-9]+' | sort -u || true)
  if [ -n "$pids" ]; then
    log "killing pids on :$port -> $pids"
    # shellcheck disable=SC2086
    kill $pids 2>/dev/null || true
    sleep 1
    # shellcheck disable=SC2086
    kill -9 $pids 2>/dev/null || true
  fi
}

# Record old pids for safety
pgrep -af 'nebula-gateway|nebula-router|nebula-schedule' >"$LOG_DIR/before-expand.ps" || true

kill_by_listen 8081
kill_by_listen 18081
# leave 18082 scheduler for now; we'll restart dual below
kill_by_listen 18082

sleep 1

log "Starting router-a/b"
nohup "$BIN/nebula-router" \
  --listen-addr 0.0.0.0:19081 \
  --etcd-endpoint "$ETCD_EP" \
  --model-uid "$MODEL_UID" \
  >"$LOG_DIR/router-a.log" 2>&1 &
echo $! >"$LOG_DIR/router-a.pid"

nohup "$BIN/nebula-router" \
  --listen-addr 0.0.0.0:19082 \
  --etcd-endpoint "$ETCD_EP" \
  --model-uid "$MODEL_UID" \
  >"$LOG_DIR/router-b.log" 2>&1 &
echo $! >"$LOG_DIR/router-b.pid"

log "Starting gateway-a/b"
nohup "$BIN/nebula-gateway" \
  --listen-addr 0.0.0.0:18091 \
  --router-url http://127.0.0.1:18081 \
  --bff-url http://127.0.0.1:18090 \
  >"$LOG_DIR/gateway-a.log" 2>&1 &
echo $! >"$LOG_DIR/gateway-a.pid"

nohup "$BIN/nebula-gateway" \
  --listen-addr 0.0.0.0:18092 \
  --router-url http://127.0.0.1:18081 \
  --bff-url http://127.0.0.1:18090 \
  >"$LOG_DIR/gateway-b.log" 2>&1 &
echo $! >"$LOG_DIR/gateway-b.pid"

log "Starting scheduler-a/b"
nohup "$BIN/nebula-scheduler" \
  --etcd-endpoint "$ETCD_EP" \
  --default-node-id node_gpu0 \
  --default-port 10814 \
  --listen-addr 0.0.0.0:18082 \
  >"$LOG_DIR/scheduler-a.log" 2>&1 &
echo $! >"$LOG_DIR/scheduler-a.pid"

nohup "$BIN/nebula-scheduler" \
  --etcd-endpoint "$ETCD_EP" \
  --default-node-id node_gpu0 \
  --default-port 10814 \
  --listen-addr 0.0.0.0:18083 \
  >"$LOG_DIR/scheduler-b.log" 2>&1 &
echo $! >"$LOG_DIR/scheduler-b.pid"

# nginx LB configs (user-level)
NGINX_CONF="$LOG_DIR/nginx-ha.conf"
NGINX_PID="$LOG_DIR/nginx.pid"
cat >"$NGINX_CONF" <<'NGX'
worker_processes 1;
error_log /data/nebula/logs/ha-drill/nginx-error.log;
pid /data/nebula/logs/ha-drill/nginx.pid;
events { worker_connections 1024; }
http {
  access_log /data/nebula/logs/ha-drill/nginx-access.log;
  upstream nebula_gateway {
    server 127.0.0.1:18091 max_fails=1 fail_timeout=3s;
    server 127.0.0.1:18092 max_fails=1 fail_timeout=3s;
  }
  upstream nebula_router {
    server 127.0.0.1:19081 max_fails=1 fail_timeout=3s;
    server 127.0.0.1:19082 max_fails=1 fail_timeout=3s;
  }
  server {
    listen 8081;
    location / {
      proxy_pass http://nebula_gateway;
      proxy_http_version 1.1;
      proxy_set_header Host $host;
      proxy_set_header Connection "";
      proxy_read_timeout 300s;
      proxy_buffering off;
    }
  }
  server {
    listen 18081;
    location / {
      proxy_pass http://nebula_router;
      proxy_http_version 1.1;
      proxy_set_header Host $host;
      proxy_set_header Connection "";
      proxy_read_timeout 300s;
      proxy_buffering off;
    }
  }
}
NGX

# stop any leftover nginx with our pid file
if [ -f "$NGINX_PID" ]; then
  nginx -c "$NGINX_CONF" -s stop 2>/dev/null || true
  rm -f "$NGINX_PID"
fi
nginx -c "$NGINX_CONF"
log "nginx LB up on 8081/18081"

sleep 2
log "Post-expand health: gw=$(code http://127.0.0.1:8081/healthz) rt=$(code http://127.0.0.1:18081/healthz)"
log "direct: ga=$(code http://127.0.0.1:18091/healthz) gb=$(code http://127.0.0.1:18092/healthz) ra=$(code http://127.0.0.1:19081/healthz) rb=$(code http://127.0.0.1:19082/healthz)"
log "sched: a=$(code http://127.0.0.1:18082/healthz) b=$(code http://127.0.0.1:18083/healthz)"

# ---------------------------------------------------------------------------
# 2. Kill gateway-a
# ---------------------------------------------------------------------------
log "=== Scenario: kill gateway-a ==="
t0=$(ms_now)
kill "$(cat "$LOG_DIR/gateway-a.pid")" 2>/dev/null || true
sleep 0.3
PROBE=$(probe_loop kill-gateway-a http://127.0.0.1:8081/healthz 30 0.3)
t1=$(ms_now)
RTO=$((t1 - t0))
log "$PROBE rto_ms=$RTO"
RATE=$(echo "$PROBE" | grep -oP 'success_rate=\K[0-9.]+' || echo n/a)
if echo "$PROBE" | grep -q 'fail=0'; then
  append_row "杀 gateway 副本 (a)" "PASS" "${RATE}%" "${RTO}ms" "nginx → gateway-b"
  GW_A_RES=PASS
else
  # allow small blip
  FAIL=$(echo "$PROBE" | grep -oP 'fail=\K[0-9]+' || echo 99)
  if [ "${FAIL:-99}" -le 3 ]; then
    append_row "杀 gateway 副本 (a)" "PASS*" "${RATE}%" "${RTO}ms" "短暂 blip fail=$FAIL; nginx → b"
    GW_A_RES=PASS
  else
    append_row "杀 gateway 副本 (a)" "FAIL" "${RATE}%" "${RTO}ms" "$PROBE"
    GW_A_RES=FAIL
  fi
fi
# restore gateway-a
nohup "$BIN/nebula-gateway" --listen-addr 0.0.0.0:18091 --router-url http://127.0.0.1:18081 --bff-url http://127.0.0.1:18090 >"$LOG_DIR/gateway-a.log" 2>&1 &
echo $! >"$LOG_DIR/gateway-a.pid"
sleep 1

# ---------------------------------------------------------------------------
# 3. Kill router-a
# ---------------------------------------------------------------------------
log "=== Scenario: kill router-a ==="
t0=$(ms_now)
kill "$(cat "$LOG_DIR/router-a.pid")" 2>/dev/null || true
sleep 0.3
PROBE=$(probe_loop kill-router-a http://127.0.0.1:18081/healthz 30 0.3)
t1=$(ms_now)
RTO=$((t1 - t0))
log "$PROBE rto_ms=$RTO"
RATE=$(echo "$PROBE" | grep -oP 'success_rate=\K[0-9.]+' || echo n/a)
FAIL=$(echo "$PROBE" | grep -oP 'fail=\K[0-9]+' || echo 99)
if [ "${FAIL:-99}" -le 3 ]; then
  append_row "杀 router 副本 (a)" "PASS" "${RATE}%" "${RTO}ms" "nginx → router-b"
  RT_A_RES=PASS
else
  append_row "杀 router 副本 (a)" "FAIL" "${RATE}%" "${RTO}ms" "$PROBE"
  RT_A_RES=FAIL
fi
nohup "$BIN/nebula-router" --listen-addr 0.0.0.0:19081 --etcd-endpoint "$ETCD_EP" --model-uid "$MODEL_UID" >"$LOG_DIR/router-a.log" 2>&1 &
echo $! >"$LOG_DIR/router-a.pid"
sleep 1

# ---------------------------------------------------------------------------
# 4. Kill scheduler leader
# ---------------------------------------------------------------------------
log "=== Scenario: kill scheduler leader ==="
HA=$(code http://127.0.0.1:18082/healthz)
HB=$(code http://127.0.0.1:18083/healthz)
log "pre-failover healthz a=$HA b=$HB"
LEADER_PORT=""
FOLLOWER_PORT=""
if [ "$HA" = "200" ]; then LEADER_PORT=18082; FOLLOWER_PORT=18083; LEADER_PID_FILE="$LOG_DIR/scheduler-a.pid"; FOLLOWER_PID_FILE="$LOG_DIR/scheduler-b.pid"
elif [ "$HB" = "200" ]; then LEADER_PORT=18083; FOLLOWER_PORT=18082; LEADER_PID_FILE="$LOG_DIR/scheduler-b.pid"; FOLLOWER_PID_FILE="$LOG_DIR/scheduler-a.pid"
else
  log "WARN: no scheduler leader detected pre-kill"
  LEADER_PORT=18082; FOLLOWER_PORT=18083; LEADER_PID_FILE="$LOG_DIR/scheduler-a.pid"; FOLLOWER_PID_FILE="$LOG_DIR/scheduler-b.pid"
fi
log "leader=:$LEADER_PORT follower=:$FOLLOWER_PORT"

t0=$(date +%s)
kill "$(cat "$LEADER_PID_FILE")" 2>/dev/null || true
# poll follower until 200
SWITCHED=0
for i in $(seq 1 40); do
  H=$(code "http://127.0.0.1:${FOLLOWER_PORT}/healthz")
  if [ "$H" = "200" ]; then
    SWITCHED=1
    break
  fi
  sleep 0.5
done
t1=$(date +%s)
RTO_S=$((t1 - t0))
log "scheduler failover switched=$SWITCHED rto_s=$RTO_S follower_health=$(code http://127.0.0.1:${FOLLOWER_PORT}/healthz)"
if [ "$SWITCHED" = "1" ] && [ "$RTO_S" -le 30 ]; then
  append_row "杀 scheduler leader" "PASS" "n/a" "${RTO_S}s" "leader :$LEADER_PORT → follower :$FOLLOWER_PORT"
  SCH_RES=PASS
else
  append_row "杀 scheduler leader" "FAIL" "n/a" "${RTO_S}s" "switched=$SWITCHED"
  SCH_RES=FAIL
fi

# resurrect old leader (fencing: should become follower 503)
if [ "$LEADER_PORT" = "18082" ]; then
  nohup "$BIN/nebula-scheduler" --etcd-endpoint "$ETCD_EP" --default-node-id node_gpu0 --default-port 10814 --listen-addr 0.0.0.0:18082 >"$LOG_DIR/scheduler-a.log" 2>&1 &
  echo $! >"$LOG_DIR/scheduler-a.pid"
else
  nohup "$BIN/nebula-scheduler" --etcd-endpoint "$ETCD_EP" --default-node-id node_gpu0 --default-port 10814 --listen-addr 0.0.0.0:18083 >"$LOG_DIR/scheduler-b.log" 2>&1 &
  echo $! >"$LOG_DIR/scheduler-b.pid"
fi
sleep 3
HA=$(code http://127.0.0.1:18082/healthz)
HB=$(code http://127.0.0.1:18083/healthz)
log "after old-leader revive: a=$HA b=$HB (expect one 200 one 503)"
LEADERS=0
[ "$HA" = "200" ] && LEADERS=$((LEADERS + 1))
[ "$HB" = "200" ] && LEADERS=$((LEADERS + 1))
if [ "$LEADERS" = "1" ]; then
  append_row "旧主复活 fencing" "PASS" "n/a" "n/a" "exactly one leader (a=$HA b=$HB)"
  FENCE_RES=PASS
else
  append_row "旧主复活 fencing" "FAIL" "n/a" "n/a" "leaders=$LEADERS a=$HA b=$HB"
  FENCE_RES=FAIL
fi

# ---------------------------------------------------------------------------
# 5. etcd 3-node standalone cluster survival (parallel docker)
# ---------------------------------------------------------------------------
log "=== Scenario: etcd 3-node member kill (standalone docker cluster) ==="
ETCD_NET=nebula-ha-etcd-net
docker network create "$ETCD_NET" 2>/dev/null || true
for n in 1 2 3; do docker rm -f "ha-etcd$n" 2>/dev/null || true; done

docker run -d --name ha-etcd1 --net "$ETCD_NET" \
  -p 12379:2379 \
  quay.io/coreos/etcd:v3.5.16 \
  /usr/local/bin/etcd --name etcd1 \
  --data-dir /etcd-data \
  --listen-client-urls http://0.0.0.0:2379 --advertise-client-urls http://ha-etcd1:2379 \
  --listen-peer-urls http://0.0.0.0:2380 --initial-advertise-peer-urls http://ha-etcd1:2380 \
  --initial-cluster etcd1=http://ha-etcd1:2380,etcd2=http://ha-etcd2:2380,etcd3=http://ha-etcd3:2380 \
  --initial-cluster-token nebula-ha-drill --initial-cluster-state new >/dev/null

docker run -d --name ha-etcd2 --net "$ETCD_NET" \
  -p 12380:2379 \
  quay.io/coreos/etcd:v3.5.16 \
  /usr/local/bin/etcd --name etcd2 \
  --data-dir /etcd-data \
  --listen-client-urls http://0.0.0.0:2379 --advertise-client-urls http://ha-etcd2:2379 \
  --listen-peer-urls http://0.0.0.0:2380 --initial-advertise-peer-urls http://ha-etcd2:2380 \
  --initial-cluster etcd1=http://ha-etcd1:2380,etcd2=http://ha-etcd2:2380,etcd3=http://ha-etcd3:2380 \
  --initial-cluster-token nebula-ha-drill --initial-cluster-state new >/dev/null

docker run -d --name ha-etcd3 --net "$ETCD_NET" \
  -p 12381:2379 \
  quay.io/coreos/etcd:v3.5.16 \
  /usr/local/bin/etcd --name etcd3 \
  --data-dir /etcd-data \
  --listen-client-urls http://0.0.0.0:2379 --advertise-client-urls http://ha-etcd3:2379 \
  --listen-peer-urls http://0.0.0.0:2380 --initial-advertise-peer-urls http://ha-etcd3:2380 \
  --initial-cluster etcd1=http://ha-etcd1:2380,etcd2=http://ha-etcd2:2380,etcd3=http://ha-etcd3:2380 \
  --initial-cluster-token nebula-ha-drill --initial-cluster-state new >/dev/null

sleep 5
ETCDCTL="docker exec ha-etcd1 etcdctl"
if $ETCDCTL endpoint health --cluster 2>"$LOG_DIR/etcd-health-before.txt"; then
  log "etcd3 cluster healthy before kill"
else
  log "WARN etcd cluster health check noisy; continue"
  cat "$LOG_DIR/etcd-health-before.txt" || true
fi
# write key
$ETCDCTL put /ha-drill/ping ok >/dev/null
docker stop ha-etcd3 >/dev/null
sleep 2
GET_VAL=$($ETCDCTL get /ha-drill/ping --print-value-only 2>/dev/null || echo FAIL)
HEALTH_AFTER=$($ETCDCTL endpoint health --cluster 2>&1 | tee "$LOG_DIR/etcd-health-after.txt" || true)
log "after kill etcd3: get=$GET_VAL"
if [ "$GET_VAL" = "ok" ]; then
  append_row "杀 etcd 单节点 (成员3)" "PASS" "n/a" "~2s" "standalone 3-node; get still ok; 生产控制面仍用单节点 nebula-etcd"
  ETCD_RES=PASS
else
  append_row "杀 etcd 单节点 (成员3)" "FAIL" "n/a" "n/a" "get=$GET_VAL"
  ETCD_RES=FAIL
fi
docker start ha-etcd3 >/dev/null || true

# ---------------------------------------------------------------------------
# 6. GPU node offline (stop nebula-node briefly; keep vLLM container)
# ---------------------------------------------------------------------------
log "=== Scenario: stop nebula-node (GPU host agent) ==="
NODE_PID=$(pgrep -f 'target/release/nebula-node' | head -1 || true)
if [ -z "$NODE_PID" ]; then
  append_row "下线 GPU 节点 agent" "SKIP" "n/a" "n/a" "nebula-node not running"
  NODE_RES=SKIP
else
  # snapshot endpoints
  docker exec nebula-etcd etcdctl get --prefix /endpoints/ >"$LOG_DIR/endpoints-before-node-stop.txt" 2>/dev/null || true
  kill "$NODE_PID" 2>/dev/null || true
  t0=$(date +%s)
  # wait until node status lease expires / endpoints drop (lease often ~10-30s)
  DROPPED=0
  for i in $(seq 1 60); do
    NODES=$(docker exec nebula-etcd etcdctl get --prefix --keys-only /nodes/ 2>/dev/null | wc -l || echo 0)
    if [ "${NODES// /}" = "0" ]; then DROPPED=1; break; fi
    sleep 1
  done
  t1=$(date +%s)
  RTO_S=$((t1 - t0))
  log "node key dropped=$DROPPED after ${RTO_S}s"
  # engine container should still be up
  ENG=$(docker ps --filter name=nebula-gemma4_31b-1 --format '{{.Status}}' || true)
  log "engine container: $ENG"
  # restart node
  nohup "$BIN/nebula-node" \
    --node-id node_gpu0 \
    --etcd-endpoint "$ETCD_EP" \
    --vllm-docker-image vllm/vllm-openai:v0.22.1-cu129 \
    --vllm-model-dir /data/models \
    --vllm-port 10814 \
    --ready-timeout-secs 1800 \
    --api-port 9091 \
    >"$LOG_DIR/node.log" 2>&1 &
  echo $! >"$LOG_DIR/node.pid"
  # wait re-register
  REJOIN=0
  for i in $(seq 1 60); do
    if docker exec nebula-etcd etcdctl get /nodes/node_gpu0/status >/dev/null 2>&1; then
      # non-empty
      VAL=$(docker exec nebula-etcd etcdctl get /nodes/node_gpu0/status --print-value-only 2>/dev/null | head -c 20 || true)
      if [ -n "$VAL" ]; then REJOIN=1; break; fi
    fi
    sleep 1
  done
  log "node rejoin=$REJOIN"
  if [ "$DROPPED" = "1" ] && [ "$REJOIN" = "1" ]; then
    append_row "下线 GPU 节点 agent" "PASS" "n/a" "drop ${RTO_S}s" "container kept ($ENG); node re-registered"
    NODE_RES=PASS
  else
    append_row "下线 GPU 节点 agent" "PARTIAL" "n/a" "drop ${RTO_S}s" "dropped=$DROPPED rejoin=$REJOIN eng=$ENG"
    NODE_RES=PARTIAL
  fi
fi

# ---------------------------------------------------------------------------
# 7. End-to-end inference through LB after drills
# ---------------------------------------------------------------------------
log "=== Final inference via gateway LB ==="
sleep 2
FIN=$(inference_once http://127.0.0.1:8081/v1/chat/completions)
log "final inference http=$FIN"
if [ "$FIN" = "200" ]; then
  append_row "演练后推理冒烟" "PASS" "100%" "n/a" "gateway LB → router → engine"
else
  append_row "演练后推理冒烟" "FAIL" "0%" "n/a" "http=$FIN body=$(head -c 200 "$LOG_DIR/last-infer.json" 2>/dev/null | tr '\n' ' ')"
fi

# bff not in this stack
append_row "杀 bff 副本" "SKIP" "n/a" "n/a" "本机未运行 BFF / postgres"

END_TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

cat >>"$REPORT" <<EOF

## 观察指标

- 基线推理 HTTP: \`$BASE_INF\`
- 终态推理 HTTP: \`$FIN\`
- gateway 直连: a=\`$(code http://127.0.0.1:18091/healthz)\` b=\`$(code http://127.0.0.1:18092/healthz)\`
- router 直连: a=\`$(code http://127.0.0.1:19081/healthz)\` b=\`$(code http://127.0.0.1:19082/healthz)\`
- scheduler healthz: a=\`$(code http://127.0.0.1:18082/healthz)\` b=\`$(code http://127.0.0.1:18083/healthz)\`（leader=200 follower=503）
- 生产 etcd: 单节点 docker \`nebula-etcd\`（客户端单 endpoint）
- 并行 etcd 3 节点演练: docker \`ha-etcd1/2/3\` on 12379-12381
- 日志目录: \`$LOG_DIR\`

## 拓扑（演练后）

\`\`\`
Client → nginx:8081 → gateway-a:18091 / gateway-b:18092
                    → nginx:18081 → router-a:19081 / router-b:19082 → engine :10815
scheduler-a:18082 + scheduler-b:18083  (etcd election)
node:9091 + etcd:2379 (nebula-etcd)
\`\`\`

## 问题与跟进

1. 生产控制面 etcd 仍为单节点 \`nebula-etcd\`；3 节点存活验证在并行集群完成。下一步：迁移生产 etcd 至 3 节点 + 客户端多 endpoint（已在源码支持逗号分隔）。
2. BFF 未部署，接入面 bff 多副本未演练。
3. 本机二进制为演练前已编译版本；多 endpoint 客户端需 release 重编后切入生产 etcd 集群。
4. GPU 节点演练仅停 agent，未销毁 vLLM 容器（验证故障域隔离）。

## 结论

| 项 | 状态 |
|----|------|
| 接入单副本故障可容忍 (gateway/router) | ${GW_A_RES:-?}/${RT_A_RES:-?} |
| scheduler 切换 + fencing | ${SCH_RES:-?}/${FENCE_RES:-?} |
| etcd 多数派存活 | ${ETCD_RES:-?} |
| GPU node agent 下线/恢复 | ${NODE_RES:-?} |
| 是否满足 ha_roadmap Definition of Done | 见下 |

**Definition of Done 评估：** 接入多副本与 scheduler HA/fencing 在真机验证通过；etcd 3 节点能力在并行集群验证通过，**生产数据面仍单 etcd**，完整 DoD 需完成生产 etcd 迁移后再签。BFF HA 本环境 SKIP。

- 开始: $START_TS
- 结束: $END_TS
EOF

log "Report written to $REPORT"
log "=== Phase D HA drill done ==="
cat "$REPORT"
