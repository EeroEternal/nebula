#!/usr/bin/env bash
# Validate docs/dev/openapi-control.yaml (I3.5 contract CI gate).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SPEC="$ROOT/docs/dev/openapi-control.yaml"

required_paths=(
  "/platform/v1/models"
  "/platform/v1/models/load"
  "/platform/v1/models/{model_uid}/deployment"
  "/platform/v1/models/{model_uid}/stop"
  "/platform/v1/models/{model_uid}/replicas"
  "/platform/v1/nodes"
  "/platform/v1/operations/{operation_id}"
  "/platform/v1/operations/{operation_id}/events"
  "/platform/v1/health/summary"
  "/platform/v1/audit-logs"
  "/platform/v1/models/{model_uid}/slo"
  "/platform/v1/models/{model_uid}/slo/evaluation"
  "/platform/v1/canaries"
  "/platform/v1/canaries/{canary_id}"
)

missing=()
for path in "${required_paths[@]}"; do
  if ! grep -Fq "  ${path}:" "$SPEC"; then
    missing+=("$path")
  fi
done

if ((${#missing[@]} > 0)); then
  echo "openapi-control.yaml missing paths:" "${missing[@]}" >&2
  exit 1
fi

echo "openapi-control.yaml ok (${#required_paths[@]} required paths present)"
