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
./scripts/ci.sh                          # cargo test --workspace + OpenAPI
cargo fmt --check
cargo clippy --all-targets -- -D warnings
bash scripts/check_ui_stack.sh
bash scripts/check_admin_nav.sh
(cd frontend && npx tsc -b --noEmit && npm run lint)
```

可选冒烟（需 docker/etcd + release 构建）：`RUN_SMOKE=1 ./scripts/ci.sh`。

UI 规范改动需确保符合 `docs/design.md`；发版与打 Tag 前，转入 skill
[`release`](../release/SKILL.md) 执行完整发版流程（三查 + 人工批准硬停）。

## 适用范围与纪律 (Scope & Discipline)
- 开发过程中的中间 commit 允许临时不跑全量，但 **push 前最后一次提交必须全绿**。
- CI 如果意外挂了：禁止盲猜盲改，必须在本地先复现该失败的等价命令，本地确认修复通过后再推送。
