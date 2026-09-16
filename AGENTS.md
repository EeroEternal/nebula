# AGENTS.md — Code Agent Collaboration Specification

Lightweight entry for AI coding agents. Details live in [`docs/ai/agents/`](docs/ai/agents/) and `.agents/skills/`; **do NOT load all chapters by default**.

## Knowledge Tiering & Token Budget

| Tier | Content | Entry |
| --- | --- | --- |
| **Standing Constraints** | Inviolable rules | This file; expanded in [`docs/ai/agents/`](docs/ai/agents/) |
| **Reusable Workflows** | Procedures & gate commands | `.agents/skills/*/SKILL.md` |
| **Vendor/Env Bridges** | Toolchain / cloud VM | [`docs/dev/setup.md`](docs/dev/setup.md), [`docs/ai/cloud.md`](docs/ai/cloud.md) |
| **Domain Specs** | UI / architecture / etcd | [`docs/design.md`](docs/design.md), [`docs/arch/architecture.md`](docs/arch/architecture.md), [`docs/dev/etcd.md`](docs/dev/etcd.md) |

Hard limit **80 lines / 1200 tokens**. Near the limit: **zero-sum** (add one, remove one). Do not promote one-off mistakes; use skill [`promote-lesson`](.agents/skills/promote-lesson/SKILL.md) after ≥ 2 sessions.

## Agent Reading Map

| Task Signal | Required Reading |
| --- | --- |
| Visible page / dialog / HTML / console UI | skill [`admin-ui-change`](.agents/skills/admin-ui-change/SKILL.md) → [`docs/design.md`](docs/design.md); [`ui-entry.md`](docs/ai/agents/ui-entry.md) |
| Admin domain modules / API contract | skill [`admin-domain-resource`](.agents/skills/admin-domain-resource/SKILL.md) |
| `git stash` | skill [`git-stash-safe`](.agents/skills/git-stash-safe/SKILL.md) |
| SQL / BFF schema | skill [`add-sql-migration`](.agents/skills/add-sql-migration/SKILL.md) |
| Design docs / DDL / Mermaid | skill [`verify-design-doc`](.agents/skills/verify-design-doc/SKILL.md) |
| Release / tagging | skill [`release`](.agents/skills/release/SKILL.md) |
| Code review / PR audit | skill [`review`](.agents/skills/review/SKILL.md) |
| etcd key / prefix / lease / owner | [`docs/dev/etcd.md`](docs/dev/etcd.md), [`docs/dev/ownership.md`](docs/dev/ownership.md) |
| K8s / HAMi runtime | [`docs/dev/k8s.md`](docs/dev/k8s.md) |
| Docs under `docs/` | [`docs-layout.md`](docs/ai/agents/docs-layout.md) |
| `tokio::spawn` / scripts / exit codes | [`engineering.md`](docs/ai/agents/engineering.md) |
| Commit messages | [`commit-style.md`](docs/ai/agents/commit-style.md) |
| Push / local CI | skill [`pre-push-local-gates`](.agents/skills/pre-push-local-gates/SKILL.md) |

## Always Active

1. **No Piggybacking**: Commits/PRs must not carry unrelated changes; split via `git reset --mixed HEAD~1`.
2. **Zero Hallucination Code**: Every definition has callers; caches have a store policy; metrics track success and failure; TODOs reference issues. Docs never cite skeleton-only features.
3. **Safe Stash**: Honest names; `git diff --stat` before stash; `cargo check --tests` after pop; never stash lockfiles or build scripts.
4. **Release Guardrail**: No merge to main and no release tags without explicit human approval.
5. **UI stack**: Product UI is `frontend/`. Never ship parallel HTML/JS. Dialogs `max-h-[85vh]` + `overflow-y-auto`. No casual subtitles. Global config only on Settings.
6. **i18n**: User-visible copy via `t('key')` with matching `zh` and `en` entries in `frontend/src/lib/i18n.tsx`. Mixed languages prohibited.
7. **Sorting & Search**: Sort options state direction; search placeholders state searchable fields.
8. **etcd is the control-plane authority**: 准入三问; one write owner; Gateway/Router hot path reads `/endpoints/` only. BFF-only CRUD → Postgres. See [`docs/dev/etcd.md`](docs/dev/etcd.md).
9. **K8s is execution plane only**: do not replace etcd with the kube API; no dual-reconcile of Node vs controller. See [`docs/dev/k8s.md`](docs/dev/k8s.md).
10. **Local CI only**: Never add or revive GitHub Actions test workflows. Push gate is `./scripts/ci.sh` via skill [`pre-push-local-gates`](.agents/skills/pre-push-local-gates/SKILL.md).
11. **Plugin/middleware** for custom data-plane logic (headers, auth decoration, masking). Never hardcode tenant branches into the core pipeline.
12. **Docs layout**: `manual/` `versions/` `dev/` `arch/`; sole index `docs/README.md`; short names; `manual/` has no roadmap checkboxes. Details: [`docs-layout.md`](docs/ai/agents/docs-layout.md).

## Skills Index

Authoritative skills: `.agents/skills/`.

- [`git-stash-safe`](.agents/skills/git-stash-safe/SKILL.md) · [`add-sql-migration`](.agents/skills/add-sql-migration/SKILL.md) · [`promote-lesson`](.agents/skills/promote-lesson/SKILL.md)
- [`admin-ui-change`](.agents/skills/admin-ui-change/SKILL.md) · [`admin-domain-resource`](.agents/skills/admin-domain-resource/SKILL.md)
- [`verify-design-doc`](.agents/skills/verify-design-doc/SKILL.md) · [`pre-push-local-gates`](.agents/skills/pre-push-local-gates/SKILL.md)
- [`release`](.agents/skills/release/SKILL.md) · [`review`](.agents/skills/review/SKILL.md)
- [`api-key-lifecycle-security`](.agents/skills/api-key-lifecycle-security/SKILL.md) · [`user-attributes-settings`](.agents/skills/user-attributes-settings/SKILL.md)
