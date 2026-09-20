# 控制面无关的数据面适配契约

> **文档性质**：设计草案（**`CONTROL_PLANE` 抽象与 adapter crate 均未实现**）。  
> **已落地但不属本文范围**：BFF（`:18090`）自 v1.9.0 起常驻 PowerLLM 控制台协议兼容层（`crates/nebula-bff/src/powerllm_compat.rs`）——那是**控制台南向适配**，不是这里的 `CONTROL_PLANE` 适配器，也不在 Gateway/Router 热路径。  
> **归属**：`docs/arch/` · 数据面接入边界，不改 L0 主轴。  
> **对照**：[`architecture.md`](./architecture.md) · [`data-plane-perf-optimization.md`](./data-plane-perf-optimization.md) · [`../dev/integration.md`](../dev/integration.md) · [`../dev/contracts.md`](../dev/contracts.md) · [`../dev/etcd.md`](../dev/etcd.md) · [`../dev/k8s.md`](../dev/k8s.md)

**一句话：** Gateway / Router 热路径只消费**规范化**的 Endpoint、鉴权判定与可选准入结果；PowerLLM / NVIDIA Dynamo / llm-d / 自研发现都停在可选适配器（crate 或 sidecar）之后。默认 `CONTROL_PLANE=none`，等于今天的 token + etcd `/endpoints/`，可以单独跑。

---

## 1. 定位

Nebula 是推理**控制面 + 接入面**，不是某一家推理平台的内嵌前端。必须能：

| 部署 | 含义 |
|------|------|
| **单独跑** | 无外部控制面；Node 写 etcd，Router 选路，引擎 OpenAI 透传 |
| **可选接入** | 发现 / 鉴权 / 准入来自 PowerLLM、Dynamo、llm-d 或自研，经适配器归一 |
| **不绑定** | 核心 crate 不依赖上述任一产品；热路径零平台 `if` |

本文契约是 **控制面 → 数据面** 的边界，与 L1 引擎协议适配（[`../dev/contracts.md`](../dev/contracts.md) 的 Capability / UniGateway）正交：

| 适配对象 | 解决什么 | 热路径是否看见平台名 |
|----------|----------|----------------------|
| **引擎 Adapter**（已有） | vLLM / SGLang 的 `/v1`、scrape、Capability | 否；只见 OpenAI 与 `EndpointStats` |
| **控制面 Adapter**（本文） | 谁发现副本、谁鉴权、谁做配额 | 否；只见规范化记录 |

vLLM / SGLang（及各自的 production-stack router、Model Gateway）是 **UpstreamEngine / 发现来源**，不是 `CONTROL_PLANE` 枚举值。

---

## 2. 问题

各平台都能把请求打到 OpenAI `/v1/*` 引擎，但**发现、角色、打分、鉴权**叠在不同位置：

- 有的把 KV 感知塞进 Router，有的放在独立 EPP，有的要外部 cache 信号。
- 有的鉴权是一等公民，有的根本不做。
- Prefill / Decode 有的用角色注册，有的用 label，有的用 `POST /workers`。

若把 Dynamo Frontend、llm-d EPP 或 PowerLLM Python 路径**叉进** `nebula-gateway` / `nebula-router` 请求路径，会破坏：

- [`architecture.md`](./architecture.md) 的 Engine-Passthrough 与「一种状态一个 Owner」；
- [`data-plane-perf-optimization.md`](./data-plane-perf-optimization.md) 的单跳 / 零拷贝目标（热路径不能再解析平台专有事件）；
- [`../dev/k8s.md`](../dev/k8s.md) 的「Router 不靠 kube EndpointSlice 选路」（`CONTROL_PLANE=none` 仍只认 etcd `/endpoints/`）。

需要一层**窄契约**：只归一「有哪些可打的上游」和「这个请求能不能进」，把 KV 打分、PD 合并、KV 传输留在引擎或外部调度器。

---

## 3. 平台分析

下表只抽取对 Nebula 数据面有约束的事实，不复述各项目全功能。

