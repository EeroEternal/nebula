# Nebula Agent Rules

This document defines rules and conventions for AI agents and developers working on the Nebula project.

## File Organization

- **Documentation:** Keep filenames in the `docs/` directory concise and descriptive. Avoid excessively long names.
  - `docs/manual/` — 产品与运维：能力说明、部署、Gateway/Router、可观测、SLO、BFF、Catalog、HA
  - `docs/versions/` — Release Notes（`v1.3.0.md` 等）
  - `docs/dev/` — 工程细节：开发环境、计划、API 边界、契约、UniGateway
  - `docs/arch/` — architecture and roadmap
  - Index: `docs/README.md`（唯一索引；子目录不再放 README）
  - Do not commit environment-specific runbooks, internal IPs, or secrets under `docs/`.
  - **Keep docs short:** prefer one screen of facts over essays. State current behavior, boundaries, and links to code; delete completed checklists, speculative plans, and historical analysis once shipped. Prefer renaming `*_integration.md` / `*_plan.md` to the topic name (e.g. `unigateway.md`) when the work is done.
  - **`manual/` 不写进度：** 不含 P0–P6、roadmap 勾选、Phase/O 编号；只写当前版本能力与运维做法。
  - **`manual/` 读者可能是非开发岗**（产品、运维）：先写「是什么、能干什么、出问题怎么办」；术语要解释；环境变量、PromQL 等放到「实施参考」小节。
  - **Short filenames:** use topic names, not suffixes — `plan.md` not `product_plan.md`, `roadmap.md` not `optimization.md`, `slo.md` not `slo_alerts.md`, `versions/v1.3.0.md` not `release_notes_v1.3.0.md`. Ops docs under `manual/`; version notes under `versions/`.
- **Scripts & Tests:**
    - All standalone test scripts, debug scripts, and utility scripts must be placed in the `scripts/` directory.
    - Production-ready binaries and service management scripts belong in `bin/`.
- **Temporary Data:** Do not store temporary data (like `default.etcd`) in the project root. Use `/tmp` or other designated temporary locations.

## Versioning

- Follow semantic versioning for releases.
- Ensure `CHANGELOG.md` is updated when releasing a new version.

## Cursor Cloud specific instructions

Services and standard commands are documented in `README.md` and `docs/dev/setup.md`; the notes below only capture non-obvious cloud caveats.

- **Rust toolchain:** Some dependencies (e.g. `unigateway-core`) require `edition2024`, so a toolchain `>= 1.85` is mandatory. The base image pins an older default (1.83); run `rustup default stable` if a build fails with an `edition2024` error. Build binaries with `cargo build --release` before using `bin/nebula-up.sh` (it runs prebuilt binaries from `target/release/`).
- **etcd:** `etcd`/`etcdctl` live in `~/bin` (not on the default `PATH`). `bin/nebula-up.sh` invokes `~/bin/etcd` directly; call `etcdctl` with the full path.
- **PostgreSQL (only needed for the BFF/console):** start with `sudo pg_ctlcluster 16 main start`. BFF expects a dedicated `nebula` database reachable at `postgresql://postgres:postgres@127.0.0.1:5432/nebula`; it auto-creates tables and seeds the default console admin `admin` / `admin123` on first start.
- **Running the stack:** copy `deploy/nebula.env.example` to `deploy/nebula.env` (gitignored) and set `START_BFF=1`, `OBSERVE_AUTH_MODE=internal` (no xtrace token needed in dev). Then `./bin/nebula-up.sh`. Ports: gateway 8081, router 18081, bff 18090, etcd 2379, frontend 5173.
- **Gateway auth:** the OpenAI-compatible API enforces auth. Set `NEBULA_AUTH_TOKENS=dev-token:admin` (format `token:role` or `token:role:tenant_id`) and call with `Authorization: Bearer dev-token`, or set `NEBULA_AUTH_DISABLED=1` for dev. Optional multi-tenant admission: `NEBULA_MULTI_TENANT=1` plus etcd `/tenants/{id}` quotas.
- **No GPU here:** real inference via `nebula-node` + vLLM cannot run (needs GPU + model files; it is intentionally excluded from `docker-compose.yml`). To exercise the `gateway → router → engine` passthrough, register a mock OpenAI endpoint in etcd at `/endpoints/{model_uid}/{replica_id}` (JSON `EndpointInfo` with `status: "ready"` and a reachable `base_url`); the router picks it up within ~1s.
- **Frontend:** `npm run dev` in `frontend/` proxies `/api` → BFF on `:18090`. The login page is at `/login`; clearing `localStorage` alone does not redirect, so navigate to `/login` directly to reach the login form.
