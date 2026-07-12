# API Ownership（Gateway / BFF / Router）

> 状态：已落地约定（2026-07-11）  
> 相关：[`../arch/architecture.md`](../arch/architecture.md)、[`../arch/optimization.md`](../arch/optimization.md)、[`unigateway_integration.md`](./unigateway_integration.md)

每个对外 API 必须有**唯一 owner**。推理热路径保持 `Client → Gateway → Router → Engine`。

## Owner 表

| 能力 | Owner | 路径约定 | 备注 |
|------|-------|----------|------|
| OpenAI 兼容推理 | Gateway 入口 + Router 选路 | Gateway `/v1/chat/completions` 等 → Router 同名 → Engine | Gateway 不做实例选择 |
| `/v1/responses` | Gateway（UG protocol 解析） | Gateway → Router `/v1/chat/completions` | 响应再映射为 Responses 形 |
| Anthropic Messages | Gateway（UG protocol） | Gateway `/v1/messages` → Router chat | 不绕过 Router |
| Router 健康/指标 | Router | `/healthz`, `/metrics` | 无 admin |
| 控制台会话 / SSO | BFF | `/api/auth/*` | Postgres；Scheduler 不碰 |
| 声明式模型管理 | BFF | `/api/v2/models/*` | 写 etcd Deployment/Spec |
| 观测面板聚合 | BFF | `/api/v2/observability/*` | |
| 运维只读快照 | BFF 优先 | `/api/overview` 等 | Gateway 遗留 `/v1/admin/cluster/status` 只读保留，新功能不扩 |
| 镜像注册 | BFF | `/api/images/*` 与 `/api/v2/...` | **禁止** Gateway 再扩 images 写 API |
| Endpoint drain | Gateway 或 BFF（择一） | 当前 Gateway `/v1/admin/endpoints/drain` | 新客户端走 BFF 时再迁；勿双写语义 |
| Scheduler 选主探针 | Scheduler | `/healthz` leader=200 / follower=503 | LB 门禁 |

## 禁止

- Gateway 绕过 Router 直连引擎（调试旁路须显式开关且默认关）。
- Gateway 与 BFF 对同一资源各写一套 etcd 更新逻辑（images / load / scale）。
- Scheduler / Node 引入用户鉴权或 Postgres。
- UniGateway 承担集群选路（见 `unigateway_integration.md`）。

## 迁移方向（不阻塞 Wave 1）

1. 新控制台写路径只加 BFF `/api/v2/*`。
2. Gateway `/v1/admin/*` 写接口标记 deprecated，文档指向 BFF。
3. Gateway 保留：推理代理、鉴权 token、少量运维只读与 drain（直至 BFF 接管）。
