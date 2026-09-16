# Cloud / local environment bridge

Non-obvious runtime notes. Canonical setup: [`docs/dev/setup.md`](../dev/setup.md), [`README.md`](../../README.md). Do not duplicate those docs here.

- **Rust:** pinned to 1.92 by `rust-toolchain.toml` (UniGateway MSRV). `cargo build --workspace --release` before `bin/nebula-up.sh` (it runs `target/release/` binaries).
- **etcd:** `etcd`/`etcdctl` may live in `~/bin`. `bin/nebula-up.sh` calls `~/bin/etcd` directly.
- **PostgreSQL (BFF/console only):** `sudo pg_ctlcluster 16 main start`. BFF: `postgresql://postgres:postgres@127.0.0.1:5432/nebula`; first start creates tables and seeds `admin` / `admin123`.
- **Stack:** copy `deploy/nebula.env.example` → `deploy/nebula.env` (gitignored); `START_BFF=1`, `OBSERVE_AUTH_MODE=internal`. Ports: gateway 8081, router 18081, bff 18090, etcd 2379, frontend 5173.
- **Gateway auth:** `NEBULA_AUTH_TOKENS=dev-token:admin` (`token:role` or `token:role:tenant_id`) + `Authorization: Bearer …`, or `NEBULA_AUTH_DISABLED=1`. Multi-tenant: `NEBULA_MULTI_TENANT=1` + etcd `/tenants/{id}`.
- **No GPU in cloud VMs:** real `nebula-node` + vLLM is excluded from `docker-compose.yml`. Mock an OpenAI endpoint at etcd `/endpoints/{model_uid}/{replica_id}` (`status: "ready"`, reachable `base_url`).
- **Frontend:** `npm run dev` in `frontend/` proxies `/api` → BFF `:18090`. Login is `/login`; clearing `localStorage` does not redirect.
