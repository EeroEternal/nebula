#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_DIR="${ROOT_DIR}/logs"
ETCD_ENDPOINT="${ETCD_ENDPOINT:-http://127.0.0.1:2379}"
BFF_PORT="${BFF_PORT:-18090}"
ETCD_DATA_DIR="${ETCD_DATA_DIR:-/tmp/nebula-replit-etcd}"
TARGET="${ROOT_DIR}/target/release/nebula-bff"

mkdir -p "${LOG_DIR}" "${ETCD_DATA_DIR}"

# The dashboard backend only needs a local etcd instance. Respect an explicitly
# configured remote endpoint and only start etcd for the default local endpoint.
ETCD_URL="${ETCD_ENDPOINT%%,*}"
if ! curl --fail --silent --show-error "${ETCD_URL}/health" >/dev/null 2>&1; then
	if [[ "${ETCD_URL}" != "http://127.0.0.1:2379" && "${ETCD_URL}" != "http://localhost:2379" ]]; then
		echo "ERROR: configured ETCD_ENDPOINT is not reachable: ${ETCD_ENDPOINT}" >&2
		exit 1
	fi

	ETCD_BIN="$(command -v etcd || true)"
	if [[ -z "${ETCD_BIN}" ]]; then
		echo "ERROR: etcd is not installed; add the etcd system dependency before starting Nebula." >&2
		exit 1
	fi

	echo "Starting local etcd..."
	nohup "${ETCD_BIN}" \
		--name nebula-replit \
		--data-dir "${ETCD_DATA_DIR}" \
		--advertise-client-urls "${ETCD_URL}" \
		--listen-client-urls "${ETCD_URL}" \
		>"${LOG_DIR}/etcd.log" 2>&1 &

	for _ in {1..20}; do
		if curl --fail --silent --show-error "${ETCD_URL}/health" >/dev/null 2>&1; then
			break
		fi
		sleep 0.5
	done

	if ! curl --fail --silent --show-error "${ETCD_URL}/health" >/dev/null 2>&1; then
		echo "ERROR: local etcd did not become healthy; see ${LOG_DIR}/etcd.log." >&2
		exit 1
	fi
fi

# Replit exposes PostgreSQL through DATABASE_URL. The BFF uses its own database
# so console sessions do not share the platform database's application tables.
BFF_DATABASE_URL="${BFF_DATABASE_URL:-}"
if [[ -z "${BFF_DATABASE_URL}" ]]; then
	if [[ -z "${DATABASE_URL:-}" ]]; then
		echo "ERROR: DATABASE_URL is required to start nebula-bff in Replit." >&2
		exit 1
	fi
	BFF_DATABASE_URL="${DATABASE_URL%/*}/nebula"
fi

if ! psql --dbname="${BFF_DATABASE_URL}" -Atqc "SELECT 1" >/dev/null 2>&1; then
	if [[ -z "${DATABASE_URL:-}" ]]; then
		echo "ERROR: BFF database is unavailable and DATABASE_URL is not set to create it." >&2
		exit 1
	fi
	echo "Creating the dedicated Nebula database..."
	psql -d postgres -v ON_ERROR_STOP=1 \
		-c "CREATE DATABASE nebula" >/dev/null
fi

if [[ ! -x "${TARGET}" ]]; then
	echo "Building nebula-bff..."
	cargo build -p nebula-bff --release
fi

# A workflow restart can briefly leave the previous process behind. Reclaim
# only this project's BFF process before binding the configured port.
OLD_PIDS="$(pgrep -u "$(id -u)" -f "${TARGET}" || true)"
if [[ -n "${OLD_PIDS}" ]]; then
	kill ${OLD_PIDS} >/dev/null 2>&1 || true
	sleep 0.5
fi

echo "Starting Nebula BFF on 0.0.0.0:${BFF_PORT}..."
export BFF_DATABASE_URL
exec "${TARGET}" \
	--listen-addr "0.0.0.0:${BFF_PORT}" \
	--etcd-endpoint "${ETCD_ENDPOINT}" \
	--router-url "${ROUTER_URL:-http://127.0.0.1:18081}" \
	--xtrace-url "${OBSERVE_URL:-http://127.0.0.1:8742}" \
	--xtrace-auth-mode "${OBSERVE_AUTH_MODE:-internal}"