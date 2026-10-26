#!/usr/bin/env bash
# HA cluster-quota drill (issue #68), single host: 1 etcd + 1 mock engine + 2 gateways.
#
# Proves, against real processes and a real etcd:
#   T1 control  — NEBULA_GATEWAY_QUOTA_MEMBERS=1 reproduces the N x oversell
#                 (10/min tenant quota, 20 admitted across two Gateways).
#   T2 sharded  — members=2 admits exactly 10 (5 + 5) and denies the rest with
#                 `tenant_rps_exceeded`.
#   T3 lifetime — a streaming request holds its concurrency slot until the stream
#                 ends (second request on the same Gateway during the stream is
#                 denied, after it is admitted; the other Gateway keeps its shard).
#   T4 tiny     — a limit < members tenant is denied instead of oversold.
#
# Usage: ./scripts/ha_quota_drill.sh        (needs `etcd`, `etcdctl`, `python3`, curl)
# Logs:  logs/ha-quota-drill/
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN=${BIN:-$ROOT/target/debug/nebula-gateway}
W=${W:-$ROOT/logs/ha-quota-drill}
ETCD_EP=http://127.0.0.1:12379
GA=http://127.0.0.1:18091
GB=http://127.0.0.1:18092
ENGINE=http://127.0.0.1:19999
TOKENS="rps-token:operator:rps,conc-token:operator:conc,tiny-token:operator:tiny"
FAIL=0

say() { printf '\n=== %s ===\n' "$*"; }
ok() { printf 'PASS  %s\n' "$*"; }
bad() { printf 'FAIL  %s\n' "$*"; FAIL=1; }

pids=()
cleanup() {
  for p in "${pids[@]:-}"; do kill "$p" 2>/dev/null; done
  pkill -f "nebula-gateway --listen-addr 127.0.0.1:1809" 2>/dev/null
  wait 2>/dev/null
}
trap cleanup EXIT

[ -x "$BIN" ] || { echo "gateway binary missing: $BIN (cargo build -p nebula-gateway)" >&2; exit 1; }
command -v etcd >/dev/null || { echo "etcd not installed" >&2; exit 1; }
mkdir -p "$W"

wait_tcp() { # host, port, tries
  for _ in $(seq 1 "${3:-60}"); do
    (exec 3<>"/dev/tcp/$1/$2") 2>/dev/null && return 0
    sleep 0.5
  done
  return 1
}

wait_http() { # url, tries
  for _ in $(seq 1 "${2:-60}"); do
    curl -s -m 1 -o /dev/null "$1" && return 0
    sleep 0.5
  done
  return 1
}

start_etcd() {
  rm -rf "$W/etcd-data"
  etcd --name drill --data-dir "$W/etcd-data" \
    --listen-client-urls "$ETCD_EP" --advertise-client-urls "$ETCD_EP" \
    --listen-peer-urls http://127.0.0.1:12380 \
    --initial-advertise-peer-urls http://127.0.0.1:12380 \
    --initial-cluster drill=http://127.0.0.1:12380 \
    --log-level error >"$W/etcd.log" 2>&1 &
  pids+=($!)
  wait_http "$ETCD_EP/health" 60 || { echo "etcd did not start" >&2; exit 1; }
}

seed() { # tenant_id, quotas-json
  etcdctl --endpoints="$ETCD_EP" put "/tenants/$1" \
    "{\"tenant_id\":\"$1\",\"display_name\":\"$1\",\"enabled\":true,\"quotas\":$2}" >/dev/null
}

start_gateway() { # name, addr, members
  NEBULA_GATEWAY_QUOTA_MEMBERS=$3 \
  NEBULA_MULTI_TENANT=1 \
  NEBULA_AUTH_TOKENS="$TOKENS" \
  NEBULA_AUTH_RATE_LIMIT_PER_MINUTE=100000 \
  ETCD_ENDPOINT=$ETCD_EP \
  RUST_LOG=info \
    "$BIN" --listen-addr "$2" --router-url "$ENGINE" \
    --log-path "$W/$1.log" >"$W/$1.out" 2>&1 &
  pids+=($!)
  wait_http "http://$2/healthz" 80 || { echo "$1 did not start" >&2; tail -20 "$W/$1.out" >&2; exit 1; }
}

req() { # url, token, stream -> "code deny_code"
  local url=$1 tok=$2 stream=${3:-false}
  local hdr code
  hdr=$(mktemp)
  code=$(curl -s -m 60 -o /dev/null -D "$hdr" -w '%{http_code}' "$url" \
    -H "authorization: Bearer $tok" -H 'content-type: application/json' \
    -d "{\"model\":\"mock\",\"stream\":$stream,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}")
  printf '%s %s\n' "$code" "$(grep -i '^x-nebula-deny-code:' "$hdr" | tr -d '\r' | awk '{print $2}' | tr '\n' ',')"
  rm -f "$hdr"
}

