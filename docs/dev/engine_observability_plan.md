# 引擎可观测开发与优化计划

> 状态：开发计划（2026-07-12）
>
> 产品边界见 [`../product/positioning.md`](../product/positioning.md)，总体可观测规范见 [`observability.md`](observability.md)，实时数据契约见 [`details/stats.md`](details/stats.md)。

## 1. 目标与边界

目标是让 Nebula 在不接管 vLLM / SGLang 内部调度的前提下，回答以下问题：

- 服务是否满足 TTFT、TPOT、吞吐、可用性和成本目标
- 瓶颈位于 Nebula 接入层、引擎、KV/cache、GPU，还是原生 Serving Cell
- 引擎版本、硬件、配置或发布变更是否造成性能回退
- 相同模型在不同引擎和硬件上的表现是否可比较

明确边界：

- 普通副本继续由 Nebula 管理并使用实时 stats 做路由、过载保护和扩缩容。
- vLLM Router、SGLang Model Gateway 等原生 Serving Cell 作为整体入口接入；内部拓扑、worker 生命周期、P/D 比例、KV 协同和请求调度归原生 serving 栈。
- Nebula 默认只采集 Cell Ingress 暴露的 `/metrics`、健康状态和官方只读 API。没有稳定接口时，不推测或反向解析内部 worker 状态。
- 本计划不包含自动创建、扩缩或调整 Prefill / Decode worker。

## 2. 当前基线

### 2.1 已有数据路径

```text
vLLM / SGLang /metrics
  └─ Node scrape
      ├─ etcd /stats/          实时控制面状态
      ├─ xtrace                产品内历史与语义
      └─ Node /metrics         Prometheus 导出

Gateway / Router
  ├─ Prometheus /metrics
  ├─ xtrace 双写
  └─ OTLP trace + JSON log

Nebula UI
  └─ BFF
      ├─ xtrace 查询
      ├─ 组件 /metrics 聚合
      └─ 控制面状态
```

现有三平面设计保持不变：

| 数据 | 权威出口 | 用途 |
|------|----------|------|
| 实时控制状态 | etcd `/stats/` | 路由、普通副本扩缩、Drain |
| LLM 语义与产品内历史 | xtrace | Trace、请求语义、真分位、控制台 |
| 运维时序 | Prometheus `/metrics` | 客户 Prometheus / VM / Grafana |
| 日志 | JSON stdout | Loki / ELK |

### 2.2 当前引擎采集

统一 `EndpointStats` 目前只有：

- `pending_requests`
- `prefix_cache_hit_rate`
- `prompt_cache_hit_rate`
- `kv_cache_used_bytes`
- `kv_cache_free_bytes`

实际映射情况：

| 类别 | vLLM | SGLang |
|------|------|--------|
| waiting / running | 已采集并合并为 pending | 已采集并合并为 pending |
| KV/cache 使用率 | 已采集 | 已采集 |
| prefix cache 命中率 | 已采集部分版本方言 | 尚未映射 |
| prompt cache 命中率 | 契约预留，未实现 | 契约预留，未实现 |
| TTFT / TPOT / token / 吞吐 | 未从引擎 scrape 统一 | 未从引擎 scrape 统一 |
| 引擎原始指标 | 未系统保留 | 未系统保留 |

因此，“Nebula 已完整理解引擎状态”不是当前事实。现状是少量指标支撑实时控制，Gateway / Router 请求指标补充端到端视角。

## 3. 目标数据分层

### 3.1 L0：控制面实时 stats

只保存 Router、Scheduler、Drain 立即需要的小规模最新状态。字段必须低频、低基数、带新鲜度和 lease。

不把延迟 histogram、token counter、引擎全部原始指标或 worker 清单写入 etcd。

### 3.2 L1：跨引擎统一 SLI

建立稳定的 Nebula 语义与单位：

| SLI | 主要采集点 |
|-----|------------|
| 请求量、状态、错误、abort | Gateway / Router |
| 端到端延迟、TTFT | Gateway / Router 流式代理 |
| TPOT / 输出吞吐 | 请求流或引擎公开指标 |
| 输入/输出 token | 响应 usage 或引擎公开指标 |
| waiting / running | 引擎 `/metrics` |
| KV/cache 压力 | 引擎 `/metrics` |
| GPU 利用率、显存、温度 | Node |

同名 SLI 必须明确单位、聚合方式和来源。来自代理测量与引擎测量的数据不能混为同一序列。

### 3.3 L2：引擎原始指标

保留有运维价值但不适合统一的数据，例如引擎调度、batch、cache、抢占、推测解码和原生 Gateway 指标。

要求：

- 使用独立命名空间并标注 `engine_type`、`engine_version`、`model_uid`、`cell_id` 等低基数标签。
- 不把 `request_id`、`user_id`、prompt 或 generation 放入 Prometheus label。
- 通过显式 allowlist 采集，避免上游新增高基数指标时污染时序库。
- 上游版本不存在某指标时显示“不支持”，不得用 `0` 伪装。

### 3.4 L3：拓扑与变更元数据

记录用于关联分析的只读元数据：

- 引擎类型、版本、镜像和启动配置摘要
- 模型、硬件、节点和普通副本关系
- 原生 Serving Cell 的入口、能力声明与健康
- 部署、升级、回滚、Drain 和配置变更事件
- 上游官方接口可提供时的 worker 角色与健康快照

L3 用于解释指标变化，不作为 Nebula 接管原生 Cell 的依据。

## 4. 开发阶段

