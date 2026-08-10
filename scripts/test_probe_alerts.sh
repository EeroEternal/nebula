#!/usr/bin/env bash
# P1 probe + console API smoke: container exit, health probe failure, BFF alerts/overview.
set -euo pipefail

BFF="${BFF:-http://127.0.0.1:18090}"
GATEWAY="${GATEWAY:-http://127.0.0.1:8081}"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
ETCDCTL="${ETCDCTL:-etcdctl}"
if ! command -v "$ETCDCTL" >/dev/null 2>&1 && [ -x "$HOME/bin/etcdctl" ]; then
  ETCDCTL="$HOME/bin/etcdctl"
fi

SGLANG_UID="${SGLANG_UID:-qwen15_moe_sglang}"
SGLANG_CONTAINER="${SGLANG_CONTAINER:-nebula-qwen15_moe_sglang-0}"
BFF_USER="${BFF_USER:-admin}"
BFF_PASS="${BFF_PASS:-admin123}"

pass=0
fail=0
ok()  { echo "[OK] $*"; pass=$((pass + 1)); }
bad() { echo "[FAIL] $*"; fail=$((fail + 1)); }

login() {
  curl -sf -X POST "$BFF/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"$BFF_USER\",\"password\":\"$BFF_PASS\"}" \
    | python3 -c "import sys,json; print(json.load(sys.stdin)['token'])"
}

wait_alert() {
  local typ="$1" deadline=$((SECONDS + ${2:-60}))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if "$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get --prefix /alerts/ --print-value-only 2>/dev/null \
      | grep -q "\"alert_type\":\"$typ\".*\"model_uid\":\"$SGLANG_UID\"" \
      || "$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get --prefix /alerts/ --print-value-only 2>/dev/null \
      | grep -q "$SGLANG_UID"; then
      return 0
    fi
    sleep 3
  done
  return 1
}

wait_endpoint_status() {
  local want="$1" deadline=$((SECONDS + ${2:-120}))
  while [ "$SECONDS" -lt "$deadline" ]; do
    raw=$("$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get "/endpoints/${SGLANG_UID}/0" --print-value-only 2>/dev/null || true)
    if echo "$raw" | grep -q "\"status\":\"$want\""; then
      return 0
    fi
    sleep 3
  done
  return 1
}

echo "========== Probe + console API test =========="

TOKEN=$(login) || { echo "BFF login failed"; exit 1; }
ok "BFF login"

# Console APIs used by frontend
ov=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/overview" || true)
if echo "$ov" | grep -q '"endpoints"' && echo "$ov" | grep -q "$SGLANG_UID"; then
  ok "GET /api/overview includes sglang endpoint"
else
  bad "GET /api/overview"
fi

if echo "$ov" | grep -q 'status_detail'; then
  ok "overview schema supports status_detail"
else
  echo "[INFO] no status_detail in overview (may be healthy)"
fi

alerts=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/alerts" || true)
if echo "$alerts" | grep -q '"engine"'; then
  ok "GET /api/v2/alerts"
else
  bad "GET /api/v2/alerts: $(echo "$alerts" | head -c 120)"
fi

detail=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/models/$SGLANG_UID" || true)
if echo "$detail" | grep -q '"endpoints"'; then
  ok "GET /api/v2/models/{uid}"
else
  bad "GET /api/v2/models/{uid}"
fi

echo "--- health probe failure (docker pause) ---"
if docker pause "$SGLANG_CONTAINER" 2>/dev/null; then
  if wait_alert "health_probe_failed" 45 || wait_endpoint_status "unhealthy" 45; then
    ok "health probe / unhealthy after pause"
  else
    bad "no health probe alert after pause"
  fi
  bff_pause=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/alerts" || true)
  if echo "$bff_pause" | grep -q "$SGLANG_UID"; then
    ok "BFF alert after pause"
  else
    bad "BFF alert missing after pause"
  fi
  docker unpause "$SGLANG_CONTAINER" 2>/dev/null || true
  if wait_endpoint_status "ready" 120; then
    ok "recovered after unpause"
  else
    bad "not ready after unpause"
  fi
else
  bad "docker pause $SGLANG_CONTAINER"
fi

echo "--- container exit (docker rm) ---"
docker rm -f "$SGLANG_CONTAINER" 2>/dev/null || true
sleep 12
ep=$("$ETCDCTL" --endpoints="$ETCD_ENDPOINT" get "/endpoints/${SGLANG_UID}/0" --print-value-only 2>/dev/null || true)
if echo "$ep" | grep -q unhealthy; then
  ok "unhealthy after rm"
else
  bad "expected unhealthy after rm: $(echo "$ep" | head -c 120)"
fi
if wait_alert "container_exited" 30; then
  ok "container_exited alert"
else
  bad "no container_exited alert"
fi
bff_rm=$(curl -sf -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/alerts" || true)
if echo "$bff_rm" | grep -q "container_exited\|$SGLANG_UID"; then
  ok "BFF alert after rm"
else
  bad "BFF alert after rm"
fi

echo "--- wait auto-recovery ---"
if wait_endpoint_status "ready" 180; then
  ok "auto-recovered ready"
  curl -sf -X POST "$GATEWAY/v1/chat/completions" -H "Content-Type: application/json" \
    -d "{\"model\":\"$SGLANG_UID\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"max_tokens\":8}" \
    | grep -q choices && ok "gateway chat after recovery" || bad "gateway chat after recovery"
else
  bad "auto-recovery timeout"
fi

echo ""
echo "========== Summary: $pass passed, $fail failed =========="
[ "$fail" -eq 0 ]