flood() { # base-url, token, count -> "admitted denied codes"
  local base=$1 tok=$2 count=$3 a=0 d=0 codes=""
  for _ in $(seq 1 "$count"); do
    read -r code deny < <(req "$base/v1/chat/completions" "$tok" false)
    if [ "$code" = "200" ]; then a=$((a + 1)); else d=$((d + 1)); codes="$codes $code:${deny:-none}"; fi
  done
  printf '%s %s%s\n' "$a" "$d" "$codes"
}

adm() { echo "${1%% *}"; }

say "start etcd + mock engine"
start_etcd
python3 "$ROOT/scripts/mock_openai_engine.py" 19999 >"$W/engine.log" 2>&1 &
pids+=($!)
wait_tcp 127.0.0.1 19999 30 || { echo "mock engine did not start" >&2; exit 1; }
ok "etcd healthy: $(etcdctl --endpoints=$ETCD_EP endpoint health 2>&1 | head -1)"
seed rps  '{"rps_per_minute":10,"max_concurrency":100,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed conc '{"rps_per_minute":1000,"max_concurrency":2,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed tiny '{"rps_per_minute":1000,"max_concurrency":1,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
ok "tenants seeded: rps=10/min, conc=2 (-> 1 per Gateway), tiny=1 (-> 0 per Gateway)"

say "T1 control: NEBULA_GATEWAY_QUOTA_MEMBERS=1 (the N x oversell)"
start_gateway ga 127.0.0.1:18091 1
start_gateway gb 127.0.0.1:18092 1
t1a=$(flood "$GA" rps-token 10); t1b=$(flood "$GB" rps-token 10)
n1=$(( $(adm "$t1a") + $(adm "$t1b") ))
printf 'gateway-A: %s\ngateway-B: %s\ntotal admitted: %s (tenant quota = 10/min)\n' "$t1a" "$t1b" "$n1"
[ "$n1" -eq 20 ] && ok "control: 20 admitted for a 10/min quota (oversell reproduced)" \
  || bad "control admitted $n1, expected 20"

say "T2 sharded: restart both Gateways with NEBULA_GATEWAY_QUOTA_MEMBERS=2"
cleanup_gw() { pkill -f "nebula-gateway --listen-addr 127.0.0.1:1809" 2>/dev/null; sleep 1; }
cleanup_gw
start_gateway ga2 127.0.0.1:18091 2
start_gateway gb2 127.0.0.1:18092 2
t2a=$(flood "$GA" rps-token 10); t2b=$(flood "$GB" rps-token 10)
n2=$(( $(adm "$t2a") + $(adm "$t2b") ))
printf 'gateway-A: %s\ngateway-B: %s\ntotal admitted: %s (tenant quota = 10/min)\n' "$t2a" "$t2b" "$n2"
if [ "$n2" -eq 10 ] && [ "$(adm "$t2a")" -eq 5 ] && [ "$(adm "$t2b")" -eq 5 ]; then
  ok "cluster quota held: 5 + 5 = 10 admitted, 10 x 429 tenant_rps_exceeded"
else
  bad "sharded run admitted $n2, expected exactly 5 + 5 = 10"
fi

say "T3 concurrency slot lifetime (max_concurrency=2 -> 1 slot per Gateway)"
req "$GA/v1/chat/completions" conc-token true >"$W/t3_stream.txt" &
stream_pid=$!
sleep 1.5
during_a=$(req "$GA/v1/chat/completions" conc-token false)
during_b=$(req "$GB/v1/chat/completions" conc-token false)
wait "$stream_pid"
after_a=$(req "$GA/v1/chat/completions" conc-token false)
printf 'streamed request: %s\n' "$(cat "$W/t3_stream.txt")"
printf 'during stream gateway-A: %s\nduring stream gateway-B: %s\nafter  stream gateway-A: %s\n' \
  "$during_a" "$during_b" "$after_a"
[ "$(adm "$during_a")" = "429" ] && ok "the streaming request holds its slot for the whole stream" \
  || bad "a second request was admitted while the first was still streaming: $during_a"
[ "$(adm "$during_b")" = "200" ] && ok "the other Gateway keeps its own shard" \
  || bad "gateway-B was wrongly denied: $during_b"
[ "$(adm "$after_a")" = "200" ] && ok "slot released when the stream ended" \
  || bad "slot not released after stream end: $after_a"

say "T4 a limit smaller than the member count is denied, not multiplied"
tiny_a=$(req "$GA/v1/chat/completions" tiny-token false)
tiny_b=$(req "$GB/v1/chat/completions" tiny-token false)
printf 'gateway-A: %s\ngateway-B: %s\n' "$tiny_a" "$tiny_b"
if [ "$(adm "$tiny_a")" = "429" ] && [ "$(adm "$tiny_b")" = "429" ]; then
  ok "1 slot cannot be split: both replicas deny (never 2 x the limit)"
else
  bad "tiny tenant was admitted somewhere: $tiny_a / $tiny_b"
fi

say "result"
[ "$FAIL" = "0" ] && echo "ALL PASS" || echo "HAS FAILURES"
exit "$FAIL"
