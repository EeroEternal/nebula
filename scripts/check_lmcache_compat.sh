#!/usr/bin/env bash
# check_lmcache_compat.sh — LMCache upgrade gate (P1c Batch 3).
#
# The no-NIXL PD form depends on LMCache's shared-KV contract (engines.md §7):
#   - lmcache_server boots and serves lm:// over TCP
#   - the config schema still accepts nebula's pinned keys
#   - chunk_size default stays 256 (prompts shorter than one chunk never
#     register as consumer hits — a trap we already paid for)
#   - vLLM still ships the zmq kv-events publisher (P1c's foundation)
#
# Usage:
#   ./scripts/check_lmcache_compat.sh            # quick: no GPU, ~1 min
#   ./scripts/check_lmcache_compat.sh --full     # + 2-GPU store/retrieve proof
#
# Env:
#   VLLM_IMAGE   engine image under test (default vllm/vllm-openai:v0.23.0)
#   LMCACHE_PIN  lmcache version the deployment pins (default 0.5.5)
#   MODEL_PATH   host model dir for --full (default /home/bodesi/models/Qwen2.5-7B-Instruct)
#
# Runs `pip install lmcache==$LMCACHE_PIN` inside the container — exactly what
# a deployment does on top of the base image (which ships an older lmcache).
set -euo pipefail

VLLM_IMAGE="${VLLM_IMAGE:-vllm/vllm-openai:v0.23.0}"
LMCACHE_PIN="${LMCACHE_PIN:-0.5.5}"
MODEL_PATH="${MODEL_PATH:-/home/bodesi/models/Qwen2.5-7B-Instruct}"
MODE="quick"; [ "${1:-}" = "--full" ] && MODE="full"

PASS=0; FAIL=0
ok()   { echo "  ✔ $1"; PASS=$((PASS+1)); }
bad()  { echo "  ✘ $1"; FAIL=$((FAIL+1)); }
note() { echo "  · $1"; }

WORK="$(mktemp -d /tmp/lmcache-gate.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

echo "== lmcache compat gate ($MODE) =="
echo "   image=$VLLM_IMAGE pin=lmcache-$LMCACHE_PIN"

# ---- quick checks (no GPU) ---------------------------------------------------
cat > "$WORK/quick.py" <<'PY'
import sys
checks = []
def check(name, fn):
    try:
        detail = fn()
        checks.append((True, name, detail))
    except Exception as e:
        checks.append((False, name, f"{type(e).__name__}: {e}"))

import lmcache
check("lmcache imports", lambda: lmcache.__version__)

def version_ok():
    v = lmcache.__version__.split(".")[:2]
    want = sys.argv[1].split(".")[:2]
    assert tuple(map(int, v)) >= tuple(map(int, want)), f"{lmcache.__version__} < pin {sys.argv[1]}"
    return lmcache.__version__
check(f"version >= pin {sys.argv[1]}", version_ok)

check("lmcache_server binary", lambda: __import__("shutil").which("lmcache_server") or "missing")

from lmcache.v1.config import LMCacheEngineConfig
check("config from_defaults", lambda: f"chunk_size={LMCacheEngineConfig.from_defaults().chunk_size}")

CFG = """local_cpu: true
max_local_cpu_size: 5
remote_url: lm://127.0.0.1:6899
remote_serde: naive
retrieve_locations: [RemoteBackend]
save_unfull_chunk: true
"""
def schema_ok():
    p = "/tmp/gate-cfg.yaml"
    open(p, "w").write(CFG)
    c = LMCacheEngineConfig.from_file(p)
    assert str(c.remote_url) == "lm://127.0.0.1:6899", c.remote_url
    assert c.save_unfull_chunk is True
    assert c.chunk_size == 256, f"chunk_size changed: {c.chunk_size} (prompts < chunk never hit)"
    return "nebula keys accepted, chunk_size=256"
check("nebula config schema", schema_ok)

import vllm.distributed.kv_events as ke
check("vllm zmq kv-events publisher",
      lambda: ",".join(sorted(ke.EventPublisherFactory._registry.keys())))

for good, name, detail in checks:
    print(("PASS " if good else "FAIL ") + name + " — " + detail)
sys.exit(1 if any(not g for g, _, _ in checks) else 0)
PY

echo "-- quick battery:"
docker run --rm --runtime=runc -v "$WORK:/work" --entrypoint bash "$VLLM_IMAGE" \
  -c "pip install -q lmcache==$LMCACHE_PIN 2>/dev/null; python3 /work/quick.py $LMCACHE_PIN" \
  > "$WORK/quick.out" 2>&1 || true
if grep -q "^FAIL" "$WORK/quick.out"; then
  bad "quick battery (see below)"; grep "^FAIL" "$WORK/quick.out"
elif grep -q "^PASS" "$WORK/quick.out"; then
  P=$(grep -c "^PASS" "$WORK/quick.out"); PASS=$((PASS+P))
  grep "^PASS" "$WORK/quick.out" | sed 's/^PASS /  ✔ /'
else
  bad "quick battery produced no results"; grep -vE "^INFO|WARNING" "$WORK/quick.out" | tail -15
fi

# ---- lmcache_server boot check (no GPU) --------------------------------------
echo "-- lmcache_server boot:"
if docker run --rm --runtime=runc --entrypoint bash "$VLLM_IMAGE" \
     -c "pip install -q lmcache==$LMCACHE_PIN 2>/dev/null; lmcache_server 127.0.0.1 6899 cpu > /tmp/srv.log 2>&1 & sleep 4; grep -q 'Server started' /tmp/srv.log && echo BOOT-OK; kill %1 2>/dev/null || true" \
     2>/dev/null | grep -q BOOT-OK; then
  ok "lmcache_server boots on plain TCP"
