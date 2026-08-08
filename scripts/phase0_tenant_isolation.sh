#!/usr/bin/env bash
# Phase 0: multi-tenant isolation gate (Gateway admission).
#
# Prerequisites:
#   - etcd up; Gateway with NEBULA_MULTI_TENANT=1 and
#     NEBULA_AUTH_TOKENS=tokA:operator:a,tokB:operator:b (auth NOT disabled)
#   - Router + at least one ready OpenAI-compatible model behind Gateway
#
# Usage:
#   MODEL=qwen35-4b-sglang \
#   GATEWAY_URL=http://127.0.0.1:8081 \
#   ETCDCTL_API=3 ETCDCTL_ENDPOINTS=http://127.0.0.1:2379 \
#   ./scripts/phase0_tenant_isolation.sh
set -euo pipefail

GATEWAY_URL="${GATEWAY_URL:-http://127.0.0.1:8081}"
METRICS_URL="${METRICS_URL:-${GATEWAY_URL}/metrics}"
MODEL="${MODEL:-}"
TOKEN_A="${TOKEN_A:-tokA}"
TOKEN_B="${TOKEN_B:-tokB}"
ETCDCTL_BIN="${ETCDCTL_BIN:-etcdctl}"
command -v "${ETCDCTL_BIN}" >/dev/null 2>&1 || ETCDCTL_BIN="${HOME}/bin/etcdctl"

if [[ -z "${MODEL}" ]]; then
  echo "MODEL is required (served model id / uid)" >&2
  exit 2
fi

now_ms() {
  python3 -c 'import time; print(int(time.time()*1000))'
}

put_tenant() {
  local id="$1"
  local rps="$2"
  local conc="$3"
  local ts
  ts="$(now_ms)"
  local json
  json=$(cat <<EOF
{"tenant_id":"${id}","display_name":"phase0-${id}","enabled":true,"quotas":{"rps_per_minute":${rps},"max_concurrency":${conc}},"api_token_principals":[],"created_at_ms":${ts},"updated_at_ms":${ts}}
EOF
)
  "${ETCDCTL_BIN}" put "/tenants/${id}" "${json}" >/dev/null
  echo "wrote /tenants/${id} rps=${rps} concurrency=${conc}"
}

metric() {
  local name="$1"
  local reason="${2:-}"
  curl -fsS "${METRICS_URL}" | python3 -c '
import sys
name, reason = sys.argv[1], sys.argv[2]
text = sys.stdin.read()
total = 0.0
found = False
for line in text.splitlines():
    if line.startswith("#") or not line.strip():
        continue
    if reason:
        needle = name + "{reason=\"" + reason + "\"}"
        if line.startswith(needle + " ") or line.startswith(needle + "\t"):
            try:
                total = float(line.rsplit(None, 1)[1])
                found = True
            except (IndexError, ValueError):
                pass
    elif line.startswith(name + " "):
        try:
            total = float(line.rsplit(None, 1)[1])
            found = True
        except (IndexError, ValueError):
            pass
print(total if found else 0)
' "$name" "$reason"
}

chat() {
  local token="$1"
  local max_tokens="${2:-8}"
  curl -sS -o /tmp/nebula_phase0_body.json -w "%{http_code}" --max-time 60 \
    -H "Authorization: Bearer ${token}" \
    -H "Content-Type: application/json" \
    -d "{\"model\":\"${MODEL}\",\"stream\":false,\"max_tokens\":${max_tokens},\"messages\":[{\"role\":\"user\",\"content\":\"ping\"}]}" \
    "${GATEWAY_URL}/v1/chat/completions"
}

deny_hdr() {
  local token="$1"
  curl -sS -D - -o /tmp/nebula_phase0_body.json --max-time 60 \
    -H "Authorization: Bearer ${token}" \
    -H "Content-Type: application/json" \
    -d "{\"model\":\"${MODEL}\",\"stream\":false,\"max_tokens\":8,\"messages\":[{\"role\":\"user\",\"content\":\"ping\"}]}" \
    "${GATEWAY_URL}/v1/chat/completions" | tr -d '\r' | awk -F': ' 'tolower($1)=="x-nebula-deny-code"{print $2; exit}'
}

