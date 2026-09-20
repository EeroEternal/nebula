#!/usr/bin/env bash
# Local CI — replaces the removed GitHub Actions workflow (.github/workflows/ci.yml).
#
# Usage (from repo root or anywhere):
#   ./scripts/ci.sh
#   RUN_SMOKE=1 ./scripts/ci.sh
#
# Jobs (mirrors the old GitHub Actions workflow):
#   1. cargo fmt --check
#   2. cargo clippy --all-targets -- -D warnings
#   3. cargo test --workspace --all-targets
#   4. ./scripts/check_openapi_control.sh
#   5. bash scripts/check_ui_stack.sh && bash scripts/check_admin_nav.sh
#   6. frontend gate: npx tsc -b --noEmit && npm run lint && npm run build
#   7. (optional) release-build gateway/router/bff + ./scripts/ci_smoke.sh
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

echo "========== cargo fmt --check =========="
cargo fmt --check

echo "========== cargo clippy =========="
cargo clippy --all-targets -- -D warnings

echo "========== cargo test --workspace =========="
cargo test --workspace --all-targets -- --nocapture

echo "========== openapi-control =========="
./scripts/check_openapi_control.sh

echo "========== UI stack + nav checks =========="
bash scripts/check_ui_stack.sh
bash scripts/check_admin_nav.sh

echo "========== frontend gate (tsc + lint + build) =========="
if [ -f frontend/package.json ]; then
  if [ ! -d frontend/node_modules ]; then
    (cd frontend && npm ci --no-audit --no-fund)
  fi
  (cd frontend && npx tsc -b --noEmit && npm run lint && npm run build)
else
  echo "frontend/package.json not found; skipping frontend gate"
fi

if [ "${RUN_SMOKE:-0}" = "1" ]; then
  echo "========== smoke (mock engine) =========="
  cargo build --release -p nebula-gateway -p nebula-router -p nebula-bff
  ./scripts/ci_smoke.sh
fi

echo "========== local CI ok =========="