| 维度 | NVIDIA Dynamo | llm-d | vLLM + production-stack router | SGLang + Model Gateway |
|------|---------------|-------|--------------------------------|------------------------|
| **入口** | Frontend：OpenAI 兼容 HTTP | Gateway / Envoy；与 EPP 分离（ext-proc） | 引擎已是 OpenAI `/v1/*`（Nebula 已透传）；router 是可选前缀 | 引擎 OpenAI `/v1`；Gateway 可合并 PD 流 |
| **选路核心** | Router：KV-aware；disagg 时 prefill → decode | EPP：Filter → Score → Pick（KV、prefix、queue、latency、LoRA） | `roundrobin` / `session` / `prefixaware` / `kvaware` / `disaggregated_prefill*` | 策略如 `cache_aware`；PD 由 Gateway 拼流 |
| **发现** | Worker 注册；etcd **或** K8s CRD / EndpointSlices | `InferencePool` = 候选集合 | 服务发现 + worker 标签 | 动态 `POST /workers` |
| **角色模型** | 注册 **model + role**（Prefill / Decode） | Pool 内候选；角色不是独立控制面 | disagg 角色靠 **labels** | `worker_type`：`prefill` \| `decode` \| `regular`；可选 `bootstrap_port` |
| **鉴权** | **非核心**（发现 + 路由 + KV 事件才是） | **Gateway filters**（与 EPP 分离） | 非引擎核心 | 非引擎核心 |
| **准入 / 流控** | 非核心 | 与 EPP **分离**（admission / flow-control 另层） | 视部署 | 视部署 |
| **KV / cache 信号** | KV 事件总线（供 Router） | EPP 打分插件消费 | `kvaware` 需**外部信号**（如 LMCache） | cache 指标 / 前缀亲和 |
| **KV 传输** | 引擎间（如 NIXL），不经 Frontend 搬块 | 引擎间 | 引擎间 | 引擎间（bootstrap 握手） |
| **可坐在谁后面** | 可挂 GAIE Endpoint Picker 之后 | 自身即 GAIE 风格 | 可被任意 OpenAI 入口反代 | 可被任意 OpenAI 入口反代 |
| **对 Nebula 的启示** | 要 **list+watch** 的目录 + `role` + 可选信号；不要叉 Frontend | **Auth ≠ Policy ≠ Score**；EPP 不进热路径 | 引擎=UpstreamEngine；router 逻辑可对标，但信号走扩展口 | `hints.bootstrap_port`；**不**在 Nebula 合并 PD 流 |

**抽取结论：**

1. 所有平台最终都落到「一组可 HTTP 访问的 worker + 模型名 + 可选角色」。这是唯一必须通用的核心。
2. 鉴权与配额在各项目中可有可无、且与 KV 打分分离——适配器必须可空，且 **Policy 禁止承载 KV score**。
3. KV / 队列 / 前缀是**加分信号**，不是发现契约的一部分；缺省时选路退化为今日的 LeastPending / 熔断。
4. PD 的 bootstrap / transfer 是**提示**，传输仍在引擎之间。

---

## 4. 契约裁决

### 4.1 核心适配器

| # | Trait | 必要性 | 职责 | 明确不职责 |
|---|--------|--------|------|------------|
| 1 | **AuthProvider** | 通用但 **OPTIONAL** | 把入站请求变成 `AuthDecision`（放行 / 拒绝 / principal / tenant / scopes） | 选路、KV 打分、写 etcd |
| 2 | **EndpointDirectory** | **CORE**，高度通用 | `list` + `watch`；产出规范化 Endpoint | 启停引擎、Placement CAS、协议翻译 |
| 3 | **PolicySource** | OPTIONAL | 准入 / 配额 / 并发（admit / deny + reason） | **禁止**嵌入 KV / prefix / queue **打分** |

`CONTROL_PLANE=none` **可以没有**外部 `AuthProvider`：沿用现有 `NEBULA_AUTH_TOKENS` / `platform_api_keys` 与可选 `TenantAdmission`（见 §9）。「可选」指可插拔，不是「standalone 无鉴权」。

### 4.2 扩展

| # | Trait | 必要性 | 职责 |
|---|--------|--------|------|
| 4 | **EndpointSignals** | OPTIONAL | 队列深度、KV/cache 利用率、前缀提示、inflight；供**未来**选路加权。适配器可整段 no-op |

