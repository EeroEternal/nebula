# UniGateway：定位与边界

> 状态：边界说明（2026-07-13，对齐 v1.3.0）  
> Gateway 已嵌入协议适配能力；本文只固定 **能用 / 不能用** 边界。排期见 [`../arch/optimization.md`](../arch/optimization.md)。  
> 相关：[`../arch/architecture.md`](../arch/architecture.md)、[`api_ownership.md`](./api_ownership.md)、[`gateway/`](./gateway/)

---

## 0. 结论先行

UniGateway（UG）对 Nebula **有使用价值，但不是 Router 替代品**。

正确用法：把 UG 当作 **Gateway 侧的协议与执行库**（解析 / 渲染 / 流式规范化 / 可选上游驱动），补齐 OpenAI / Anthropic / Responses / SSE 等接入能力。

错误用法：用 UG 的 TOML / provider pool / 库内选路，替代 `nebula-router` 基于 etcd Placement / Endpoint 的集群选路。

一句话：

**UG 管「怎么跟上游说话」；Nebula Router 管「集群里此刻该打哪台引擎」。**

---

## 1. 双方职责对照

| 维度 | UniGateway | Nebula Router / 控制面 |
|------|------------|------------------------|
| 形态 | library-first（无内置 HTTP 服务） | 独立微服务 + etcd watch |
| 权威状态 | 嵌入方配置 / `GatewayState`（常为 TOML） | etcd PlacementPlan + Endpoint |
| 选路语义 | provider pool、binding、failover、重试 | Ready / `plan_version` / 熔断 / admission / affinity |
| 协议 | OpenAI / Anthropic 解析渲染、Responses、SSE 规范化 | 基本透传引擎 HTTP |
| 生命周期 | 不负责引擎启停 | Node reconcile 启停引擎并注册 endpoint |
| 典型上游 | 多厂商 API 或固定 base_url | 本集群引擎副本（vLLM / SGLang 等） |

Nebula 数据面目标路径保持不变：

```
Client → Gateway（鉴权 / 协议 / 审计）→ Router（选 endpoint + 代理）→ Engine HTTP
```

UG 只应出现在 **Gateway 进程内**（或 Gateway 调用的库边界内），不得绕过 Router 自行做集群实例选择。

---

## 2. 价值分层

### 2.1 高价值（Gateway 内使用）

| 能力 | 落点 | 说明 |
|------|------|------|
| 协议解析与渲染 | `nebula-gateway` | OpenAI / Anthropic、`ProtocolHttpResponse` / SSE |
| Responses / tool 相关转换 | `nebula-gateway` | `/v1/responses` 等 |
| 流式规范化 | `nebula-gateway` | 统一 SSE 事件序列，便于契约测试与 abort 口径 |
| 推荐依赖面 | `unigateway-sdk`（或仅 protocol） | 优先薄依赖 |

### 2.2 中价值（借鉴，不替换控制面）

| 能力 | 建议 |
|------|------|
| 按 key 限流 / 排队 | 落在 Gateway 鉴权之后；不要进 Scheduler |
| 上游失败重试 / 降级 | 候选集仍由 Router 决定 |
| abort / disconnect 传播 | 语义归 Nebula；实现可参考 UG 流式取消 |

### 2.3 应拒绝

| 做法 | 原因 |
|------|------|
| 用 UG 替换 `nebula-router` | 缺少 etcd、`plan_version`、Draining、fencing |
| TOML / provider pool 当集群权威 | 与声明式 Placement 双权威 |
| Gateway 内嵌 UG 后再做一套实例选择 | 违反 Gateway / Router 边界 |
| Scheduler / Node 依赖 UG | 控制面与协议库耦合，无收益 |
| 为「功能全」把多厂商 failover 塞进热路径 | Nebula 扩展轴是镜像矩阵 + Engine Adapter，不是 provider 全家桶 |

---

## 3. 集成形态

UG 只嵌入 Gateway；Router 继续 etcd watch 选路。不在 Scheduler / Node 引入 UG。失败时回退 Native HTTP Passthrough。

更细的历史分阶段方案已完成；当前以边界为准，不再维护独立落地排期表。
