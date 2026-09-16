---
name: release
description: 发版（打 tag）完整 promoter 流程：本地门禁全量重跑、发版三查（版本号/changelog/文档生命周期）、人工批准硬停、tag 部署与验证。Use before cutting any vX.Y.Z tag or running a production deploy.
---

# Release Promoter Process（发版与打 Tag 流程规范）

## 适用场景
在执行任何 `git tag v*.*.*` 打标、合并 release 分支或触发生产部署之前，必须严格遵守本规范。

---

## 核心铁律（最高站立约束）
**未经人类用户明确书面批准，AI Agent 绝对不得执行 `git tag` 或合并发布分支。**

---

## Changelog（强制）

Canonical file: [`CHANGELOG.md`](../../../CHANGELOG.md) at repo root (Keep a Changelog + SemVer).
On a version cut, also add [`docs/versions/vX.Y.Z.md`](../../../docs/versions/). There is no `docs/changelog.md`.

**While landing work** (not only at tag time): notable features, fixes, and breaking changes go under `## [Unreleased]` in the matching `### Added` / `### Changed` / `### Fixed` / `### Removed` subsection.

**When cutting `vX.Y.Z`:**

1. Rename `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD` (today’s date).
2. Insert a new empty `## [Unreleased]` above it.
3. Drop empty subsections. Do not ship a version whose only notes are `misc` / commit subjects / “various improvements”.

Each bullet must be detailed enough to brief a user:

| Required | Example |
| --- | --- |
| What changed | Auth register is three steps (email → code → password) |
| Who / where | Admin `/register`; `Mailer::send_verification_code` |
| Breaking? | Breaking: `OPENHUB_*` mail env aliases removed; use `CF_*` / `MAIL_*` |

Reject: `- feat: auth`, `- update docs`, `- fix bug`.

---

## 发版标准 4 步走 (Step-by-Step)

### 第 1 步：本地门禁全量重跑 (Local Gate Full Run)
确保工作区干净且全量门禁 100% 通过：
```bash
git status
./scripts/ci.sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
bash scripts/check_ui_stack.sh
bash scripts/check_admin_nav.sh
(cd frontend && npm run build)
```

### 第 2 步：发版三查 (Three-Point Verification)
1. **版本号一致性**：确认 workspace `Cargo.toml` 中的版本号已正确自增。
2. **Changelog**：根目录 `CHANGELOG.md` 已把 `[Unreleased]` 转正为 `[X.Y.Z] - <date>`，并新增 `docs/versions/vX.Y.Z.md`；条目可独立阅读，破坏性变更单独写明。
3. **敏感信息与构建物扫描**：确认无私有密钥、`.env`、临时调试日志或未编译产物被包含。

### 第 3 步：人工批准硬停 (Human Approval Hard Stop)
向用户输出完整的发版摘要（拟定 Tag、Commit Hash、**changelog 全文或本版全部条目**），**明确请求人类批准**。不得只贴 commit list 代替 changelog。

### 第 4 步：打 Tag 并验证 (Tag & Verification)
获得人类明确批准后，执行打标并推送到远端：
```bash
git tag -a vX.Y.Z -m "release: vX.Y.Z"
git push origin vX.Y.Z
```
