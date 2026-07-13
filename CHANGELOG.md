# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog.

## [Unreleased]

## [1.3.0] - 2026-07-13

Product-alignment release covering P0–P6 from `docs/dev/plan.md`. Real GPU / native Gateway e2e and multi-tenant load tests remain deferred.

### Added
- **P0 Observability trust:** `kv_cache_usage` semantics, scrape health metrics/fixtures, Gateway API `data_source: router`, O8 SLO/alert runbook samples.
- **P1 Engine Capability / Adapter:** capability contracts, static tables, dialect CLI, runtime discovery persisted to etcd `/capabilities/`, engine version support checks.
- **P2 Serving Cell:** etcd `/cells/` CRUD, BFF observe + OpenAI probe, Router whole-ingress routing without Nebula retry/circuit amplification, console `/cells`.
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

[Unreleased]: https://github.com/EeroEternal/nebula/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/EeroEternal/nebula/compare/v1.2.1...v1.3.0
[0.2.0]: https://github.com/EeroEternal/nebula/compare/v0.1.1...v0.2.0
