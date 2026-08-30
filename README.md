# nebula

Nebula 是本地 / 专有化推理环境的跨引擎控制面（Gateway / Router / Scheduler / Node / BFF）：默认 **Engine-Passthrough**（Gateway → Router → vLLM / SGLang 原生 HTTP），权威状态在 etcd。当前发布 **v1.8.0**。

## 快速开始

**克隆：**

```bash
git clone --depth 1 https://github.com/EeroEternal/nebula.git
```

**文档索引：** [docs/README.md](docs/README.md)（`manual/` 产品与运维 · `dev/` 工程 · `arch/` 架构）。

**安装依赖：** [开发环境设置](docs/dev/setup.md)（etcd、protoc、Rust 1.92 等）。

**一键本地栈：** 复制 `deploy/nebula.env.example` → `deploy/nebula.env`，设置 `START_BFF=1`、`OBSERVE_AUTH_MODE=internal`，构建后执行 `./bin/nebula-up.sh`。常用端口：Gateway `8081`、Router `18081`、BFF `18090`、前端 `5173`。

**鉴权：** 推理面 `NEBULA_AUTH_TOKENS=dev-token:admin`（或 `token:role:tenant_id`）+ `Authorization: Bearer …`；开发可 `NEBULA_AUTH_DISABLED=1`。多租户准入可选 `NEBULA_MULTI_TENANT=1`。详见 [部署指南](docs/manual/deployment.md) 与 [v1.8.0 Release Notes](docs/versions/v1.8.0.md)。

## 项目结构

- `crates/nebula-common`：共享契约（Placement、Endpoint、Capability、Compat、SLO、Benchmark、Tenant、ExecutionContext 等）
- `crates/nebula-meta`：MetaStore（etcd + 内存；election / lease / CAS）
- `crates/nebula-gateway`：OpenAI 兼容 HTTP；鉴权、审计、租户准入、透传 Router
- `crates/nebula-router`：选路与代理（plan_version、策略、熔断）
- `crates/nebula-scheduler`：`/deployments/` → PlacementPlan（CAS）；兼容/平台过滤
- `crates/nebula-node`：watch placement → 启停引擎 → 注册 endpoints / capabilities
- `crates/nebula-bff`：控制台 API（模型、治理、Benchmark、租户）
- `crates/nebula-cli`：运维 CLI
- `frontend/`：控制台（含 `/governance`）
- `scripts/benchmark/`：标准 workload 与 runner

## 能力概览（v1.4.0）

声明式副本生命周期与路由；引擎能力与兼容矩阵；Model SLO / 诊断；Benchmark 推荐与 Canary；可选多租户配额与成本归因。Serving Cell（CellIngress）已下线。真机 GPU e2e 与多租户压测暂缓，见 [roadmap.md](docs/arch/roadmap.md)。

全量单测：`cargo test --workspace`。架构见 [docs/arch/architecture.md](docs/arch/architecture.md)。

## Replit 开发工作流

Replit 的 `Project` 工作流会并行启动控制台前端和 BFF 后端：

- 前端：Vite 绑定 `0.0.0.0:5000`，预览入口为根路径；
- 后端：`nebula-bff` 绑定 `0.0.0.0:18090`，控制台健康检查为 `/api/healthz`；
- etcd：首次启动时由 `bin/replit-backend.sh` 在 `/tmp/nebula-replit-etcd` 启动本地实例；
- PostgreSQL：使用 Replit 提供的 `DATABASE_URL`，脚本会创建独立的 `nebula` 数据库，不会把凭据写入仓库。

点击 **Run** 前，请确认工作区已提供 `DATABASE_URL`。前端依赖可在 `frontend/` 执行 `npm ci`，Rust BFF 可用
`cargo build -p nebula-bff --release` 预构建。控制台不需要 xtrace 凭据，默认使用
`OBSERVE_AUTH_MODE=internal`。真实 vLLM 推理仍需要带 GPU 和模型文件的外部运行环境；Replit 工作流只负责控制台与控制面后端。
Replit 通过 `rustup` 和仓库中的 `rust-toolchain.toml` 使用 Rust 1.92，因此可直接运行
`cargo build --workspace --release` 构建完整 workspace（包括 Gateway、Router、Scheduler 和 Node）。
