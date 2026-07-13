#!/usr/bin/env bash
# Verify live control-plane metrics after etcd-based stats sync (P0-2+).
# Legacy xtrace-as-routing-signal probes were removed with dead
# nebula_{router,scheduler}_xtrace_* counters.
set -euo pipefail

HOST="${1:-10.21.11.92}"
USER_NAME="${2:-ai}"

ROUTER_METRICS_URL="http://${HOST}:18081/metrics"
SCHED_METRICS_URL="http://${HOST}:18082/metrics"

pass() { echo "[PASS] $*"; }
fail() { echo "[FAIL] $*"; exit 1; }
info() { echo "[INFO] $*"; }

metric_present() {
  local url="$1"
  local name="$2"
  curl -fsS "$url" | awk -v metric="$name" '$1 == metric {found=1} END {exit !found}'
}

metric_absent() {
  local url="$1"
  local name="$2"
  ! curl -fsS "$url" | awk -v metric="$name" '$1 ~ "^"metric"({|$)" {found=1} END {exit !found}'
}

info "target host: ${HOST}"
ssh -o BatchMode=yes -o ConnectTimeout=8 "${USER_NAME}@${HOST}" "echo ok" >/dev/null
pass "ssh reachable"

curl -fsS "$ROUTER_METRICS_URL" >/dev/null || fail "router /metrics unreachable"
curl -fsS "$SCHED_METRICS_URL" >/dev/null || fail "scheduler /metrics unreachable"
pass "router and scheduler /metrics reachable"

metric_present "$ROUTER_METRICS_URL" "nebula_router_route_stale_stats_dropped_total" \
  || fail "missing nebula_router_route_stale_stats_dropped_total"
pass "router exposes route_stale_stats_dropped"

metric_present "$SCHED_METRICS_URL" "nebula_scheduler_reconcile_total" \
  || fail "missing nebula_scheduler_reconcile_total"
pass "scheduler exposes reconcile_total"

for dead in \
  nebula_router_xtrace_stale_total \
  nebula_router_xtrace_query_errors_total \
  nebula_scheduler_xtrace_stale_total \
  nebula_scheduler_xtrace_query_errors_total
do
  if curl -fsS "$ROUTER_METRICS_URL" 2>/dev/null | grep -q "^${dead}" \
    || curl -fsS "$SCHED_METRICS_URL" 2>/dev/null | grep -q "^${dead}"; then
    fail "dead metric still exported: ${dead}"
  fi
done
pass "legacy xtrace routing-signal metrics are absent"

info "degradation check complete"
