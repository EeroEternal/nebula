#!/usr/bin/env bash
# Create or list Gateway platform API keys (Postgres).
set -euo pipefail

DB_URL="${NEBULA_PLATFORM_DB_URL:-${NEBULA_BFF_DATABASE_URL:-postgresql://postgres:postgres@127.0.0.1:5432/nebula}}"

usage() {
  cat <<EOF
Usage:
  $0 create --name NAME --role viewer|operator|admin [--scopes inference,control,admin] [--tenant TENANT_ID]
  $0 list

Environment:
  NEBULA_PLATFORM_DB_URL or NEBULA_BFF_DATABASE_URL (default: local nebula DB)
EOF
}

cmd="${1:-}"
shift || true

case "$cmd" in
  create)
    name=""
    role="operator"
    scopes="inference,control"
    tenant=""
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --name) name="$2"; shift 2 ;;
        --role) role="$2"; shift 2 ;;
        --scopes) scopes="$2"; shift 2 ;;
        --tenant) tenant="$2"; shift 2 ;;
        *) echo "unknown arg: $1"; usage; exit 1 ;;
      esac
    done
    if [[ -z "$name" ]]; then
      echo "--name is required"
      exit 1
    fi
    secret="nb_${RANDOM}_${RANDOM}_$(date +%s)"
    IFS=',' read -r -a scope_arr <<< "$scopes"
    pg_scopes="{$(printf '%s,' "${scope_arr[@]}" | sed 's/,$//')}"
    psql "$DB_URL" -v ON_ERROR_STOP=1 <<SQL
INSERT INTO platform_api_keys (name, key_hash, role, scopes, tenant_id)
VALUES (
  '$name',
  encode(sha256('$secret'::bytea), 'hex'),
  '$role',
  '$pg_scopes'::text[],
  $(if [[ -n "$tenant" ]]; then echo "'$tenant'"; else echo "NULL"; fi)
);
SQL
    echo "Created API key for '$name' (store this secret; shown once):"
    echo "$secret"
    ;;
  list)
    psql "$DB_URL" -c "SELECT name, role, scopes, tenant_id, created_at, revoked_at FROM platform_api_keys ORDER BY created_at DESC;"
    ;;
  *)
    usage
    exit 1
    ;;
esac
