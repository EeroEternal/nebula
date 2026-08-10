#!/usr/bin/env bash
# Frontend-facing BFF API smoke (same paths the console uses).
set -euo pipefail

BFF="${BFF:-http://127.0.0.1:18090}"
BFF_USER="${BFF_USER:-admin}"
BFF_PASS="${BFF_PASS:-admin123}"
MODEL_UID="${MODEL_UID:-qwen15_moe_sglang}"

curl_noproxy() {
  curl --noproxy '*' "$@"
}

pass=0
fail=0
ok()  { echo "[OK] $*"; pass=$((pass + 1)); }
bad() { echo "[FAIL] $*"; fail=$((fail + 1)); }

echo "========== Frontend BFF API smoke =========="
echo "bff=$BFF model=$MODEL_UID"

TOKEN=$(curl_noproxy -sf --max-time 10 -X POST "$BFF/api/auth/login" \
  -H "Content-Type: application/json" \
  -d "{\"username\":\"$BFF_USER\",\"password\":\"$BFF_PASS\"}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['token'])") || { echo "login failed"; exit 1; }
ok "POST /api/auth/login"

ov=$(curl_noproxy -sf --max-time 10 -H "Authorization: Bearer $TOKEN" "$BFF/api/overview" || true)
echo "$ov" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'endpoints' in d" 2>/dev/null \
  && ok "GET /api/overview schema" || bad "GET /api/overview"

alerts=$(curl_noproxy -sf --max-time 10 -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/alerts" || true)
echo "$alerts" | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'engine' in d and 'disk' in d" 2>/dev/null \
  && ok "GET /api/v2/alerts schema" || bad "GET /api/v2/alerts"

models=$(curl_noproxy -sf --max-time 10 -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/models" || true)
echo "$models" | grep -q model_uid && ok "GET /api/v2/models" || bad "GET /api/v2/models"

detail=$(curl_noproxy -sf --max-time 10 -H "Authorization: Bearer $TOKEN" "$BFF/api/v2/models/$MODEL_UID" || true)
echo "$detail" | python3 -c "
import sys,json
d=json.load(sys.stdin)
assert d.get('model_uid') == '$MODEL_UID'
assert 'state' in d and 'endpoints' in d and 'replicas' in d
for ep in d.get('endpoints', []):
    assert 'status' in ep
" 2>/dev/null && ok "GET /api/v2/models/{uid} detail" || bad "GET /api/v2/models/{uid}"

echo ""
echo "========== Summary: $pass passed, $fail failed =========="
[ "$fail" -eq 0 ]
