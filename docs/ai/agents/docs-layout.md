# Documentation Layout & Lifecycle

This document describes the structure and lifecycle of the `docs/` tree. Product docs stay in Nebula’s four folders; console-kit agent/UI specs live beside them.

## Directory Structure

| Path | Audience | Contents |
| --- | --- | --- |
| `docs/manual/` | 产品、运维 | 当前版本能力、怎么用、出问题怎么办。不含 P0–P6、roadmap 勾选、Phase 编号。 |
| `docs/versions/` | 全员 | Release notes：`v1.8.0.md` 等。 |
| `docs/dev/` | 研发 | 开发环境、计划、API/etcd 边界、契约、UniGateway。 |
| `docs/arch/` | 研发 | 架构与 roadmap。 |
| `docs/design.md` + `docs/design/` | 研发 / Agent | UI 规范（console-kit）。按需加载，见 skill `admin-ui-change`。 |
| `docs/ai/agents/` | Agent | 工程纪律、commit、循环章程。 |
| `docs/ai/cloud.md` | Agent | 云端/本机环境桥（toolchain、etcd 路径）。 |
| `CHANGELOG.md`（仓库根） | 全员 | Keep a Changelog + SemVer。`[Unreleased]` → 发版时转入 `docs/versions/`。 |

- **唯一索引：** [`docs/README.md`](../../README.md)。子目录不再放 README。
- **短文件名：** 主题名，不要后缀。`plan.md` 不是 `product_plan.md`；`versions/v1.3.0.md` 不是 `release_notes_v1.3.0.md`。
- **短文：** 一屏事实优于长文。写清当前行为、边界、代码链接；完成后删除清单、臆测计划和历史分析。落地后把 `*_integration.md` / `*_plan.md` 改成主题名。
- **禁止写入 `docs/`：** 环境相关 runbook、内网 IP、密钥。
- **`manual/` 读者可能是非开发岗：** 先写「是什么、能干什么、出问题怎么办」；术语要解释；环境变量、PromQL 放到「实施参考」。
- **脚本：** 独立测试/调试脚本放 `scripts/`；生产二进制与服务脚本放 `bin/`。
- **临时数据：** 不要把 `default.etcd` 一类文件放仓库根；用 `/tmp`。

当前版本：`Cargo.toml` + `git log` + 根目录 `CHANGELOG.md` + `docs/versions/`。不要另写一份项目状态快照。

## Document Lifecycle Discipline

1. **No Phantom Capabilities**: Never document skeleton-only or hypothetical features as ready.
2. **Deterministic Verification**: SQL and code snippets in docs must be executable. Skill [`verify-design-doc`](../../../.agents/skills/verify-design-doc/SKILL.md).
3. **Landed work**: write the conclusion into `CHANGELOG.md` (and `docs/versions/` on a cut), then delete the stale draft. Do not leave “done” proposals as the source of truth.
