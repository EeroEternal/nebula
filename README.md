# nebula

Nebula 是 deepinfer 架构落地的 Rust 控制面（Gateway / Router / Scheduler / Node），默认 **Engine-Passthrough**：Gateway → Router → 引擎原生 HTTP（vLLM / SGLang），权威状态在 etcd。

## 快速开始

**克隆项目（推荐使用浅克隆）：**

```bash
git clone --depth 1 https://github.com/lipish/nebula.git
```

**文档索引：** [docs/README.md](docs/README.md)（`manual/` 部署手册 · `dev/` 开发文档 · `arch/` 架构）。

**安装依赖：** 参阅 [开发环境设置指南](docs/dev/setup.md) 安装所需的外部依赖（etcd、protoc 等）。

**部署与鉴权建议：** 参阅 [部署指南](docs/manual/deployment.md)。其中 BFF 访问 xtrace 推荐：
- 开发环境：`OBSERVE_AUTH_MODE=internal`
- 生产环境：`OBSERVE_AUTH_MODE=service` + `OBSERVE_TOKEN=<internal-service-token>`

## 项目结构

- `crates/nebula-common`：共享类型与热路径 JSON helpers（PlacementPlan、EndpointInfo、`x-nebula-model` 等）
- `crates/nebula-meta`：MetaStore（etcd + 内存实现，election / lease / CAS）
- `crates/nebula-router`：endpoint 选择与代理（plan_version、策略、熔断）
- `crates/nebula-gateway`：对外 OpenAI 兼容 HTTP（含 `/v1/responses` SSE）
- `crates/nebula-node`：watch placements → 启停引擎 → 注册 endpoints
- `crates/nebula-scheduler`：声明式 `/deployments/` → PlacementPlan（CAS）
- `crates/nebula-cli`：运维 CLI（load / scale / chat 等）

## 本地验证（MVP）

启动 Gateway：

```bash
cargo run -p nebula-gateway
```

Non-stream：

```bash
curl -sS http://127.0.0.1:8080/v1/responses \
  -H 'content-type: application/json' \
  -d '{"model":"stub","input":"hello"}'
```

Stream（SSE）：

```bash
curl -N http://127.0.0.1:8080/v1/responses \
  -H 'content-type: application/json' \
  -d '{"model":"stub","input":"hello","stream":true}'
```

预期：

- 每条为 `data: {"type": ... }` 的 JSON 事件（用 `type` 识别事件）。
- Responses streaming **不使用** `event:` 行。
- Responses streaming **不使用** `data: [DONE]` 哨兵。

全量单测：`cargo test --workspace`。架构与优化计划见 [docs/arch/architecture.md](docs/arch/architecture.md)、[docs/arch/optimization.md](docs/arch/optimization.md)。
