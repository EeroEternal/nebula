# 引入 UniGateway：定位、边界与落地建议

> 状态：架构指导（2026-07-11）  
> **边界仍有效；控制面 HA（选主/Drain/abort）已完成，剩余接入多副本见 optimization N1。**  
> 相关：[`../arch/architecture.md`](../arch/architecture.md)、[`../arch/optimization.md`](../arch/optimization.md)、[`api_ownership.md`](./api_ownership.md)、[`gateway/`](./gateway/)

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

## 2. 对 Nebula 的使用价值

### 2.1 高价值（建议引入）

| 能力 | 落点 | 说明 |
|------|------|------|
| 协议解析与渲染 | `nebula-gateway` | OpenAI / Anthropic 请求解析、`ProtocolHttpResponse` / SSE 渲染 |
| Responses / tool 相关转换 | `nebula-gateway` | 补齐现有 `/v1/responses` 薄弱实现 |
| 流式规范化 | `nebula-gateway` | 统一 SSE 事件序列，便于契约测试与 abort 口径 |
| 推荐依赖面 | `unigateway-sdk`（或仅 `unigateway-protocol`） | 优先薄依赖；能只用 protocol 就不要拉整棵 engine |

### 2.2 中价值（借鉴或按需抽用）

| 能力 | 建议 |
|------|------|
| 按 key 限流 / 排队 | 可对照 UG `runtime` 思路，落在 Gateway 鉴权之后；不要进 Scheduler |
| 上游失败重试 / 降级 | Gateway→Router 或 Router→Engine 的有限重试可借鉴策略，但候选集仍由 Router 决定 |
| abort / disconnect 传播 | 与 PowerLLM 吸收指导中的 Request abort 对齐；实现可参考 UG 流式取消，语义归 Nebula |
| AIMD / 反馈选路 | **不要**用 UG 替换 Router 策略；最多作算法参考 |

### 2.3 低价值 / 应拒绝

| 做法 | 原因 |
|------|------|
| 用 UG 替换 `nebula-router` | 缺少 etcd、`plan_version`、Draining、fencing 语义 |
| TOML / provider pool 当集群权威 | 与声明式 Placement 双权威，必出对账债 |
| Gateway 内嵌 UG 后再做一套实例选择 | 违反 Gateway / Router 边界（见 `../arch/optimization.md`、吸收指导 §3.7） |
| Scheduler / Node 依赖 UG | 控制面与协议库耦合，无收益 |
| 为「功能全」把多厂商 failover 塞进热路径 | Nebula 扩展轴是镜像矩阵 + Engine trait，不是 provider 全家桶 |

---

## 3. 推荐集成形态

### 3.1 目标架构（嵌入 Gateway）

```
                    ┌─────────────────────────────────────┐
  Client ──────────►│ nebula-gateway                      │
                    │  · 鉴权 / 审计 / ExecutionContext    │
                    │  · unigateway-protocol（解析/渲染）  │
                    │  · （可选）薄 host 适配，不选集群实例 │
                    └──────────────┬──────────────────────┘
                                   │ 已规范化的上游请求
                                   ▼
                            nebula-router
                                   │ 按 etcd endpoint 选路 + 代理
                                   ▼
                              Engine HTTP
```

原则：

1. **HTTP / 鉴权 / 管理 API** 仍由 Nebula Gateway 拥有。
2. **集群选路** 只在 Router；Gateway 把请求交给 Router（或经 Router 暴露的代理面），不把「哪个 replica」交给 UG pool。
3. UG 若需要 `PoolHost` / dispatch，嵌入方应构造 **已由 Router 选定的单一上游**，或根本不走 UG 选路 API，只复用 protocol 层。

### 3.2 依赖策略

- 优先：`unigateway-sdk` 且尽量只用 `protocol` 相关导出。
- 次选：直接依赖 `unigateway-protocol`。
- 谨慎：引入 `unigateway-core` / 完整 `UniGatewayEngine`——仅当 Gateway 需要其驱动与流式执行，且明确 **不** 用其做 etcd 级选路。
- 版本：与 crates.io / 对照仓库对齐同一 release line；避免混用 1.x / 2.x API（当前工作树曾出现 `unigateway` 1.7 与代码不匹配导致 Gateway 编译失败，见 `../arch/optimization.md`）。

### 3.3 与现有代码的关系

- `nebula-router`：**保留并强化**（plan_version、Draining、熔断、admission、契约测试）。
- `nebula-gateway`：协议与接入增强的主战场；引入 UG 后仍须保持「可编译 + 请求最终经 Router」。
- 若历史上存在「Gateway 内 UG pool 同步 etcd」实验路径：应收敛为「etcd → Router 缓存；Gateway 不持有第二套选路权威」，或删除未闭合集成，先恢复 workspace 编译。

