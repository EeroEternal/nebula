#!/usr/bin/env bash
# Local CI — replaces the removed GitHub Actions workflow (.github/workflows/ci.yml).
#
# Usage (from repo root or anywhere):
#   ./scripts/ci.sh
#   RUN_SMOKE=1 ./scripts/ci.sh
#
# Jobs (same as the old workflow):
#   1. cargo test --workspace --all-targets
#   2. ./scripts/check_openapi_control.sh
#   3. (optional) release-build gateway/router/bff + ./scripts/ci_smoke.sh
#
# Env:
#   RUN_SMOKE=1   also build release binaries and run mock-engine smoke
#                 (needs docker compose / etcd, see scripts/ci_smoke.sh)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found (install rustup; toolchain is pinned in rust-toolchain.toml)" >&2
  exit 1
fi

echo "========== cargo test --workspace =========="
cargo test --workspace --all-targets -- --nocapture

echo "========== openapi-control =========="
./scripts/check_openapi_control.sh

if [ "${RUN_SMOKE:-0}" = "1" ]; then
  echo "========== smoke (mock engine) =========="
  cargo build --release -p nebula-gateway -p nebula-router -p nebula-bff
  ./scripts/ci_smoke.sh
fi

echo "========== local CI ok =========="