### P0：口径与采集可靠性

目标：先保证“采得到、看得懂、不会误导”。

- 为 vLLM / SGLang 建立按版本记录的指标样本 fixture 和能力矩阵。
- 为引擎 scrape 增加成功、失败、超时、解析失败、数据陈旧指标。
- 明确 KV 当前使用比例转换为虚拟 used/free 值的语义，避免被误认为真实字节。
- 校验 OTLP endpoint 配置，确保 Gateway → Router → Engine 的 `traceparent` 连续传播。
- 清理或标注不再使用的 xtrace / Scheduler 遗留指标。
- 修正 BFF 指标名称、label 和实际 Router/Gateway 数据源不一致的问题。

验收：

- 引擎不可达、指标缺失、格式变化和 stale 数据均有明确状态与告警。
- 同一指标在文档、Prometheus、xtrace 和 UI 中单位一致。
- 不支持的引擎指标显示为 unknown / unsupported。

### P1：统一 SLI 补齐

目标：形成可比较的请求与引擎视角。

- Gateway 补齐按模型的端到端延迟、TTFT、abort 和流式结果口径。
- 对齐 Gateway 与 Router 的请求 outcome，确保 abort / 主动 Drain 不进入 5xx 错误预算。
- 在可稳定获取时补充 TPOT、输入/输出 token、吞吐和 cache hit；记录采集来源。
- SGLang 仅在官方 `/metrics` 提供对应数据时映射 prefix / prompt cache，不假定与 vLLM 指标同名。
- 为统一 SLI 增加 `engine_type`、`engine_version`、`model_uid` 和 `cell_id` 等低基数维度。

验收：

- 一次流式请求可以在 Gateway、Router、引擎和 Node 视角关联。
- vLLM 与 SGLang 面板使用相同单位，并明确 unsupported 字段。
- TTFT、端到端延迟和错误率可按模型、引擎版本和 Cell 对比。

### P2：原始指标与 Serving Cell 只读观测

目标：保留引擎差异，避免统一抽象丢失诊断信息。

- 为 vLLM 和 SGLang 分别维护原始指标 allowlist 与版本兼容规则。
- 将允许的原始指标输出到 Prometheus / xtrace 独立命名空间，不进入 etcd `/stats/`。
- 接入 vLLM Router、SGLang Model Gateway 的入口健康和聚合指标。
- 上游存在稳定官方只读接口时，采集 Cell 内 worker 角色、注册和健康快照；否则只展示 Cell 入口状态。
- 为每个字段显示来源、更新时间和支持状态。

验收：

- 原生 Cell 接入后，即使没有 worker API，也能准确显示“入口健康、内部状态不可见”。
- 上游指标新增或 label 变化不会未经审核进入客户时序库。
- 观测代码没有创建、删除、注册或扩缩 P/D worker 的写操作。

### P3：关联分析与产品化

目标：把数据转化为可执行但不越权的诊断。

- BFF 统一聚合 Gateway、Router、Engine、Node 和变更事件。
- 控制台支持按模型、引擎、版本、节点、Cell 和时间窗口下钻。
- 在性能曲线上叠加部署、升级、配置、故障与回滚事件。
- 建立 TTFT、TPOT、错误率、吞吐和成本的 SLO / 告警模板。
- 输出容量、引擎和配置建议，并展示证据、置信度与适用范围。

验收：

- 故障演练可以回答错误从哪一层开始、影响哪些模型/版本以及变更前后差异。
- 推荐只产生建议或跨 Cell 治理动作，不修改原生 Cell 的内部拓扑。
- 所有自动治理动作可解释、可审计、可回滚。

## 5. 实施约束

- 优先使用引擎稳定公开的 `/metrics` 和官方只读 API，不依赖私有接口或日志正则作为核心契约。
- 指标适配按引擎和版本隔离；不能仅靠模糊后缀匹配长期维持兼容。
- 原始指标采集必须有数量、label 基数和 scrape 开销预算。
- 请求热路径只做常数级测量与异步批量上报，不同步查询观测后端。
- UI 只通过 BFF 访问数据，不直连 xtrace、Prometheus 或引擎。
- 控制面实时决策不依赖历史观测系统可用性。

## 6. 测试矩阵

| 场景 | 验证 |
|------|------|
| vLLM 普通副本 | pending、KV、可用 cache 指标和请求 SLI 正确 |
| SGLang 普通副本 | pending、KV 正确；缺失 cache 指标显示 unsupported |
| 原生 Serving Cell | 只读采集 Ingress；无内部 API 时不虚构 worker 状态 |
| SSE 请求 | TTFT、结束状态、abort 口径一致 |
| 引擎 `/metrics` 超时 | stats 过期、告警和路由降级符合契约 |
| 上游指标改名 | fixture 测试失败并指出兼容性变化 |
| 高基数标签 | allowlist 拒绝或剥离危险 label |
| 观测后端故障 | 推理热路径和实时控制面继续工作 |
| 版本或配置变更 | 趋势图能关联变更事件并比较前后窗口 |

## 7. 与现有文档和排期的关系

- [`observability.md`](observability.md) 是三平面可观测设计与实施规范。
- [`details/stats.md`](details/stats.md) 是控制面实时 stats 契约。
- 本文负责 vLLM / SGLang 指标扩展、Serving Cell 只读观测和产品化开发顺序。
- [`../arch/optimization.md`](../arch/optimization.md) 仍是执行勾选入口；开始实施时应将本文阶段拆成对应工程项并同步状态。