---

## 4. 分阶段落地

### Phase A：边界冻结 + 编译闭环（先于功能）

验收：

- 文档与代码一致：UG ≠ Router。
- `cargo check -p nebula-gateway` / workspace 可通过。
- Gateway/Router **已移除**未使用的 `unigateway` 依赖；需要协议能力时再按 `unigateway-sdk` / protocol-only 引入。
- 无「绕过 Router 的 UG 直连引擎」默认路径（调试旁路须显式开关且不进生产 compose）。

### Phase B：协议能力（产品可感知）

状态（2026-07-10）：**已落地初版**

- Gateway 依赖 `unigateway-protocol` / `unigateway-core` 2.6（protocol 解析，不引入 Engine 选路）。
- `/v1/responses`：UG 解析 → 转 OpenAI chat → **Router** `/v1/chat/completions` → Responses SSE/JSON。
- `/v1/messages`：Anthropic Messages → UG 转 OpenAI chat → Router → Anthropic 形响应（含流式）。
- 契约单测：`protocol_adapt`（Anthropic/Responses 转换）+ Responses SSE 序号。

验收：

- `/v1/chat/completions` 流式事件序列稳定，有契约测试。
- `/v1/responses` 达到可用（经 Router，非本地假引擎占位）。
- Anthropic 兼容入口 `/v1/messages`：转换发生在 Gateway，上游仍走 Router。

### Phase C：生命周期对齐（与可靠性 P0 协同）

验收：

- client disconnect / abort 经 Gateway 取消下游到 Router，再断开引擎连接。
- abort / 主动 drain 的 metrics 口径与 SLO 文档一致（不算错误预算，除非另有定义）。

不在本文件范围：etcd 三节点与接入多副本——见 [`../arch/optimization.md`](../arch/optimization.md) N1 与 [`ha/ha_roadmap.md`](./ha/ha_roadmap.md)。

---

## 5. 行为验收（对齐「行为」而非「类名」）

| 用户/运维可感知行为 | Nebula 目标 | UG 角色 |
|--------------------|-------------|---------|
| 发 OpenAI chat，拿稳定 SSE | Gateway 解析/渲染正确 | protocol 层 |
| 发 Responses / 工具调用 | Gateway 转换后经 Router 到引擎 | protocol / 有限 conversion |
| 缩容不打断进行中请求 | Router Draining + Node drain | **不参与** |
| 杀 scheduler 后旧主不能写 | etcd election + fencing | **不参与** |
| 取消生成 | Gateway→Router→断上游 | 可参考流式取消实现 |
| 多引擎扩展 | 镜像 + Engine trait | **不参与** provider 全家桶 |

---

## 6. 反模式清单

- 为了少跳一跳，让 Gateway+UG 直连引擎，架空 Router。
- 把 etcd endpoint 同步进 UG `ProviderPool` 并在 UG 内做 least-pending / plan_version（第二套 Router）。
- License / RBAC / 用户体系塞进 UG 或 Scheduler。
- 用 UG 的 TOML 热更新替代 Placement reconcile。
- 文档写「只嵌 protocol」，代码却 `dispatch` 全量选路。
- API 版本漂移导致 Gateway 长期无法通过 CI。

---

## 7. 对 UniGateway 上游的可选诉求（非阻塞）

若 UG 继续演进以更好服务 Nebula 这类 embedder，优先低成本能力（不要求 Nebula 阻塞等待）：

- 稳定、版本化的 **protocol-only** 使用面（少牵 core）。
- Hooks 对请求/响应的受控可变引用（审计 Header、上下文注入），避免动态 Middleware 链。
- 文档中明确「集群选路外置」模式：嵌入方先选定单一 upstream，再交给执行层。

这些是对 UG 仓库的增强建议，**不是** Nebula 引入 UG 的前置条件。Nebula 侧以「只用现有 protocol / sdk 能力」即可启动 Phase B。

---

## 8. 文档维护

- 本文件描述 **是否引入、引入到哪一层、验收什么**。
- Gateway 具体任务拆解仍放在 `docs/dev/gateway/`。
- 若集成策略变更（例如产品决定让 UG 承担更多执行），必须同步更新本文与 `../arch/optimization.md` 中的边界段落，避免再次出现双路由层。

---

## 9. 一句话收束

Nebula 引入 UniGateway，是为了站在巨人肩膀上补齐 **接入协议完备度**；集群可靠性与选路仍走 etcd + Router + Node。不要把 UG 做成「Rust 版第二控制面」，也不要因为有了 UG 就削弱 Router。
