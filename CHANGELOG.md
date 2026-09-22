# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog.

## [Unreleased]

## [1.10.0] - 2026-09-22

### Added
- **Engine-level bounded fair queue admission (P0-#2).** The Gateway now admits requests through a per-model bounded queue before reaching the engine: `NEBULA_GATEWAY_QUEUE_ENABLED`, `NEBULA_GATEWAY_QUEUE_MAX_CONCURRENCY` (in-flight cap per model), `NEBULA_GATEWAY_QUEUE_MAX_WAITERS`, `NEBULA_GATEWAY_QUEUE_TIMEOUT_MS`, `NEBULA_GATEWAY_QUEUE_MODE` (`fair` | `fast_fail`), and `NEBULA_GATEWAY_QUEUE_TENANT_SHARE` (per-tenant share of concurrency). Over-limit requests are rejected with **HTTP 429** instead of being queued unboundedly. The guard spans the whole streaming lifetime, exposes queue depth/in-flight/wait metrics, and separates TTFT into `x-nebula-queue-wait-ms` and `x-nebula-upstream-ttft-ms` response headers. New module `crates/nebula-common/src/queue.rs` (`EngineQueue` / `EngineQueues` / `QueueConfig` / `QueueMode` / `EnginePermit`). Real-machine (RTX 5090, Qwen2.5-7B): engine-side P99 TTFT at concurrency 64 dropped **311 ms → 82 ms (−74 %)** with the same throughput.
- **Multiple engine replicas per model.** `ModelDeployment.replicas` now provisions N engine pods per model (`nebula-{model}` for replica 0, `nebula-{model}-{n}` for the rest), each pinned to a distinct GPU, each registering its own `/endpoints/{model}/{replica}` lease and its own `/stats/{model}/{replica}` from that replica's `/metrics` scrape. Scaling down deletes surplus replicas, terminal pods are recreated, and stale endpoints are dropped when a replica leaves `Running`. Real-machine: 2 replicas, `LeastPending` split 132/128 (≈ 50/50), prefix-cache hit rate 0.92/0.93.
- **Multi-engine alignment (vLLM / SGLang).** Engine metric parsing now lives in `nebula_common::engine_metrics` and is dispatched by `parse_engine_metrics(engine_type, …)`; both the bare-metal node and the Kubernetes controller reuse it instead of keeping private copies. The Kubernetes controller selects image and launch arguments from `ModelSpec.engine_type` (`vllm/vllm-openai:latest` vs `lmsysorg/sglang:latest`) and stamps `engine_type` onto the endpoint. Real-machine: an SGLang pod registers an endpoint with `engine_type: "sglang"`, its `/stats` come from the SGLang parser (`kv_cache_usage` ← `token_usage`), and a chat request through the Gateway returns a valid completion. See [`docs/dev/engines.md`](../../docs/dev/engines.md).
- **`replica_specs[i].node_id` placement.** The Kubernetes controller now honours the per-replica target node (`node_id`, defaulting to the agent's own node) instead of placing every replica on its own `--gpu-node`; with multiple node agents this previously created duplicate pods for the same replica. `replica_specs[i].gpu_indices` was already honoured. Real-machine: `gpu_indices: [1, 3]` pins replica 0 to GPU 1 and replica 1 to GPU 3, and a replica pointed at another node is not created locally.
- **Opt-in embedded router mode.** `NEBULA_ROUTER_MODE=embedded` (`NEBULA_ROUTER_STRATEGY=…`) routes `/v1/chat/completions`, `/v1/responses`, and `/v1/messages` through an in-process router with retry parity and router metrics, removing the separate `nebula-router` hop. Measured effect on P99 is **neutral** (the hop is loopback), so this remains opt-in; `remote` is still the default.
- **Opt-in per-stage latency forensics.** `NEBULA_GATEWAY_STAGE_TIMING=1` logs per-request `recv → body → prep → upstream_hdr → first_chunk` timings correlated by request id. Measured `recv → first_chunk` p99 tracks direct-to-engine TTFT, i.e. the Gateway's own processing overhead is ≈ 0.
- **Operations scripts.** `scripts/nebula-stack.sh` (`start` / `stop` / `status`) brings up and tears down etcd + controller + router + gateway + BFF, and `scripts/verify-embedded-router.sh` verifies the embedded-router path end to end.
- **Design and measurement documents.** New reports under `docs/dev/`: `hw/vllm-serving-perf.md` (engine-boundary latency decomposition, vllm-rs vs Python frontend, plugin-system analysis, zero-code optimization checklist), `landscape.md` (vs NVIDIA Dynamo / llm-d), `direction.md` (strategic and product/performance focus), `queue-admission.md` (P0-#2 design), `prefix-affinity.md` (P1), and `engines.md` (multi-engine alignment).

### Changed
- **Engine metric parsers moved into `nebula-common`.** `parse_vllm_metrics_text` / `parse_sglang_metrics_text` / `parse_engine_metrics` and the `extract_*_metric` helpers now live in `crates/nebula-common/src/engine_metrics.rs`; `crates/nebula-node/src/engine/*` re-use them and the Kubernetes controller scrapes through the same dispatch.
- **Router sync loops promoted into the library.** `crates/nebula-router/src/sync.rs` is public so the Gateway can embed the same loops instead of re-implementing them.

### Fixed
- **Engine metric substring over-match.** `extract_metric` matched metric names by substring, so querying `prefix_cache_*` also picked up `external_prefix_cache_*` counters and corrupted the derived hit-rate. Matching is now by full metric name (`crates/nebula-common/src/engine_metrics.rs`).
- **SGLang would not come up reachable under the Kubernetes controller.** SGLang binds `127.0.0.1` by default (vLLM binds `0.0.0.0`), so the pod IP was unreachable; the controller now passes `--host 0.0.0.0` and `--enable-metrics` (without the latter `/metrics` returns 404).
- **SGLang endpoints expired instead of refreshing.** SGLang's `/health` responds in **> 800 ms**, so the controller's 800 ms readiness probe failed and the `/endpoints/…` lease was left to expire; the probe timeout is raised to 3 s.
- **Kubernetes controller scale-down and pod recovery.** Surplus replicas are deleted on scale-down and pods stuck in a terminal phase are recreated; stale endpoints are removed when a replica is not `Running`.
- **`nebula-stack stop` left replica pods behind.** The script deleted only the replica-0 pod; it now deletes every pod carrying the `nebula.model_uid` label.
- **`Cargo.lock` workspace versions** were out of sync with `Cargo.toml` after the v1.9.2 cut.

### Removed
- _(none)_

## [1.9.2] - 2026-09-20

### Changed
- **Frontend lint rules adopted from console-kit (no behavior change).** `frontend/eslint.config.js` now enforces console-kit's token and restricted-syntax rules: no hard-coded hex (use `index.css` semantic tokens), no palette-ramp classes (`bg/text/border-{gray,zinc,slate,red,…}-NN`), and no native `<select>` / `<dialog>` / `window.alert|confirm|prompt`. The six remaining native `<select>` usages were migrated to the shared `Select` component (`LoadModelDialog`, `account-settings`, `governance`, `images` ×2, `model-catalog`). `eslint`, `tsc -b --noEmit`, and the `vite` build are green.
- **Local CI consolidated into `./scripts/ci.sh`.** The single local gate now mirrors the removed GitHub Actions workflow end-to-end: `cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo test --workspace --all-targets` → OpenAPI contract check → UI-stack/nav checks → frontend `tsc + eslint + build`. `RUN_SMOKE=1` still opts into the release-build mock-engine smoke. The `pre-push-local-gates` skill checklist now points at the single entry point.

## [1.9.1] - 2026-09-20

### Changed
- **Doc boundary for the PowerLLM compatibility layer (no behavior change).** `docs/dev/integration.md`, `docs/arch/control-plane-adapters.md`, `docs/arch/vision.md`, and `docs/arch/architecture.md` now state explicitly that the hot path (Gateway `:8081` / Router) and the etcd control plane stay platform-agnostic with zero platform branches, while the BFF (`:18090`) PowerLLM console compatibility layer shipped in v1.9.0 is a console-edge southbound adapter — not a `CONTROL_PLANE` adapter and never on the hot path. Removes the contradiction with the v1.9.0 release notes.
- **Local-gate hygiene (no functional change).** The v1.9.0 tree did not pass the pre-push gates: `cargo fmt` was unaligned, `cargo clippy --all-targets -- -D warnings` reported 57 findings, and the frontend ESLint run reported 8 errors. All are resolved — realigned formatting; dropped genuinely dead code (gateway `engine.rs` streaming scaffold and `AppState.engine`, `not_implemented`, `idempotency_conflict`, `control_error_with_request_id`, `openai_sse_content_delta`, `list_disk_alerts`/`DeployReject` aliases, `STARTUP_GRACE_MS`, `Engine::validate_config`); marked intentional-but-unused contracts with reasoned `#[allow]`; and fixed the frontend lint findings. `./scripts/ci.sh`, fmt, clippy, `tsc`, ESLint, and the frontend build are green.

### Fixed
- **Admin console `npm install` in a nested checkout.** `admin/package.json` `prepare` no longer hard-fails on `husky install` when `admin/` is vendored without a surrounding git repository (`test -d ../.git && husky install || true`).

## [1.9.0] - 2026-09-20

### Added
- **Kubernetes Execution Plane (`crates/nebula-k8s-controller`):** Dedicated in-cluster reconciliation controller for declarative vLLM engine Pods lifecycle, adhering to etcd-only control plane authority and Pod `/endpoints/` lease registration.
- **PowerLLM Protocol Compatibility Layer:** Full API compatibility in `crates/nebula-bff` (`/token`, `/v1/user/*`, `/v1/device/info`, `/v1/models/instances`) enabling 100% drop-in backend replacement for PowerLLM.
- **PowerLLM Admin Console (`admin/`):** Full-featured Ant Design Pro + Umi Max web console integrated into the repository with automated proxy to Nebula BFF.
- **Hot-path Engine Direct Pass-through:** Gateway and Router bypass Python runtime supervisor layers, reducing TTFT by 66.7% and P99 latency by 78.4%.

### Changed
- **Local CI:** `./scripts/ci.sh` replaces GitHub Actions (workspace tests + OpenAPI contract; `RUN_SMOKE=1` for mock-engine smoke).
- **Agent kit:** Adopted console-kit standing rules (`AGENTS.md` ≤80 lines), `.agents/skills/`, `docs/design.md`, and local UI gates (`scripts/check_ui_stack.sh`, `scripts/check_admin_nav.sh`). Product UI remains `frontend/`; GitHub Actions are still forbidden.
- **Admin UI kit:** Product pages use console-kit page types (`PageShell` / `PageHeader` / `StatCard` / entity lists). Settings is sectioned (session / profile / account / locale). Login remains Auth split.

## [1.8.0] - 2026-08-29

### Added
- **HardwarePool Resource Abstraction (`/pools/`):** Define logical heterogeneous GPU/NPU resource pools with role, platform, node membership, and schedulability controls.
- **Pool-Constrained Placement Scheduling:** `ModelDeployment` supports `allowed_pools` to restrict replica placement to designated pools with explicit capacity rejection.
- **Node-level Graceful Drain:** `POST /platform/v1/nodes/:node_id/drain` and `POST /api/v2/nodes/:node_id/drain` to gracefully drain all model replicas on a physical node prior to maintenance.
- **CLI Commands:** Added `nebula pool [list|get|create|delete]` and `nebula drain --node-id <node_id>`.

## [1.7.0] - 2026-08-29

### Removed
- **Nebula Lite (`crates/nebula-lite`):** Deprecated and removed the single-node local launcher; Nebula strictly focuses on distributed cluster control-plane coordination.
- **GitHub Workflows:** Cleaned up unused automation workflows and stale templates.

### Changed
- **BFF Storage Migration (PostgreSQL):**
  - Migrated deployment templates (`/templates/`) to `bff_templates` table in PostgreSQL.
  - Migrated L3 model profiles (`/model_profiles/`) to `bff_model_profiles` table in PostgreSQL.
  - Migrated tenant pricing (`/pricing/`) to `bff_pricing` table in PostgreSQL.
  - Migrated window usage & metrics (`/usage/{tenant}/{window}`) to `bff_usage` table in PostgreSQL.
  - Migrated benchmark runs and profiles (`/benchmarks/*`) to `bff_benchmark_runs` and `bff_benchmark_profiles` in PostgreSQL.
- **etcd Clarification:** etcd keyspace strictly pruned to coordination truths (`/deployments/`, `/placements/`, `/endpoints/`, `/stats/`, `/capabilities/`, leases & election).

## [1.6.0] - 2026-08-11

### Removed
- **Gateway `/v1/admin/*`:** All legacy Admin routes, BFF v2 proxy, deprecation middleware, and local admin logs/metrics/image registry endpoints.
- **`nebula-cli`:** No longer calls `/v1/admin/*`; control uses `/platform/v1/*`, console v2 uses BFF `/api/v2/*` directly.

### Added
- **`GET /platform/v1/cluster/status`**, **`GET /platform/v1/whoami`**, **`POST /platform/v1/replicas/drain`** — migrated from removed Admin paths.

### Changed
- OpenAPI and integration docs describe `/platform/v1` as the sole control contract.

## [1.5.0] - 2026-08-11

### Added
- **Control API `/platform/v1/*` (I1):** ModelSpec CRUD, deployment load/scale/stop, replicas/nodes inventory, async Operations, Postgres API Key auth (`control` / `inference` / `admin` scope).
- **Integration I2:** Operation SSE (`GET …/operations/{id}/events`), inference `x-nebula-request-id` echo, optional `x-nebula-replica-id` replica pin.
- **Integration I2.4:** Per-replica heterogeneous placement via `replica_specs[]` on deployment.
- **Integration I2.5:** Operation webhooks — `callback_url` on write requests + `GET/POST/DELETE /platform/v1/webhooks` subscriptions (Postgres, HMAC signature).
- **Integration I3:** `GET /platform/v1/health/summary`, audit-logs read API, `Idempotency-Key` on Control POST, OpenAPI contract CI.
- **Integration I4:** Governance read API — `GET …/models/{uid}/slo`, `…/slo/evaluation`, `GET /canaries`.
- **Integration I5:** Legacy Admin `Sunset` header (2027-02-11, removal v1.6.0).
- **`nebula-control` crate (I0):** Single write path for Gateway Admin and BFF; unified compat matrix and C3 errors.

### Changed
- Legacy `/v1/admin/*` responses include `Deprecation: true`, `Sunset: Thu, 11 Feb 2027 23:59:59 GMT`, and `Link` successor-version header; removal target v1.6.0.
- OpenAPI [`docs/dev/openapi-control.yaml`](docs/dev/openapi-control.yaml) documents `/platform/v1` as the stable contract.

### Deprecated
- `/v1/admin/models/load`, `/v1/admin/v2/*` BFF proxy — use `/platform/v1/*` instead.

## [1.4.0] - 2026-07-14

### Added
- **Nebula Lite** (`nebula-lite`): single-process local launcher that spawns vLLM or SGLang and proxies OpenAI-compatible HTTP (`/v1/chat/completions`, etc.). No etcd / BFF / Router / Scheduler. See `docs/dev/lite.md`.

### Removed
- **Serving Cell (CellIngress):** etcd `/cells/`、BFF `/api/v2/cells`、Router Cell 选路与不重试分支、相关契约与控制台能力已下线。

## [1.3.0] - 2026-07-13

Product-alignment release covering P0–P6 from `docs/dev/plan.md`. Real GPU e2e and multi-tenant load tests remain deferred. Serving Cell (P2) was later removed in v1.4.0.

### Added
- **P0 Observability trust:** `kv_cache_usage` semantics, scrape health metrics/fixtures, Gateway API `data_source: router`, O8 SLO/alert runbook samples.
- **P1 Engine Capability / Adapter:** capability contracts, static tables, dialect CLI, runtime discovery persisted to etcd `/capabilities/`, engine version support checks.
- **P2 Serving Cell (later removed):** historically shipped etcd `/cells/` CRUD and Router whole-ingress routing; capability retired — see Unreleased.
- **P3 Compat / hardware ledger:** `NodeStatus.platform` + GPU identity, `CompatibilityRule` + etcd `/compat/`, placement rejects, inventory API, console governance matrix.
- **P4 Model SLO / diagnostics:** `ModelSlo` + evaluate (never fake-green on low traffic), `DiagnosticEvent` timeline, console governance SLO panel.
- **P5 Benchmark / recommend / canary:** `scripts/benchmark/`, etcd profiles/runs, recommend API (insufficient data → no silent default engine), canary promote/rollback, console panels.
- **P6 Multi-tenant / cost:** `Tenant` + quotas, `token:role[:tenant_id]`, Gateway admission (`NEBULA_MULTI_TENANT`), usage/pricing/cost APIs, low-cardinality deny metrics, audit tenant/deny tags, console tenant view.
- Shared `ExecutionContext` header propagation (`x-nebula-tenant-id` / priority / deadline / budget).

### Changed
- Auth tokens accept optional tenant binding; rate-limit keys are tenant-scoped when bound.
- Product plan / optimization docs mark P0–P6 Batch 1 complete; true hardware e2e left paused.

## [0.2.0] - 2026-07-11

### Added
- Node multi-replica reconcile keyed by `(model_uid, replica_id)`, with periodic full reconcile to advance Drain/orphan cleanup.
- Scheduler scale-down when healthy replicas exceed desired (`select_replicas_to_remove`).
- Endpoint recovery budget (24h / 5 attempts + backoff → `Failed`) and process-group cleanup on local restart.
- Meta `list_prefix_snapshot` and Router per-model `plan_version` filtering (snapshot-revision watch).
- Cancel/SSE contract script: `scripts/test_cancel_sse.sh`.
- Docs layout: `docs/arch/`, `docs/dev/`, `docs/manual/` with index at `docs/README.md`.
- Shared HTTP client helpers in `nebula-common` (`proxy` / `control_plane` / `health` / `audit` presets).
- Comma-separated `ETCD_ENDPOINT` multi-address support for HA clients.
- HA topology: `docker-compose.ha.yml`, `deploy/ha/Caddyfile.*`, `scripts/phase_d_ha_drill.sh`.
## [0.1.1] - 2026-04-28

### Changed
- Reorganized project structure: moved test and debug scripts from root to `scripts/`.
- Renamed long documentation filenames to more concise alternatives.
- Configured `etcd` in `bin/nebula-up.sh` to use a temporary data directory (`/tmp/nebula-etcd`) instead of the project root.

### Added
- Added `AGENTS.md` to define project organization rules for AI agents and developers.

## [2026-02-15]

### Added
- Added full frontend i18n infrastructure via `frontend/src/lib/i18n.tsx` with locale persistence and interpolation support.
- Added Chinese/English language switch entry in the account menu and default locale bootstrap to Chinese.
- Added `docs/i18n_acceptance_checklist.md` for end-to-end bilingual QA and regression checks.

### Changed
- Migrated major frontend views and dialogs to translation keys, including Dashboard, Models, Inference, Catalog/Library, Templates/Images, Login, Settings, Profile, and Account pages.
- Updated app bootstrap to wrap `App` with `I18nProvider` in `frontend/src/main.tsx`.
- Expanded i18n dictionaries to cover all currently referenced frontend translation keys in both `zh` and `en`.

### Changed
- Updated `bin/nebula-up.sh` with xtrace preflight validation to fail fast when `OBSERVE_AUTH_MODE=service` but `OBSERVE_TOKEN` is empty.

### Added
- Added a deployment checklist for preventing recurrent `{"message":"Unauthorized"}` on `/api/audit-logs`.
- Added remote runbook troubleshooting steps to auto-sync `OBSERVE_TOKEN` from `~/github/xtrace/.env` and verify audit API health.

### Fixed
- Fixed a common operational misconfiguration where missing `deploy/nebula.env` or empty `OBSERVE_TOKEN` caused Audit Logs to fail intermittently after restart.

## [2026-02-14]

### Added
- Added explicit BFF xtrace auth mode with `OBSERVE_AUTH_MODE` (`service` / `internal`).
- Added `crates/nebula-bff/Dockerfile` for containerized BFF builds.
- Added optional `xtrace` service under Docker Compose profile `observe`.
- Added deployment guidance for BFF + xtrace auth strategy in docs.

### Changed
- Updated BFF xtrace proxy behavior to use explicit mode policy instead of caller-token fallback.
- Updated local startup scripts to support BFF startup and xtrace mode configuration.
- Updated `docker-compose.yml` to include BFF service and route Gateway to BFF.
- Updated deployment and README docs with dev/prod recommended auth settings.

### Fixed
- Fixed Audit Logs Unauthorized behavior by clarifying and enforcing service/internal auth configuration paths.

[Unreleased]: https://github.com/EeroEternal/nebula/compare/v1.4.0...HEAD
[1.4.0]: https://github.com/EeroEternal/nebula/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/EeroEternal/nebula/compare/v1.2.1...v1.3.0
[0.2.0]: https://github.com/EeroEternal/nebula/compare/v0.1.1...v0.2.0