Router 已 watch etcd `/stats/`（`EndpointStats`：`pending_requests`、`kv_cache_usage`、`prefix_cache_hit_rate`）。`EndpointSignals` 是同一语义的**归一入口**，不是第二套权威。默认实现继续读 `/stats/`；Dynamo KV 事件 / LMCache / EPP 指标经适配器写入同一形状，或 no-op。

### 4.3 热路径只认这三件事

```text
请求
  │
  ├─ AuthProvider?  → AuthDecision        （无适配器 = 今日 token）
  ├─ PolicySource?  → Admit | Deny        （无适配器 = 今日租户配额或直接过）
  └─ EndpointDirectory（内存快照）
          │
          ▼
   Router.route(model, hints, signals?)
          │
          ▼
   HTTP Passthrough → endpoint.base_url + /v1/*
```

**零平台分支：** `nebula-gateway` / `nebula-router` 的请求处理禁止 `if dynamo` / `if llm-d` / `if powerllm`。平台差异只存在于适配器实现与启动装配。

---

## 5. 规范化 Endpoint（草案 v0）

`EndpointDirectory` 每条记录至少包含：id、base_url、models[]、role、labels、可选 bootstrap/transfer hints、ready/health。

```json
{
  "schema_version": 0,
  "id": "qwen3-prod/0",
  "base_url": "http://10.0.12.8:8000",
  "models": ["Qwen/Qwen3-8B", "qwen3-prod"],
  "role": "unified",
  "labels": {
    "node_id": "gpu-node-01",
    "engine": "vllm",
    "plan_version": "3",
    "replica_id": "0"
  },
  "hints": {},
  "ready": true,
  "health": "ready"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `schema_version` | int | 契约版本；当前 `0` |
| `id` | string | 目录内稳定键。none 模式建议 `{model_uid}/{replica_id}`，与今日 etcd key 对齐 |
| `base_url` | string | OpenAI 兼容根（无尾路径）。热路径只向这里发 `/v1/*` |
| `models` | string[] | 可服务的对外名 / 内部 uid；选路按请求 `model` 过滤 |
| `role` | enum | `unified` \| `prefill` \| `decode`；未知值进 `labels`，热路径当普通候选或忽略 |
| `labels` | map | 不透明；禁止在核心 crate 解析平台专有键 |
| `hints.bootstrap_port` | int? | SGLang PD 等握手端口；**Nebula 不连此端口** |
| `hints.transfer` | object? | 引擎间 KV 传输提示；**Nebula 不搬 KV** |
| `ready` | bool | 可被选路 |
| `health` | enum | `starting` \| `ready` \| `unhealthy` \| `draining` \| `failed`（对齐今日 `EndpointStatus`） |

PD 示例（decode worker；prefill 另条 `role=prefill`）：

```json
{
  "schema_version": 0,
  "id": "llama-70b/decode/2",
  "base_url": "http://10.0.12.21:8000",
  "models": ["meta-llama/Llama-3.1-70B-Instruct"],
  "role": "decode",
  "labels": { "engine": "sglang", "worker_type": "decode" },
  "hints": {
    "bootstrap_port": 8998,
    "transfer": { "protocol": "nixl" }
  },
  "ready": true,
  "health": "ready"
}
```

`EndpointSignals` v0（可选，可缺省）：

```json
{
  "endpoint_id": "qwen3-prod/0",
  "updated_ms": 1726700000000,
  "queue_depth": 4,
  "inflight": 2,
  "kv_cache_util": 0.71,
  "prefix_hint": null
}
```

缺字段 = 未知（**禁止用 0 表示不支持**，与 [`../dev/stats.md`](../dev/stats.md) / C6 一致）。

---

## 6. `CONTROL_PLANE` 模式

拟议环境变量（**尚未实现，勿当已有配置**）：

```text
CONTROL_PLANE=none|powerllm|dynamo|llm-d|custom
```

| 值 | 发现 | 鉴权 | 准入 | 装配 |
|----|------|------|------|------|
| **`none`（默认）** | 今日 etcd `/endpoints/` list+watch；实验室可加静态列表 | 今日 `NEBULA_AUTH_TOKENS` / API Key | 可选 `NEBULA_MULTI_TENANT` + `/tenants/` | **无**可选适配 crate |
| `powerllm` | PowerLLM 库存 / 服务目录 → Endpoint | 可选对接其身份，或仍用 Nebula token | 可选 | `nebula-adapter-powerllm` 或 sidecar |
| `dynamo` | Dynamo worker 注册（etcd 或 K8s CRD/EndpointSlices） | 默认仍 Nebula token（Dynamo 鉴权非核心） | 可选 | 可选 crate / sidecar |
| `llm-d` | `InferencePool` 候选投影 | 可对接 Gateway 已鉴权身份，或 Nebula token | 不把 EPP score 当 Policy | 可选 crate / sidecar |
| `custom` | 用户实现 `EndpointDirectory` | 用户实现或 none | 用户实现或 none | 进程内 trait / sidecar |

约束：

- 默认 **`none`**：与现网行为一致，不加载任何平台适配器。
- `custom` 不得把实现写进 `nebula-gateway` / `nebula-router` 源码；走可选 crate 或 sidecar。
- 适配器进程可以读 Dynamo etcd 或 kube EndpointSlices，但 **投影后的内存视图形状与 none 相同**。`none` 模式 **不**改 [`../dev/k8s.md`](../dev/k8s.md)：Router 仍不直接拿 kube 对象选路。
- 站点若已有 Envoy + EPP 或 Dynamo Frontend，可以**不用** Nebula Gateway 当入口；Nebula 只做编排或不部署数据面。禁止为「兼容」把那些前端抄进热路径。

---

## 7. PowerLLM：双边可选

| 产品 | 默认 | 另一侧 |
|------|------|--------|
| **PowerLLM** | 现有 Python 路径 | Nebula **可选**（适配器，不是替换内核） |
| **Nebula** | **Standalone**（`CONTROL_PLANE=none`） | PowerLLM **可选**适配器 |

互不硬依赖：PowerLLM 发行物不把 `nebula-*` 当必装；Nebula `Cargo.toml` 工作区不引入 PowerLLM 运行时。「核心无兼容层」有严格范围：**Gateway / Router 热路径与 etcd 控制面不模仿 Xinference / PowerLLM API，也不含平台分支**。控制台边缘是另一个面——`nebula-bff` 自 v1.9.0 起为 PowerLLM 控制台（`admin/`）提供南向协议兼容层（[`../../crates/nebula-bff/src/powerllm_compat.rs`](../../crates/nebula-bff/src/powerllm_compat.rs)），只服务该控制台、鉴权走控制台 session；它既不是本文的 `CONTROL_PLANE` 适配器，也不改变 standalone 默认行为。

---

## 8. 落地形态（实现时）

两种装配，可并存：

```text
【进程内】                         【Sidecar 投影】
adapter crate ──list/watch──┐      外部 CP ──► sidecar ──写 /endpoints/（及 /stats/）
                             ├─► 规范化快照 ──► Router / Gateway 热路径
none: etcd MetaStore ────────┘      （热路径仍只读规范化记录）
```

| 规则 | 说明 |
|------|------|
| 可选 crate | 例如 `nebula-adapter-dynamo`；**不**进 `nebula-gateway` 默认依赖 |
| Sidecar | 把外部发现写成今日 `/endpoints/`（lease）时，Router **零改**；适合先验证 |
| 插件 / middleware | 自定义 header、鉴权装饰走插件，**禁止**在核心管道写租户/平台分支（[`AGENTS.md`](../../AGENTS.md)） |
| 实现顺序 | 先冻结 JSON/trait → `none` 把现有 `EndpointInfo` 映射到 v0 → 再接一个外部 CP |

不在本文范围改 Placement / Scheduler：外部控制面自己管生命周期时，Nebula 可以只消费目录、不写 `/deployments/`。

---

## 9. 与现状映射（已核对代码）

下列为**当前主干事实**，不是拟议能力。核对：`CONTROL_PLANE` 在仓库中不存在；`crates/` 无 adapter 包；Router `watch_prefix("/endpoints/")`；鉴权在 `nebula-common` / `platform_auth`。

| 今日对象 | 拟议契约 | 映射 |
|----------|----------|------|
| `EndpointInfo`（`model_uid`/`replica_id`/`base_url`/`status`/…） | Endpoint v0 | `id={model_uid}/{replica_id}`；`models` 含 uid；`role=unified`；`labels` 收 `node_id`/`plan_version`/`endpoint_kind` |
| `EndpointStatus` | `health` + `ready` | `Ready` ⇒ `ready=true`；`Draining` 不可新流 |
| etcd `/endpoints/` list+watch | `EndpointDirectory`（none） | `endpoints_sync_loop` |
| `EndpointStats` / `/stats/` | `EndpointSignals`（none） | `pending_requests` → queue/inflight（今日未拆）；`kv_cache_usage` → `kv_cache_util` |
| `NEBULA_AUTH_TOKENS`、`platform_api_keys` | 内置 Auth（none） | 无外部 `AuthProvider` 也能跑 |
| `TenantAdmission` + `/tenants/` | 内置 `PolicySource`（none） | RPS / 并发 / token 预算；**已是配额，不是 KV score** |
| Gateway → Router → 引擎 `/v1/*` | UpstreamEngine | 不变；见 [`../dev/integration.md`](../dev/integration.md) §4 |
| BFF PowerLLM 控制台兼容层（v1.9.0） | —（不在本文契约内） | 控制台边缘南向适配；热路径不感知，standalone 不依赖 |
| OTLP `traceparent` → xtrace | 观测 | **不**在流式热路径挂 Langfuse |

`EndpointInfo` 今日无 `role` / `bootstrap_port`：none 模式全部 `unified`、hints 为空，直到 Node 或外部适配器开始填写。

---

## 10. 非目标

| 不做 | 原因 |
|------|------|
| 把 Dynamo / llm-d / GAIE EPP **叉进** Gateway/Router 热路径 | 破坏 Passthrough 与性能方案；平台升级绑死 Nebula |
| PowerLLM **硬依赖**（Cargo / 默认镜像） | 违反双边可选；standalone 必须可编可跑 |
| 流式热路径挂 **Langfuse**（或等价同步导出） | 观测失败不得挡 token；已用 W3C + xtrace（[`../dev/integration.md`](../dev/integration.md) §4.2） |
| 引擎协议非 OpenAI 时在 Router 翻译 | L1 边界在 Gateway + UniGateway（[`../dev/contracts.md`](../dev/contracts.md)） |
| Nebula 做 **KV 块传输** / PD 流合并 | 引擎到引擎；SGLang Gateway / Dynamo NIXL 的事 |
| 在 `PolicySource` 里做 KV / prefix **打分** | 与 llm-d「admission ≠ EPP」同构；打分属 Router 策略 + 可选 Signals |
| 把 vLLM / SGLang 标成 `CONTROL_PLANE` | 它们是 UpstreamEngine，最多是 Directory 的数据来源 |
| 用 kube API 替换 etcd 权威（none 模式） | [`../dev/k8s.md`](../dev/k8s.md)；K8s 只做执行面 |
| 文档把本文写成已落地能力 | 无 `CONTROL_PLANE`、无 adapter crate；落地后删草案口吻并进 Changelog（BFF 控制台兼容层是另一面，不改变本判定） |

---

## 11. 与数据面性能方案的关系

[`data-plane-perf-optimization.md`](./data-plane-perf-optimization.md) 要砍掉 Gateway→Router 内跳、做流式 body 透传、并用 pending / KV / prefix 做加权。与本文同时成立：

- 嵌入式 Router 仍只读**规范化** Endpoint 快照，不读 Dynamo 类型或 InferencePool CR。
- Phase 3 的 KV / prefix 加权只消费 `EndpointSignals`（none 下即今日 `/stats/`），不把 LMCache / KV 事件总线链进 `reqwest` 热路径。
- 平台适配器的 list/watch 属于**同步环**（对照今日 `endpoints_sync_loop`），失败时沿用旧快照 + 重连，不得阻塞单请求。

---

## 12. 成功标准（产品 / 评审）

- 默认 standalone：不配任何外部控制面，行为与今日 token + etcd 选路一致。
- 热路径源码对平台名无分支；适配器可卸。
- 契约能表达 Dynamo 的 Prefill/Decode、llm-d 的 Pool 候选、SGLang 的 `bootstrap_port`、vLLM 的 label 角色——而核心不引用这些项目。
- PowerLLM 与 Nebula 均可在对方缺席时发布。
- 实现 PR 另开；本文只定边界。