else
  bad "lmcache_server failed to boot"
fi

# ---- full: 2-GPU cross-engine store/retrieve ---------------------------------
if [ "$MODE" = "full" ]; then
  echo "-- full cross-engine proof (2 GPUs, ~3 min):"
  [ -d "$MODEL_PATH" ] || { bad "MODEL_PATH not found: $MODEL_PATH"; exit 1; }
  cat > "$WORK/prefiller.yaml" <<'YAML'
local_cpu: True
max_local_cpu_size: 5
remote_url: "lm://127.0.0.1:6800"
remote_serde: "naive"
save_unfull_chunk: true
YAML
  cat > "$WORK/decoder.yaml" <<'YAML'
local_cpu: True
max_local_cpu_size: 5
remote_url: "lm://127.0.0.1:6800"
remote_serde: "naive"
retrieve_locations: ["RemoteBackend"]
YAML
  cat > "$WORK/run.sh" <<'EOS'
#!/bin/bash
pip install -q lmcache==PIN 2>/dev/null
lmcache_server 127.0.0.1 6800 cpu > /work/remote.log 2>&1 &
sleep 2
export PYTHONHASHSEED=123 LMCACHE_USE_EXPERIMENTAL=True VLLM_ENABLE_V1_MULTIPROCESSING=1 VLLM_WORKER_MULTIPROC_METHOD=spawn
CUDA_VISIBLE_DEVICES=0 LMCACHE_CONFIG_FILE=/work/prefiller.yaml \
  vllm serve MODEL --served-model-name qwen --port 8100 --enforce-eager \
  --max-model-len 8192 --gpu-memory-utilization 0.80 \
  --kv-transfer-config '{"kv_connector":"LMCacheConnectorV1","kv_role":"kv_producer","kv_connector_extra_config":{"discard_partial_chunks":false,"lmcache_rpc_port":"producer1"}}' \
  > /work/prefill.log 2>&1 &
CUDA_VISIBLE_DEVICES=1 LMCACHE_CONFIG_FILE=/work/decoder.yaml \
  vllm serve MODEL --served-model-name qwen --port 8200 --enforce-eager \
  --max-model-len 8192 --gpu-memory-utilization 0.80 \
  --kv-transfer-config '{"kv_connector":"LMCacheConnectorV1","kv_role":"kv_both","kv_connector_extra_config":{"discard_partial_chunks":false,"lmcache_rpc_port":"consumer1"}}' \
  > /work/decode.log 2>&1 &
sleep 1
python3 - <<'PY' > /work/req.json
import json
w = ("capacity planning gpu kv cache transfer disaggregated prefill decode lmcache remote tcp "
     "router affinity block hash replica scheduling throughput latency benchmark ").split()
json.dump({"model":"qwen","prompt":" ".join(w[i%len(w)] for i in range(600)),
           "max_tokens":8,"temperature":0}, open("/work/req.json","w"))
PY
# wait for both engines
for i in $(seq 1 40); do
  a=$(curl -s -o /dev/null -w %{http_code} --max-time 3 http://127.0.0.1:8100/health || true)
  b=$(curl -s -o /dev/null -w %{http_code} --max-time 3 http://127.0.0.1:8200/health || true)
  [ "$a$b" = "200200" ] && break
  sleep 10
done
[ "$a$b" = "200200" ] || { echo ENGINES-UP-FAIL; exit 1; }
echo ENGINES-UP
# 1) prefiller stores (>= chunk_size prompt), 2) decoder retrieves
curl -s --max-time 180 -o /dev/null http://127.0.0.1:8100/v1/completions -H "Content-Type: application/json" -d @/work/req.json
sleep 3
curl -s --max-time 180 -o /dev/null http://127.0.0.1:8200/v1/completions -H "Content-Type: application/json" -d @/work/req.json
sleep 3
curl -s --max-time 180 -o /dev/null http://127.0.0.1:8200/v1/completions -H "Content-Type: application/json" -d @/work/req.json
sleep 3
grep -a "Stored" /work/prefill.log | tail -1
grep -aE "Retrieved" /work/decode.log | tail -1
grep -aqE "Retrieved [1-9][0-9]* out of" /work/decode.log && echo TRANSFER-OK || echo TRANSFER-FAIL
EOS
  sed -i "s|PIN|$LMCACHE_PIN|; s|MODEL|$MODEL_PATH|" "$WORK/run.sh"
  docker rm -f lmcache-gate >/dev/null 2>&1 || true
  docker run -d --name lmcache-gate --gpus all -v "$MODEL_PATH:$MODEL_PATH:ro" \
    -v "$WORK:/work" --network host --entrypoint bash "$VLLM_IMAGE" /work/run.sh >/dev/null
  OUT=""
  for i in $(seq 1 60); do
    sleep 10
    OUT=$(docker logs lmcache-gate 2>&1 | tail -4 || true)
    echo "$OUT" | grep -qE "TRANSFER-(OK|FAIL)|ENGINES-UP-FAIL" && break
  done
  docker rm -f lmcache-gate >/dev/null 2>&1 || true
  if echo "$OUT" | grep -q TRANSFER-OK; then
    ok "cross-engine store→retrieve over lm:// TCP ($(echo "$OUT" | grep -a Stored | tail -1 | grep -oE '[0-9]+/[0-9]+ tokens' | head -1))"
    note "$(echo "$OUT" | grep -a Retrieved | tail -1 | cut -c1-160)"
  else
    bad "cross-engine transfer (last: $(echo "$OUT" | tail -1 | cut -c1-140))"
  fi
fi

echo "== result: $PASS passed, $FAIL failed =="
exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
