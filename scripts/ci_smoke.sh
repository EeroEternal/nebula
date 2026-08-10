#!/usr/bin/env bash
# CI-friendly stack smoke: etcd mock endpoint + gateway/router/BFF, no GPU.
# Requires: docker compose (etcd+postgres), release binaries in target/release/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COMPOSE_FILE="${COMPOSE_FILE:-$ROOT/docker-compose.yml}"
BIN="$ROOT/target/release"
mkdir -p "$ROOT/logs"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
MOCK_PORT="${MOCK_PORT:-18999}"
MODEL_UID="${MODEL_UID:-ci_mock_model}"
MODEL_NAME="${MODEL_NAME:-ci/mock-model}"
SERVED_NAME="${SERVED_NAME:-ci-mock-model}"
GATEWAY="${GATEWAY:-http://127.0.0.1:8081}"
ROUTER="${ROUTER:-http://127.0.0.1:18081}"
BFF="${BFF:-http://127.0.0.1:18090}"
RUN_CANCEL_SSE="${RUN_CANCEL_SSE:-1}"
CURL_MAX="${CURL_MAX:-30}"

PIDS=()
pass=0
fail=0

ok()  { echo "[OK] $*"; pass=$((pass + 1)); }
bad() { echo "[FAIL] $*"; fail=$((fail + 1)); }

etcdctl_cmd() {
  if docker compose -f "$COMPOSE_FILE" ps etcd >/dev/null 2>&1; then
    docker compose -f "$COMPOSE_FILE" exec -T etcd etcdctl "$@"
  elif command -v etcdctl >/dev/null 2>&1; then
    etcdctl --endpoints="$ETCD_ENDPOINT" "$@"
  elif [ -x "$HOME/bin/etcdctl" ]; then
    "$HOME/bin/etcdctl" --endpoints="$ETCD_ENDPOINT" "$@"
  else
    echo "etcdctl unavailable" >&2
    return 1
  fi
}

wait_http() {
  local url="$1" label="$2" deadline=$((SECONDS + ${3:-60}))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if curl -sf --max-time 5 "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  bad "timeout waiting for $label ($url)"
  return 1
}

cleanup() {
  for pid in "${PIDS[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
  sleep 1
  for pid in "${PIDS[@]:-}"; do
    kill -9 "$pid" 2>/dev/null || true
  done
  docker compose -f "$COMPOSE_FILE" down >/dev/null 2>&1 || true
}
trap cleanup EXIT

now_ms() {
  python3 -c 'import time; print(int(time.time()*1000))'
}

need_bin() {
  local name="$1"
  if [ ! -x "$BIN/$name" ]; then
    echo "missing $BIN/$name — run: cargo build --release -p ${name#nebula-}" >&2
    exit 1
  fi
}

echo "========== CI smoke (mock engine) =========="

need_bin nebula-router
need_bin nebula-gateway
need_bin nebula-bff

wait_compose_postgres() {
  local i
  for i in $(seq 1 120); do
    if docker compose -f "$COMPOSE_FILE" exec -T postgres pg_isready -U postgres -d nebula >/dev/null 2>&1; then
      return 0
    fi
    local health
    health=$(docker compose -f "$COMPOSE_FILE" ps postgres --format '{{.Health}}' 2>/dev/null || true)
    if [ "$health" = "healthy" ]; then
      return 0
    fi
    sleep 2
  done
  echo "--- postgres logs ---" >&2
  docker compose -f "$COMPOSE_FILE" logs --tail 40 postgres >&2 || true
  return 1
}

echo "--- docker compose (etcd + postgres) ---"
docker compose -f "$COMPOSE_FILE" down -v >/dev/null 2>&1 || true
docker compose -f "$COMPOSE_FILE" up -d etcd postgres
for _ in $(seq 1 90); do
  if curl -sf --max-time 3 "$ETCD_ENDPOINT/health" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
curl -sf --max-time 3 "$ETCD_ENDPOINT/health" >/dev/null || { echo "etcd not healthy"; exit 1; }
wait_compose_postgres || { echo "postgres not healthy"; exit 1; }
ok "etcd + postgres ready"

echo "--- mock engine ---"
python3 "$ROOT/scripts/mock_openai_engine.py" "$MOCK_PORT" 127.0.0.1 &
PIDS+=("$!")
MOCK_URL="http://127.0.0.1:${MOCK_PORT}"
wait_http "$MOCK_URL/health" "mock engine" 30
ok "mock engine ready"