echo "== seed tenants =="
# A: tight RPS; B: loose
put_tenant "a" 3 32
put_tenant "b" 600 64

# Warm: ensure tokens work once for B (and A within quota).
code_b0="$(chat "${TOKEN_B}")"
echo "B warmup HTTP ${code_b0}"
if [[ "${code_b0}" != "200" ]]; then
  echo "tenant B failed warmup; check NEBULA_MULTI_TENANT + AUTH tokens" >&2
  cat /tmp/nebula_phase0_body.json >&2 || true
  exit 1
fi

before_rps="$(metric nebula_gateway_tenant_denied_total rps)"
echo "before denied_rps=${before_rps}"

echo "== burst A beyond rps=3 =="
declare -a codes_a=()
for _ in $(seq 1 10); do
  codes_a+=("$(chat "${TOKEN_A}")")
done
echo "A codes: ${codes_a[*]}"

denied_a=0
ok_a=0
for c in "${codes_a[@]}"; do
  if [[ "$c" == "429" ]]; then
    denied_a=$((denied_a + 1))
  elif [[ "$c" == "200" ]]; then
    ok_a=$((ok_a + 1))
  fi
done

if [[ "${denied_a}" -lt 1 ]]; then
  echo "FAIL: expected tenant A to get at least one 429 (rps), got denied=${denied_a} ok=${ok_a}" >&2
  exit 1
fi
echo "A ok=${ok_a} denied_429=${denied_a}"

deny_code="$(deny_hdr "${TOKEN_A}" || true)"
# After burst, next A call should still be denied within the minute window.
echo "A deny header sample: ${deny_code:-"(none)"}"

echo "== concurrent B must still succeed =="
code_b="$(chat "${TOKEN_B}")"
echo "B during A throttle HTTP ${code_b}"
if [[ "${code_b}" != "200" ]]; then
  echo "FAIL: tenant B should not be affected by A quota" >&2
  cat /tmp/nebula_phase0_body.json >&2 || true
  exit 1
fi

after_rps="$(metric nebula_gateway_tenant_denied_total rps)"
echo "after denied_rps=${after_rps}"
python3 - <<PY
before=float("${before_rps}")
after=float("${after_rps}")
assert after >= before + 1, f"tenant denied rps metric did not increase: {before} -> {after}"
print("OK: metrics tenant_denied reason=rps increased")
PY

echo "== optional model ACL: A only allowed fake-model =="
ts="$(now_ms)"
acl_json=$(cat <<EOF
{"tenant_id":"a","display_name":"phase0-a-acl","enabled":true,"quotas":{"rps_per_minute":600,"max_concurrency":32,"allowed_models":["__phase0_deny_all__"]},"api_token_principals":[],"created_at_ms":${ts},"updated_at_ms":${ts}}
EOF
)
"${ETCDCTL_BIN}" put "/tenants/a" "${acl_json}" >/dev/null
# Gateway loads tenant per request — no restart needed.
acl_code="$(chat "${TOKEN_A}")"
echo "A with ACL HTTP ${acl_code}"
if [[ "${acl_code}" != "403" ]]; then
  echo "FAIL: expected 403 model deny for A, got ${acl_code}" >&2
  cat /tmp/nebula_phase0_body.json >&2 || true
  exit 1
fi
code_b2="$(chat "${TOKEN_B}")"
echo "B with A ACL HTTP ${code_b2}"
if [[ "${code_b2}" != "200" ]]; then
  echo "FAIL: B should still succeed under A ACL" >&2
  exit 1
fi

# Restore A to loose for later SLO traffic if needed
put_tenant "a" 600 32

echo "OK: phase0 tenant isolation passed (A limited, B isolated, ACL ok)"
