#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "Installing locked frontend dependencies..."
(cd "${ROOT_DIR}/frontend" && npm ci --no-audit --no-fund)

echo "Building frontend..."
(cd "${ROOT_DIR}/frontend" && npm run build)

echo "Building Nebula BFF..."
(cd "${ROOT_DIR}" && cargo build -p nebula-bff --release)

echo "Post-merge setup complete."