echo "--- seed etcd ---"
TS="$(now_ms)"
SPEC=$(cat <<EOF
{"model_uid":"$MODEL_UID","model_name":"$MODEL_NAME","model_source":"local","engine_type":"vllm","config":{"served_model_name":"$SERVED_NAME"},"created_at_ms":$TS,"updated_at_ms":$TS}
EOF
)
DEPLOY=$(cat <<EOF
{"model_uid":"$MODEL_UID","desired_state":"running","replicas":1,"version":1,"updated_at_ms":$TS}
EOF
)
PLAN=$(cat <<EOF
{"model_uid":"$MODEL_UID","model_name":"$MODEL_NAME","version":1,"updated_at_ms":$TS,"leader_epoch":1,"assignments":[{"replica_id":0,"node_id":"ci-node","engine_config_path":"/tmp/ci.yaml","port":$MOCK_PORT,"engine_type":"vllm"}]}
EOF
)
EP=$(cat <<EOF
{"model_uid":"$MODEL_UID","replica_id":0,"plan_version":1,"node_id":"ci-node","endpoint_kind":"native_http","api_flavor":"openai","status":"ready","last_heartbeat_ms":$TS,"base_url":"$MOCK_URL"}
EOF
)
etcdctl_cmd put "/models/$MODEL_UID/spec" "$SPEC" >/dev/null
etcdctl_cmd put "/deployments/$MODEL_UID" "$DEPLOY" >/dev/null
etcdctl_cmd put "/placements/$MODEL_UID" "$PLAN" >/dev/null
etcdctl_cmd put "/endpoints/$MODEL_UID/0" "$EP" >/dev/null
ok "seeded spec/deployment/placement/endpoint"

echo "--- start control plane ---"
env NEBULA_AUTH_DISABLED=1 "$BIN/nebula-router" \
  --listen-addr 127.0.0.1:18081 \
  --etcd-endpoint "$ETCD_ENDPOINT" \
  --model-uid "$MODEL_UID" >"$ROOT/logs/ci-router.log" 2>&1 &
PIDS+=("$!")

env NEBULA_AUTH_DISABLED=1 "$BIN/nebula-gateway" \
  --listen-addr 127.0.0.1:8081 \
  --router-url "$ROUTER" \
  --bff-url "$BFF" >"$ROOT/logs/ci-gateway.log" 2>&1 &
PIDS+=("$!")

env OBSERVE_AUTH_MODE=internal "$BIN/nebula-bff" \
  --listen-addr 127.0.0.1:18090 \
  --etcd-endpoint "$ETCD_ENDPOINT" \
  --router-url "$ROUTER" \
  --database-url postgresql://postgres:postgres@127.0.0.1:5432/nebula \
  >"$ROOT/logs/ci-bff.log" 2>&1 &
PIDS+=("$!")

sleep 5
wait_http "$ROUTER/healthz" "router" 30 || true
wait_http "$GATEWAY/healthz" "gateway" 30 || true
wait_http "$BFF/api/healthz" "bff" 60 || true

echo "--- gateway inference ---"
for model in "$MODEL_UID" "$SERVED_NAME"; do
  resp=$(curl -sf --max-time "$CURL_MAX" -X POST "$GATEWAY/v1/chat/completions" -H "Content-Type: application/json" \
    -d "{\"model\":\"$model\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"max_tokens\":8}" || true)
  echo "$resp" | grep -q mock && ok "chat model=$model" || bad "chat model=$model: $(echo "$resp" | head -c 120)"
done

comp=$(curl -sf --max-time "$CURL_MAX" -X POST "$GATEWAY/v1/completions" -H "Content-Type: application/json" \
  -d "{\"model\":\"$SERVED_NAME\",\"prompt\":\"hi\",\"max_tokens\":8}" || true)
echo "$comp" | grep -q mock && ok "completions" || bad "completions: $(echo "$comp" | head -c 120)"

echo "--- BFF console APIs ---"
TOKEN=$(curl -sf --max-time "$CURL_MAX" -X POST "$BFF/api/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' | python3 -c "import sys,json; print(json.load(sys.stdin)['token'])" || true)
if [ -n "$TOKEN" ]; then
  ok "BFF login"
  curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/overview" | grep -q "$MODEL_UID" \
    && ok "BFF overview" || bad "BFF overview"
  curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/alerts" | grep -q engine \
    && ok "BFF alerts schema" || bad "BFF alerts schema"
  detail=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/models/$MODEL_UID" || true)
  echo "$detail" | grep -q '"state":"running"' && ok "BFF model running" || bad "BFF model state"
else
  bad "BFF login"
fi

if [ "$RUN_CANCEL_SSE" = "1" ]; then
  echo "--- cancel SSE (router) ---"
  if MODEL="$MODEL_UID" ROUTER_URL="$ROUTER" "$ROOT/scripts/test_cancel_sse.sh"; then
    ok "cancel SSE contract"
  else
    bad "cancel SSE contract"
  fi
fi

echo ""
echo "========== Summary: $pass passed, $fail failed =========="
if [ "$fail" -ne 0 ]; then
  for f in ci-router ci-gateway ci-bff; do
    [ -f "$ROOT/logs/$f.log" ] && echo "--- $f.log ---" && tail -20 "$ROOT/logs/$f.log" || true
  done
fi
[ "$fail" -eq 0 ]
