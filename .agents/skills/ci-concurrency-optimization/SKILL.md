---
name: ci-concurrency-optimization
description: GitHub Actions CI 并发优化、变更感知过滤与缓存防逐出规范。Use when configuring, debugging, or optimizing CI pipelines for Rust/Admin full-stack AI projects.
---

# CI 并发优化与缓存调度规范 (CI Concurrency & Cache Optimization)

**Nebula standing constraint:** do **not** add or revive GitHub Actions test workflows. Local gate is `./scripts/ci.sh`. This skill is retained as reference only; applying it means editing `scripts/ci.sh` / local runners, never `.github/workflows/ci.yml`.

本规范源自生产级 CI 体系的工程化沉淀（并发取消、变更感知、缓存防逐出）。

---

## 核心设计支柱 (Core Architectural Pillars)

### 1. 抢占式并发取消 (Preemptive Concurrency Cancellation)
在高频提交或 PR 迭代场景下，旧 commit 的 CI 仍在运行会挤占 Runner 队列并造成无谓等待。
必须配置 workflow 级并发控制，使同分支新 push 自动取消前序运行：

```yaml
concurrency:
  group: ci-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

### 2. 变更感知与精准分流 (Path-Based Change Detection)
并非所有提交都需要跑满全量测试。通过 GitHub API 比对 PR / push 改动的文件清单，进行条件分流：
- **纯文档/规范/Skill 变更**（如 `docs/`、`.agents/`、`*.md`）：直接跳过重型编译与构建。
- **纯前端 Admin 变更**（如 `frontend/`）：仅触发前端校验矩阵，跳过耗时数十分钟的 Rust 集成测试。
- **纯后端内部模块**（如算法、协议、路由内核）：执行 Rust 测试矩阵，跳过 Playwright E2E 浏览器渲染。

### 3. Rust 统一共享缓存 (Cache Quota Anti-Thrashing)
GitHub 每个仓库的 Actions Cache 存在硬性上限（默认 10GB）。当测试矩阵切分为 5~8 个并发 Job 时，若各自生成独立缓存 key，会导致单次构建产生多个几 GB 的缓存包，直接**击穿配额并互相淘汰缓存**。
- **强制规则**：使用 `Swatinem/rust-cache@v2` 时，所有并发 Rust 编译 Job 必须显式指定相同的 `shared-key`：
  ```yaml
  - uses: Swatinem/rust-cache@v2
    with:
      shared-key: ci-rust
  ```
- **禁用增量编译**：CI 环境中设置 `CARGO_INCREMENTAL: "0"`，避免未命中缓存时的磁盘与时间膨胀。

### 4. 领域分片测试 (Domain-Level Test Sharding)
当集成测试增长到数十个文件时，禁止使用单一单线程 `cargo test`。必须按领域模块或并发分片运行：
- **Sharding 分片范式**：
  - `rust-lib`：纯内存单元测试。
  - `rust-intg-protocol`：网络协议、配置与发现。
  - `rust-intg-auth`：鉴权、配额与治理。
  - `rust-intg-billing`：计费、价目与路由。
  - `e2e`：按 Playwright spec 文件利用取模分片（`matrix.shard: [0, 1]`）双轨并行。

---

## 检查清单 (Verification Checklist)

配置或调整 CI 流水线时，核对以下项目：
- [ ] 根部已设置 `concurrency.cancel-in-progress: true`；
- [ ] 包含前置 `changes` job，正确导出 `code` / `admin` 等判断标志；
- [ ] 所有 Rust 构建步骤包含 `CARGO_INCREMENTAL: "0"` 与统一的 `shared-key`；
- [ ] 前端构建配置了独立的 `package-lock.json` hash 缓存；
- [ ] 前后端门禁步骤与本地 `pre-push-local-gates` 校验命令保持 100% 对齐。
