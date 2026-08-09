#!/usr/bin/env bash
# Probe Nebula stack: etcd endpoints, node stats, engine alerts, Docker containers, GPU.
set -euo pipefail

ETCD="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
NODE_METRICS="${NODE_METRICS:-http://127.0.0.1:9090/metrics}"
MODEL_UID="${MODEL_UID:-}"

echo "=== etcd endpoints ==="
if command -v etcdctl >/dev/null 2>&1; then
  etcdctl --endpoints="$ETCD" get --prefix /endpoints/ --keys-only 2>/dev/null || true
  etcdctl --endpoints="$ETCD" get --prefix /endpoints/ 2>/dev/null | head -80 || true
else
  curl -sf "${ETCD}/v3/kv/range" -X POST -H 'Content-Type: application/json' \
    -d '{"key":"L2VuZHBvaW50cy8=","range_end":"L2VuZHBvaW50c4A="}' 2>/dev/null | head -20 || echo "(etcd unreachable)"
fi

echo ""
echo "=== engine probe alerts (/alerts/) ==="
if command -v etcdctl >/dev/null 2>&1; then
  etcdctl --endpoints="$ETCD" get --prefix /alerts/ 2>/dev/null | tail -40 || true
fi

echo ""
echo "=== node metrics (probe / GPU) ==="
curl -sf "$NODE_METRICS" 2>/dev/null | grep -E 'nebula_node_engine_(probe_failures|container_running|kv_cache_usage|pending_requests)' || echo "(node metrics unreachable)"

echo ""
echo "=== Docker engine containers ==="
docker ps -a --filter 'name=nebula-' --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}' 2>/dev/null || true

if [[ -n "$MODEL_UID" ]]; then
  cname="nebula-${MODEL_UID//[^a-zA-Z0-9_]/_}-0"
  echo ""
  echo "=== inspect $cname ==="
  docker inspect -f 'Running={{.State.Running}} OOMKilled={{.State.OOMKilled}} ExitCode={{.State.ExitCode}} Status={{.State.Status}}' "$cname" 2>/dev/null || echo "(container not found)"
fi

echo ""
echo "=== GPU (nvidia-smi) ==="
nvidia-smi --query-gpu=index,name,memory.used,memory.total,utilization.gpu,temperature.gpu --format=csv,noheader 2>/dev/null || echo "(no GPU)"
