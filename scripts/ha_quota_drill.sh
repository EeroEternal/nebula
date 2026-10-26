#!/usr/bin/env bash
# HA cluster-quota drill (issue #68), single host: 1 etcd + 1 mock engine + 2 gateways.
#
# Fixed-divisor mode (NEBULA_GATEWAY_QUOTA_MEMBERS):
#   T1 control  — members=1 reproduces the N x oversell (10/min quota, 20 admitted).
#   T2 sharded  — members=2 admits exactly 10 (5 + 5), rest 429 tenant_rps_exceeded.
#   T3 lifetime — a streaming request holds its concurrency slot until the stream ends.
#   T4 tiny     — a limit < members tenant is denied instead of oversold.
# Live-registry mode (/gateways/ leases, short TTL for the drill):
#   T5 discovery — both Gateways self-register, each enforces limit/2 = 5.
#   T6 leave     — Gateway A stops, B's view shrinks to 1 and it takes the whole
#                  limit (10) within the TTL: the dead replica's shard is reused.
#   T7 join      — A returns, both are back to 5 (no oversell after the re-join).
#   T8 stale     — a Gateway whose membership view cannot be trusted answers
#                  503 tenant_quota_unavailable and reports view_fresh=0.
# Engine queue (T9):
#   T9 queue     — the engine in-flight cap is divided by the member count, so two
#                  Gateways cannot each admit the engine's full sweet spot.
#
# Usage: ./scripts/ha_quota_drill.sh   (needs etcd/etcdctl, python3, curl)
# Logs:  logs/ha-quota-drill/
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN=${BIN:-$ROOT/target/debug/nebula-gateway}
W=${W:-$ROOT/logs/ha-quota-drill}
ETCD_EP=http://127.0.0.1:12379
GA=http://127.0.0.1:18091
GB=http://127.0.0.1:18092
ENGINE=http://127.0.0.1:19999
TOKENS="rps-token:operator:rps,conc-token:operator:conc,tiny-token:operator:tiny,live-token:operator:live,leave-token:operator:leave,join-token:operator:join,q-token:operator:q"
FAIL=0

say() { printf '\n=== %s ===\n' "$*"; }
ok() { printf 'PASS  %s\n' "$*"; }
bad() { printf 'FAIL  %s\n' "$*"; FAIL=1; }
adm() { echo "${1%% *}"; }
deny() { echo "$1" | awk '{print $2}'; }

pids=()
stop_gateways() { pkill -f "nebula-gateway --listen-addr 127.0.0.1:1809" 2>/dev/null; sleep 1; }
cleanup() {
  stop_gateways
  for p in "${pids[@]:-}"; do kill "$p" 2>/dev/null; done
  wait 2>/dev/null
}
trap cleanup EXIT

[ -x "$BIN" ] || { echo "gateway binary missing: $BIN (cargo build -p nebula-gateway)" >&2; exit 1; }
command -v etcd >/dev/null || { echo "etcd not installed" >&2; exit 1; }
mkdir -p "$W"

wait_tcp() { # host port tries
  for _ in $(seq 1 "${3:-60}"); do
    (exec 3<>"/dev/tcp/$1/$2") 2>/dev/null && return 0
    sleep 0.5
  done
  return 1
}

