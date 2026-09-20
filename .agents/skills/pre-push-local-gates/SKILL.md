---
name: pre-push-local-gates
description: Push 前必须在本地跑满与 CI 等效的门禁(scripts/ci.sh + fmt/clippy + frontend). 禁止把远端 CI 当本地沙盒。Nebula 不使用 GitHub Actions。Use before every git push.
---

# Pre-push local gates（推送前本地门禁）

## 核心痛点与禁止项 (Symptom / Misjudgment)
严禁把 CI 当本地沙盒：推送后才发现 lint 报错、rustfmt 未对齐、编译报警、测试失败，
形成「推 → 挂 → 本地修 → 再推」的低效循环。Nebula **禁止**新增或复活 GitHub Actions 测试 workflow；门禁是本地 `./scripts/ci.sh`。

## 本地门禁执行标准 (Local Gate Checklist)
在执行 `git push` 或提 PR 之前，以下命令必须**全部在本地通过**：

```bash
./scripts/ci.sh   # fmt + clippy + test + OpenAPI + UI/nav + frontend(tsc/lint/build)
```

`./scripts/ci.sh` 已是全量门禁（等价于已删除的 GitHub Actions workflow）；改了 `frontend/package.json` 后先 `npm install`，否则 `node_modules` 陈旧、tsc 报 `Cannot find module`。

可选冒烟（需 docker/etcd + release 构建）：`RUN_SMOKE=1 ./scripts/ci.sh`。

UI 规范改动需确保符合 `docs/design.md`；发版与打 Tag 前，转入 skill
[`release`](../release/SKILL.md) 执行完整发版流程（三查 + 人工批准硬停）。

## 已知陷阱（v1.9.1 复盘）

- **别只留到 push 才首次跑**：release / 大功能 commit 也要跑。v1.9.0 未跑，main 上遗留 57 条 clippy、8 条前端 ESLint、未对齐 fmt，只能靠下一个 patch 清债。
- **前端依赖漂移**：改了 `frontend/package.json` 后先 `npm install`，否则 `node_modules` 陈旧、`tsc` 报 `Cannot find module 'react-day-picker' / 'cmdk' / '@radix-ui/*'`。
- **`clippy -D warnings` 对 `#[cfg(test)]` 的 `dead_code` 误报**：只被 `mod tests`（经 `use super::*` glob）使用的函数会在非 test target 被报 dead。正解是加 `#[cfg(test)]` 或带原因的 `#[allow(dead_code)]`（见 [`engineering.md`](../../../docs/ai/agents/engineering.md) §一.3）；直接删会让 `cargo test` 编译失败。
- **`cargo clippy --fix` 会误删测试专用导入**（如 `serde_json::json`）：把它移进 `mod tests`，不要恢复顶层导入；`--fix` 后必须重跑 `./scripts/ci.sh`。
- **加 `#[allow]` 后重跑 `cargo fmt`**：`#[allow(...)] // 注释` 会被 rustfmt 折行，否则 `cargo fmt --check` 仍红。

## 适用范围与纪律 (Scope & Discipline)
- 开发过程中的中间 commit 允许临时不跑全量，但 **release / 大功能 commit 与 push 前最后一次提交必须全绿**；不要等到 push 才首次跑门禁。
- CI 如果意外挂了：禁止盲猜盲改，必须在本地先复现该失败的等价命令，本地确认修复通过后再推送。
