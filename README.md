# nebula

Nebula 是 deepinfer 架构落地的 Rust-Native 新项目目录（控制面/路由/调度/agent）。

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

- `crates/nebula-common`：共享类型（ExecutionContext、EndpointInfo/Stats 等）
- `crates/nebula-meta`：MetaStore 抽象（首期含内存实现，后续接入 etcd）
- `crates/nebula-router`：请求路由（least-connections + session affinity 预埋）
- `crates/nebula-gateway`：对外 HTTP（后续实现 OpenAI-compatible，含 /v1/responses 1:1 streaming）
- `crates/nebula-node`：节点侧 reconcile（watch placements → 管理引擎进程/注册 endpoints）
- `crates/nebula-scheduler`：放置与副本规划（PlacementPlan）
- `crates/nebula-cli`：统一入口（后续整合启动参数）

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