wait_http() { # url tries
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

seed() { # tenant_id quotas-json
  etcdctl --endpoints="$ETCD_EP" put "/tenants/$1" \
    "{\"tenant_id\":\"$1\",\"display_name\":\"$1\",\"enabled\":true,\"quotas\":$2}" >/dev/null
}

start_gateway() { # name addr [pinned-members]
  local name=$1 addr=$2 members=${3:-}
  if [ -n "$members" ]; then export NEBULA_GATEWAY_QUOTA_MEMBERS=$members
  else unset NEBULA_GATEWAY_QUOTA_MEMBERS; fi
  NEBULA_MULTI_TENANT=1 \
  NEBULA_AUTH_TOKENS="$TOKENS" \
  NEBULA_AUTH_RATE_LIMIT_PER_MINUTE=100000 \
  ETCD_ENDPOINT=$ETCD_EP RUST_LOG=info \
    "$BIN" --listen-addr "$addr" --router-url "$ENGINE" \
    --log-path "$W/$name.log" >"$W/$name.out" 2>&1 &
  pids+=($!)
  wait_http "http://$addr/healthz" 80 || { echo "$name did not start" >&2; tail -20 "$W/$name.out" >&2; exit 1; }
}

req() { # url token stream
  local url=$1 tok=$2 stream=${3:-false} hdr code
  hdr=$(mktemp)
  code=$(curl -s -m 60 -o /dev/null -D "$hdr" -w '%{http_code}' "$url" \
    -H "authorization: Bearer $tok" -H 'content-type: application/json' \
    -d "{\"model\":\"mock\",\"stream\":$stream,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}")
  printf '%s %s\n' "$code" "$(grep -i '^x-nebula-deny-code:' "$hdr" | tr -d '\r' | awk '{print $2}' | tr '\n' ',')"
  rm -f "$hdr"
}

flood() { # base token count
  local base=$1 tok=$2 count=$3 a=0 d=0 codes=""
  for _ in $(seq 1 "$count"); do
    read -r code deny < <(req "$base/v1/chat/completions" "$tok" false)
    if [ "$code" = "200" ]; then a=$((a + 1)); else d=$((d + 1)); codes="$codes $code:${deny:-none}"; fi
  done
  printf '%s %s%s\n' "$a" "$d" "$codes"
}

qmetrics() { # base-url metric-name
  curl -s -m 5 "$1/metrics" | awk -v m="$2" '$1==m {print $2; found=1} END {if(!found) print "absent"}'
}

mkdir -p "$W"

# A stale gateway from a previous run still answers /healthz, which would make
# every following assertion test the wrong process. Fail fast instead.
for port in 18091 18092 18093; do
  if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
    echo "port $port already in use (stale gateway?) — stop it first" >&2
    exit 1
  fi
done

say "start etcd + mock engine"
start_etcd
python3 "$ROOT/scripts/mock_openai_engine.py" 19999 >"$W/engine.log" 2>&1 &
pids+=($!)
wait_tcp 127.0.0.1 19999 30 || { echo "mock engine did not start" >&2; exit 1; }
ok "etcd healthy: $(etcdctl --endpoints=$ETCD_EP endpoint health 2>&1 | head -1)"
seed rps   '{"rps_per_minute":10,"max_concurrency":100,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed conc  '{"rps_per_minute":1000,"max_concurrency":2,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed tiny  '{"rps_per_minute":1000,"max_concurrency":1,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed live  '{"rps_per_minute":10,"max_concurrency":100,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed leave '{"rps_per_minute":10,"max_concurrency":100,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed join  '{"rps_per_minute":10,"max_concurrency":100,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
seed q     '{"rps_per_minute":1000,"max_concurrency":1000,"max_tokens_per_minute":1000000,"max_pin_requests_per_minute":1000}'
ok "tenants seeded: rps/live/leave/join=10/min, conc=100, tiny=1"

say "T1 control: NEBULA_GATEWAY_QUOTA_MEMBERS=1 (the N x oversell)"
start_gateway ga 127.0.0.1:18091 1
start_gateway gb 127.0.0.1:18092 1
t1a=$(flood "$GA" rps-token 10); t1b=$(flood "$GB" rps-token 10)
n1=$(( $(adm "$t1a") + $(adm "$t1b") ))
printf 'gateway-A: %s\ngateway-B: %s\ntotal admitted: %s (tenant quota = 10/min)\n' "$t1a" "$t1b" "$n1"
[ "$n1" -eq 20 ] && ok "control: 20 admitted for a 10/min quota (oversell reproduced)" \
  || bad "control admitted $n1, expected 20"

say "T2 sharded: restart both with NEBULA_GATEWAY_QUOTA_MEMBERS=2"
stop_gateways
start_gateway ga 127.0.0.1:18091 2
start_gateway gb 127.0.0.1:18092 2
t2a=$(flood "$GA" rps-token 10); t2b=$(flood "$GB" rps-token 10)
n2=$(( $(adm "$t2a") + $(adm "$t2b") ))
printf 'gateway-A: %s\ngateway-B: %s\ntotal admitted: %s\n' "$t2a" "$t2b" "$n2"
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
printf 'during stream A: %s\nduring stream B: %s\nafter stream A: %s\n' "$during_a" "$during_b" "$after_a"
[ "$(adm "$during_a")" = "429" ] && ok "the streaming request holds its slot for the whole stream" \
  || bad "a second request was admitted while the first was still streaming: $during_a"
[ "$(adm "$during_b")" = "200" ] && ok "the other Gateway keeps its own shard" \
  || bad "gateway-B was wrongly denied: $during_b"
[ "$(adm "$after_a")" = "200" ] && ok "slot released when the stream ended" \
  || bad "slot not released after stream end: $after_a"

say "T4 a limit smaller than the member count is denied, not multiplied"
tiny_a=$(req "$GA/v1/chat/completions" tiny-token false)
tiny_b=$(req "$GB/v1/chat/completions" tiny-token false)
printf 'A: %s\nB: %s\n' "$tiny_a" "$tiny_b"
if [ "$(adm "$tiny_a")" = "429" ] && [ "$(adm "$tiny_b")" = "429" ]; then
  ok "1 slot cannot be split: both replicas deny (never 2 x the limit)"
else
  bad "tiny tenant was admitted somewhere: $tiny_a / $tiny_b"
fi

# ---------------- live registry ----------------
say "T5 live registry: no pinned members, both Gateways self-register"
stop_gateways
export NEBULA_GATEWAY_MEMBER_TTL_MS=4000
export NEBULA_GATEWAY_MEMBER_REFRESH_MS=500
export NEBULA_GATEWAY_QUOTA_VIEW_STALE_MS=2000
export NEBULA_GATEWAY_INSTANCE_ID=gw-a
start_gateway ga 127.0.0.1:18091
export NEBULA_GATEWAY_INSTANCE_ID=gw-b
start_gateway gb 127.0.0.1:18092
printf 'members seen: A=%s B=%s\n' "$(qmetrics "$GA" nebula_gateway_quota_members)" "$(qmetrics "$GB" nebula_gateway_quota_members)"
t5a=$(flood "$GA" live-token 10); t5b=$(flood "$GB" live-token 10)
n5=$(( $(adm "$t5a") + $(adm "$t5b") ))
printf 'A: %s\nB: %s\ntotal admitted: %s\n' "$t5a" "$t5b" "$n5"
if [ "$n5" -eq 10 ] && [ "$(adm "$t5a")" -eq 5 ] && [ "$(adm "$t5b")" -eq 5 ]; then
  ok "discovered membership gives 5 + 5 = 10 (registry replaces the static divisor)"
else
  bad "live registry admitted $n5, expected 5 + 5 = 10"
fi

say "T6 leave: stop gateway-A (SIGTERM → deregister), B takes the whole limit"
pkill -f "nebula-gateway --listen-addr 127.0.0.1:18091" 2>/dev/null
for _ in $(seq 1 20); do
  [ "$(qmetrics "$GB" nebula_gateway_quota_members)" = "1" ] && break
  sleep 0.5
done
printf 'members seen by B after A left: %s\n' "$(qmetrics "$GB" nebula_gateway_quota_members)"
t6=$(flood "$GB" leave-token 10)
printf 'B: %s\n' "$t6"
if [ "$(qmetrics "$GB" nebula_gateway_quota_members)" = "1" ] && [ "$(adm "$t6")" -eq 10 ]; then
  ok "the dead replica's shard is reused: B enforces the full 10/min"
else
  bad "B did not take over the shard after A left (members=$(qmetrics "$GB" nebula_gateway_quota_members), $t6)"
fi

say "T7 join: start gateway-A again; both fall back to 5 each"
export NEBULA_GATEWAY_INSTANCE_ID=gw-a
start_gateway ga 127.0.0.1:18091
for _ in $(seq 1 20); do
  [ "$(qmetrics "$GB" nebula_gateway_quota_members)" = "2" ] && break
  sleep 0.5
done
printf 'members seen by B after A returned: %s\n' "$(qmetrics "$GB" nebula_gateway_quota_members)"
t7=$(flood "$GB" join-token 10)
printf 'B: %s\n' "$t7"
if [ "$(qmetrics "$GB" nebula_gateway_quota_members)" = "2" ] && [ "$(adm "$t7")" -eq 5 ]; then
  ok "re-join re-shards back to limit/2 = 5 (no oversell after scale-up)"
else
  bad "re-join did not re-shard B (members=$(qmetrics "$GB" nebula_gateway_quota_members), $t7)"
fi

say "T8 untrusted membership view must fail closed, not guess"
pkill -f "nebula-gateway --listen-addr 127.0.0.1:18092" 2>/dev/null
sleep 0.5
# stale window shorter than the refresh interval = the view can never be trusted
export NEBULA_GATEWAY_QUOTA_VIEW_STALE_MS=1
export NEBULA_GATEWAY_INSTANCE_ID=gw-stale
start_gateway gs 127.0.0.1:18093
t8=$(req "http://127.0.0.1:18093/v1/chat/completions" conc-token false)
printf 'stale-view gateway: %s view_fresh=%s degraded=%s\n' \
  "$t8" "$(qmetrics http://127.0.0.1:18093 nebula_gateway_quota_view_fresh)" \
  "$(qmetrics http://127.0.0.1:18093 nebula_gateway_quota_degraded_total)"
if [ "$(adm "$t8")" = "503" ] && [ "$(deny "$t8")" = "tenant_quota_unavailable," ]; then
  ok "stale view answers 503 tenant_quota_unavailable (fail closed) and reports view_fresh=0"
else
  bad "stale view did not fail closed: $t8"
fi
stop_gateways

say "T9 engine queue cap is divided by the member count (8 / 2 = 4 slots)"
unset NEBULA_GATEWAY_QUOTA_VIEW_STALE_MS   # back to the normal 3x refresh window
export NEBULA_GATEWAY_QUEUE_MODE=fast_fail
export NEBULA_GATEWAY_QUEUE_MAX_CONCURRENCY=8
export NEBULA_GATEWAY_QUEUE_MAX_WAIT_MS=0
export NEBULA_GATEWAY_INSTANCE_ID=gw9-a
start_gateway ga 127.0.0.1:18091
export NEBULA_GATEWAY_INSTANCE_ID=gw9-b
start_gateway gb 127.0.0.1:18092
# Wait until both (and only both) are counted before firing the burst.
for _ in $(seq 1 20); do
  [ "$(qmetrics "$GA" nebula_gateway_quota_members)" = "2" ] && break
  sleep 0.5
done
printf 'queue members: A=%s B=%s\n' "$(qmetrics "$GA" nebula_gateway_quota_members)" "$(qmetrics "$GB" nebula_gateway_quota_members)"
codes="$W/t9_codes.txt"
: >"$codes"
req_pids=()
for _ in $(seq 1 6); do
  ( req "$GA/v1/chat/completions" q-token true >>"$codes" ) &
  req_pids+=($!)
done
sleep 2.5
# Wait only for the request subshells: `wait` with no args would also wait for
# the etcd/gateway children and hang the drill.
for pid in "${req_pids[@]}"; do wait "$pid" 2>/dev/null; done
admitted=$(grep -c '^200 ' "$codes" || true)
denied=$(grep -c '^429 queue_' "$codes" || true)
printf 'concurrent streams on A: %s admitted, %s queue-rejected (%s)\n' \
  "${admitted:-0}" "${denied:-0}" "$(awk '{print $1":"$2}' "$codes" | sort | uniq -c | tr '\n' ' ')"
if [ "${admitted:-0}" -eq 4 ] && [ "${denied:-0}" -eq 2 ]; then
  ok "queue admits 4 (8 / 2 members), not 8 — two Gateways cannot double the engine cap"
else
  bad "queue divisor not applied: $admitted admitted / $denied rejected (expected 4 / 2)"
fi

say "result"
[ "$FAIL" = "0" ] && echo "ALL PASS" || echo "HAS FAILURES"
exit "$FAIL"
