# 产品定位对齐开发与优化计划

> 状态：**v1.3.0 Batch 1 已发布**（2026-07-13）；真机 e2e / 压测项仍暂缓。
>
> 本文把 [`../product/positioning.md`](../product/positioning.md) 的产品定位转化为工程阶段、交付物和验收门槛。架构与执行状态分别以 [`../arch/architecture.md`](../arch/architecture.md) 和 [`../arch/optimization.md`](../arch/optimization.md) 为准。Release Notes：[`../manual/release_notes_v1.3.0.md`](../manual/release_notes_v1.3.0.md)。

## 1. 目标

Nebula 的优化目标不是替代 vLLM、SGLang 或其原生 Gateway，而是成为本地 / 专有化推理环境中的跨引擎控制面：

- 统一管理普通引擎副本的部署、放置、生命周期和流量接入
- 将原生 Serving Cell 作为整体入口发现、接入、观测和治理
- 管理引擎、版本、镜像、硬件和模型之间的兼容关系
- 用统一 SLI、变更事件和成本数据支持诊断、SLO 与选型
- 提供多租户、审计、灰度、回滚和故障处置能力

最终客户应当只声明模型、服务目标和资源约束，而不必手工维护每种引擎的版本、镜像和启动脚本。

## 2. 不可突破的边界

- 原生 Serving Cell 内部的 worker 生命周期、P/D 比例、KV 协同和请求调度归 vLLM / SGLang serving 栈。
- Nebula 不创建、删除、注册或扩缩原生 Cell 的 Prefill / Decode worker。
- 普通副本与原生 Cell 必须分别只有一个状态 owner，不能形成双重 reconcile。
- Nebula 不复制引擎原生 cache-aware、PD 或 gRPC tokenizer 路由。
- Engine-Passthrough 仍是默认数据路径；EngineShim 只有在明确能力缺口出现时启用。
- `/stats/` 只保存实时控制决策必需字段，不承载历史、原始指标或 worker 清单。
- 不把在线云 API 聚合和虚拟模型服务扩展为产品主线。

## 3. 当前基线


| 能力             | 状态   | 当前事实                                                                             |
| -------------- | ---- | -------------------------------------------------------------------------------- |
| 声明式控制面         | 已实现  | Deployment → Placement → Node reconcile                                          |
| 普通副本生命周期       | 已实现  | vLLM / SGLang 本地或 Docker 启停、健康、自愈、Drain                                          |
| 普通副本路由         | 已实现  | least-pending、KV、prefix 策略及熔断、重试、过载保护                                            |
| 普通副本弹性         | 已实现  | 基于 pending / KV 阈值扩缩                                                             |
| Engine Adapter | 部分实现 | 启停/健康/scrape + 校验/静态表/运行时探测 + 方言 CLI + `/capabilities/` + 版本支持；完整镜像矩阵持续演进 |
| Serving Cell   | 部分实现 | Batch 1–2 + Cell 不重试边界；真机 Gateway e2e 暂缓                                         |
| 引擎可观测          | 部分实现 | `kv_cache_usage`、scrape 健康、fixture、ModelSlo 评估已落；真机 burn 暂缓               |
| 镜像管理           | 部分实现 | 注册/预拉/GC + platforms 放置 + 兼容矩阵 CRUD；缺历史画像                                        |
| 加速器台账          | 部分实现 | name/driver/cuda/platform + 库存 API/控制台；缺历史利用率库                                   |
| SLO / 成本治理     | 部分实现 | ModelSlo + 评估/诊断 + 租户用量/定价成本归因；真机 burn ⏸                                   |
| Benchmark / 推荐 | 部分实现 | schema + scripts runner + BFF recommend/canary + 控制台骨架；真机多引擎 e2e 暂缓              |
| 多租户            | 部分实现 | Tenant 实体/配额/Gateway 准入 + token→tenant；关闭 `NEBULA_MULTI_TENANT` 兼容单 token |
| 引擎覆盖           | 部分实现 | vLLM / SGLang；其他目标引擎尚无 Adapter                                                   |
| EngineShim     | 预留   | `GrpcShim` 类型存在，生产路径未实现                                                          |


产品文档中的「能力方向」不能表述为「当前已具备」。每项能力只有通过本文验收门槛后，才能更新为已实现。v1.3.0 将 P0–P6 的 Batch 1 工程交付标为完成，未勾选的真机验收项保持暂缓。

## 4. 总体演进顺序

```text
P0 契约、边界和观测可信度
 ├─→ P1 Engine Capability 与 Adapter 基座
 ├─→ P2 原生 Serving Cell 只读接入
 └─→ P3 镜像兼容矩阵与硬件台账

P0 + P1 ─→ P4 统一 SLI、SLO 与诊断
P3 + P4 ─→ P5 Benchmark、推荐、灰度与回滚
P4      ─→ P6 多租户与成本治理
P1 + P3 ─→ P7 扩展引擎；EngineShim 按门禁启用
```

阶段编号表达依赖顺序。P0–P6 Batch 1 已随 v1.3.0 发布；P7 与真机验收按需开启。

## 5. P0：契约、边界和观测可信度

### 目标

消除字段语义、指标来源和职责归属中的歧义，为后续能力建立可信基线。

### 交付物

- 定义 `EngineCapability`、`ServingTopology`、`CellIngress` 的 JSON / Rust 契约草案。 ✅（`nebula_common::capability`）
- `ServingTopology` 至少能识别 `standalone`、`replicated`、`native_gateway` 和 `pd_disaggregated`，但原生拓扑只记录能力和 Ingress。 ✅
- 为 vLLM / SGLang 建立按版本保存的 `/metrics` fixture 和解析兼容测试。 ✅
- 为 engine scrape 增加成功、失败、超时、解析失败和 stale 指标。 ✅（Node `nebula_node_engine_scrape_result`；Router stale 既有）
- 修正 `kv_cache_used_bytes` / `kv_cache_free_bytes` 当前实际为比例刻度而非真实字节的命名或展示语义。 ✅ → `kv_cache_usage`
- 校验 Gateway → Router → Engine 的 W3C TraceContext 和 OTLP endpoint。 ✅（inject/extract 往返单测 + `normalize_otlp_endpoint`）
- 审计 BFF、Gateway、Router、Scheduler 的指标名称、label 和实际数据源。 ✅（`upstream_5xx` 对齐；gateway API `data_source: router`；删除死 `*_xtrace_*`）
- 建立“支持 / 不支持 / 未知”三态，禁止用 `0` 表示上游不支持。 ✅（`SupportLevel` + `Option` 字段语义）

### 验收门槛

- 上游指标名或格式变化会触发 fixture 测试失败，并指出受影响版本。
- Prometheus、xtrace、BFF、UI 和文档对统一指标使用相同单位。
- `native_gateway` 契约没有 P/D worker assignment 或写操作。
- 引擎不可达、指标不支持和数据陈旧在 API / UI 中可区分。
- 观测后端故障不影响推理热路径与 etcd 实时控制面。

## 6. P1：Engine Capability 与 Adapter 基座

### 目标

将 Engine 从“启动命令封装”升级为可校验、可发现、可演进的引擎适配层。

### 交付物

- `EngineCapability` 描述引擎版本、协议、模型类型、TP / DP、LoRA、结构化输出、KV Connector 和观测能力。 ✅（静态表）
- Engine Adapter 增加能力发现、配置校验、健康语义、指标映射和版本兼容接口。 ✅（静态 + 启动后运行时探测；版本矩阵待补）
- 未知 `engine_type` 必须显式报错，不再静默回退 vLLM。 ✅
- 配置在创建 Deployment 时校验，避免等到 Node 启动阶段才失败。 ✅（BFF create/update/start/template）
- 能力声明区分“静态兼容表”“运行时发现”和“人工覆盖”，并记录来源。 🚧（StaticTable / RuntimeDiscovery 已用；ManualOverride 待）
- 每个 Adapter 维护版本支持范围、已知限制、fixture 和端到端部署用例。 ⏳
- Placement `extra_args` 按引擎方言编码（vLLM vs SGLang）。 ✅

### 验收门槛

- 不兼容的模型、引擎、版本或参数组合在部署前被拒绝，并返回可操作原因。
- vLLM / SGLang 现有普通副本行为保持兼容。
- 未知引擎返回明确错误，不启动错误的 vLLM 进程。
- Adapter 能区分 unsupported 与探测失败。
- 新增引擎必须通过统一 Adapter 契约测试，而不是在 Scheduler / Router 中增加专用分支。

## 7. P2：原生 Serving Cell 只读接入

### 目标

让 SGLang Model Gateway、vLLM Router 等原生 serving 入口进入 Nebula 的统一服务视图，同时不接管其内部 worker。

### 交付物

- 新增外部托管 `CellIngress` 的声明、注册、健康检查和删除流程。
- Router 将 Cell Ingress 视为整体上游，不直接发现或选择 Cell 内 worker。
- 支持 Ingress 的 OpenAI 兼容性、模型身份和能力校验。
- 采集 Ingress `/metrics`、健康和官方只读状态。
- 上游存在稳定只读 API 时，可展示 worker 角色、注册和健康快照；否则明确显示“内部拓扑不可见”。
- 为 Cell Ingress 定义重试、熔断和超时所有权，避免 Nebula 与原生 Gateway 重复放大请求。
- 控制台增加 Cell 入口、能力、健康、版本和观测来源视图。

### 验收门槛

- SGLang Model Gateway 和 vLLM Router 至少各完成一次真实 Ingress 接入验证。
- 推理流量只到达 Cell Ingress，不绕过它直达 P/D worker。
- Node 不对 External Ingress 执行 stop、restart、register-worker 或 scale-worker。
- 缺少 worker API 时，系统不推测内部拓扑或以“0 worker”误报。
- 删除 Nebula 中的接入声明不会删除或停止外部 Serving Cell。

### Batch 1 进度（工程）


| 项                                         | 状态                         |
| ----------------------------------------- | -------------------------- |
| `CellIngress` / etcd `/cells/`            | ✅                          |
| BFF 注册 / 列表 / 删除 + OpenAI 探针 + Running 互斥 | ✅                          |
| Router 整入口优先选路                            | ✅                          |
| Node 不管理外部 Cell                           | ✅（不 watch `/cells/`）       |
| Ingress metrics / 控制台视图                   | ✅（`/observe` + `/cells` 页） |
| 真实 Gateway e2e                            | ⏸ 等真机环境                    |


## 8. P3：镜像兼容矩阵与加速器资源平面

### 目标

把“引擎 × 版本 × 硬件”从人工经验变成可校验、可调度、可回滚的数据。

### 交付物

- Node 上报 GPU / 加速器型号、显存、驱动、运行时平台和健康能力。
- `EngineImage.platforms` 真正参与 BFF 校验与 Scheduler 放置。
- 建立引擎版本、镜像、平台、驱动和已知问题的兼容矩阵。
- Deployment 支持引用固定 `image_id`、版本策略和人工覆盖理由。
- Scheduler 在候选节点过滤阶段执行兼容检查，并输出可解释的拒绝原因。
- 控制台提供硬件库存、占用、兼容引擎和历史利用率视图。
- 先完整支持 NVIDIA 路径；其他平台在 Adapter 和采集器就绪前明确标记为手动登记。

### 验收门槛

- CUDA 镜像不会调度到不兼容节点。
- 兼容矩阵中禁止的组合在部署 API 阶段被拒绝。
- Placement 决策可以追溯使用了哪条兼容规则。
- 镜像切换产生新的部署 / Placement 版本，并可通过 Drain 安全回滚。
- 节点库存、Placement 和实际运行进程之间不存在无法解释的占用差异。

### Batch 进度（工程）


| 项                                                  | 状态  |
| -------------------------------------------------- | --- |
| platforms 参与放置 + GPU name/driver/cuda/platform     | ✅   |
| CompatibilityRule / `/compat/` + 部署校验 + 结构化拒绝      | ✅   |
| Deployment `image_id` / override / compat_rule_ids | ✅   |
| 硬件库存 API + 控制台治理页                                  | ✅   |
| GPU 历史画像 / 真机 Ascend 采集                            | ⏸   |


## 9. P4：统一 SLI、SLO 与关联诊断

### 目标

让 Nebula 能跨引擎比较服务表现、定位故障并提供有证据的治理建议。

详细指标实施见 [`observability.md`](observability.md) 与 [`details/stats.md`](details/stats.md)。

### 交付物

- 统一请求量、错误、abort、端到端延迟、TTFT、TPOT、token、吞吐、队列、KV 和 GPU 语义。
- 指标标明来源：代理测量、引擎指标或 Node 采集，禁止混成同一序列。
- 原始引擎指标使用 allowlist 和独立命名空间，不进入 etcd `/stats/`。
- 建立模型级 SLO 对象：TTFT、TPOT、可用性、吞吐和可选预算约束。
- 完成 SLO / 告警 runbook，明确 abort / 主动 Drain 不计入 5xx 错误预算。 ✅（`[slo_alerts.md](slo_alerts.md)`、O8）
- BFF 聚合 Gateway、Router、Engine、Node 与部署变更事件。
- 控制台支持按模型、引擎、版本、节点、Cell 和时间窗口下钻。
- 普通副本可生成扩缩建议；原生 Serving Cell 只生成容量或配置建议。

### 验收门槛

- 一次流式请求可关联 Gateway、Router、Engine 和 Node 视角。
- vLLM / SGLang 相同 SLI 使用相同单位，不支持字段显示 unsupported。
- 故障演练能定位错误起点、影响范围和关联变更。
- SLO 违约事件包含证据窗口、阈值、数据来源和建议。
- 任何针对原生 Cell 的建议都不包含 worker 写操作。

### Batch 进度（工程）


| 项                                 | 状态  |
| --------------------------------- | --- |
| O8 SLO/告警 runbook                 | ✅   |
| ModelSlo CRUD + evaluate（不足数据不假绿） | ✅   |
| DiagnosticEvent 时间线               | ✅   |
| 控制台治理页挂载                          | ✅   |
| 真机 burn / 成本模型 / 跨层演练             | ⏸   |


## 10. P5：Benchmark、推荐、灰度与回滚

### 目标

用可复现测试和线上反馈支持引擎、硬件、版本及参数选择。

### 交付物

- 在 `scripts/` 下建立标准 benchmark workload、执行器和结果 schema。 ✅（`scripts/benchmark/` + `nebula_common::benchmark`）
- 建立 `模型 × 引擎 × 版本 × 硬件 × 参数` 性能画像。 ✅（etcd `/benchmarks/profiles/`）
- 结果至少包含 TTFT、TPOT、吞吐、错误率、显存和单位有效 token 成本。 ✅（`BenchmarkRun` 字段；真机填数 ⏸）
- 推荐 API 输入模型、SLO、预算和可用硬件，输出候选方案、证据和置信度。 ✅（`POST /api/v2/benchmarks/recommend`）
- 无足够画像时返回“数据不足”，不静默选择默认引擎。 ✅
- 支持候选方案 canary、流量权重、对照评估和一键回滚。 ✅（`/api/v2/canaries`；违约自动 weight=0）
- 线上 SLI 只校正同类负载画像，避免将不同 prompt / output 分布直接比较。 ✅（`ProfileKey.workload_id` 维度约束）

### 验收门槛

- 至少两种模型、vLLM / SGLang 和一个硬件档位的 benchmark 可重复执行。 ⏸ 等真机
- 推荐结果可追溯到 benchmark run、软件版本和线上观测窗口。 ✅（候选含 `evidence_run_ids`）
- canary 未达 SLO 时停止放量并恢复上一稳定版本。 ✅（evaluate 违约回滚 `image_id`）
- 回滚后 Deployment、Placement、流量和观测事件保持一致。 ⏸ Placement/流量权重真机联调
- 推荐服务故障不影响手工声明式部署。 ✅（recommend 独立 API，部署路径不依赖）

### Batch 进度（工程）


| 项                                                                          | 状态  |
| -------------------------------------------------------------------------- | --- |
| `BenchmarkRun` / `PerformanceProfile` / `Recommend*` / `CanaryRelease` 契约  | ✅   |
| `scripts/benchmark` workload + `run_benchmark.py`（dry-run / live / ingest） | ✅   |
| BFF runs/profiles/recommend + canaries CRUD/evaluate/promote/rollback      | ✅   |
| 控制台 `/governance`：runs 列表、推荐、canary 操作                                     | ✅   |
| 真机双引擎多模型重复 benchmark                                                       | ⏸   |


## 11. P6：多租户、配额与成本治理

### 目标

在统一接入面提供企业需要的租户隔离、准入、审计和成本归因。

### 交付物

- 建立租户实体及 token / API key 映射。 ✅（etcd `/tenants/`；`NEBULA_AUTH_TOKENS=token:role[:tenant_id]`）
- `ExecutionContext` 全链路携带 `tenant_id`、priority、deadline 和预算信息。 ✅（`x-nebula-*` headers；auth tenant 覆盖客户端伪造）
- 支持租户级 RPS、并发、token 和模型访问配额。 ✅（`TenantQuota` + Gateway `TenantAdmission`）
- 准入控制在 Gateway / Router 边界执行，不把租户策略推入原生引擎。 ✅（Gateway 边界；引擎透传）
- 建立租户、模型、引擎版本和 Cell 维度的用量与成本归因。 ✅（`/usage/` + `/pricing/` + cost summary）
- 所有配额、策略、发布和治理动作进入审计日志。 ✅（audit 含 `tenant_id` / `deny_code` tags）
- 控制台提供租户 SLO、用量、成本和拒绝原因视图。 ✅（`/governance` 租户区；拒绝 breakdown）

### 验收门槛

- 一个租户耗尽配额不会阻塞其他租户。 ✅（按 `tenant_id` 隔离计数）
- 限流和拒绝具有稳定错误码、审计记录和指标。 ✅（`tenant_*` codes；低基数 metrics）
- 高基数租户信息不进入 Prometheus label；明细通过 trace / 审计查询。 ✅（`reason=` only）
- 成本数据能追溯 token、引擎 / 硬件价格配置和统计窗口。 ✅（pricing + 15m usage windows）
- 关闭多租户功能时保持现有单 token 模式兼容。 ✅（默认 `NEBULA_MULTI_TENANT` 关闭；`token:role` 仍可用）

### Batch 进度（工程）

| 项 | 状态 |
|----|------|
| `Tenant` / `TenantQuota` / `UsageWindow` / `CostPriceConfig` 契约 | ✅ |
| Auth `token:role[:tenant_id]` + 租户级 RPS key | ✅ |
| Gateway 准入（RPS/并发/token/模型）+ ExecutionContext 注入 | ✅ |
| BFF tenants/pricing/usage/cost API | ✅ |
| 控制台 governance 租户用量/成本/拒绝 breakdown | ✅ |
| 真机多租户压测与配额联调 | ⏸ |

## 12. P7：持续扩展引擎与按需 EngineShim

### 引擎扩展门槛

TensorRT-LLM、MLX、llama.cpp 或其他引擎只有满足以下条件才进入支持列表：

- 有明确客户场景、硬件目标和维护 owner
- 完成 Engine Capability、配置校验和兼容矩阵
- 完成普通副本启动、健康、Drain、停止和失败恢复
- 完成指标 fixture、统一 SLI 和 unsupported 语义
- 完成真实硬件端到端部署与流式请求验证

### EngineShim 启用门槛

只有同时满足以下条件才启动 EngineShim：

- Passthrough 无法提供明确且高价值的引擎能力
- 原生 Gateway 无法直接接入或无法满足协议要求
- Shim 不复制原生 Cell 内路由和 worker 管理
- 性能收益或兼容价值有可重复 benchmark 证据
- 故障时可以回退 Native HTTP Passthrough

跨节点 TP 同样保持按需能力，仅在单机资源无法承载目标模型且有真实场景时启动。

## 13. 横向工程要求

### 兼容与迁移

- 新字段使用向后兼容默认值，etcd schema 变更必须提供迁移和回滚。
- 新能力先以显式 feature flag 或 capability gate 发布。
- BFF 是控制台唯一后端；前端不直连 etcd、xtrace、Prometheus 或引擎管理 API。

### 性能与可靠性

- 推理热路径不查询历史观测、兼容矩阵或推荐服务。
- 指标和 trace 异步批量写入；观测失败不能阻塞 token 流。
- 自动治理默认建议模式，只有经过故障演练和回滚验证后才能显式启用。

### 安全

- 引擎配置、外部 Ingress 和镜像来源需要准入校验。
- 日志、指标和 trace 不记录 prompt、generation、token 或 secret，除非有明确脱敏策略。
- 外部 Cell 只读凭据与推理凭据分离。

### 测试

- 契约：序列化兼容、schema migration、capability negotiation。
- Adapter：fixture、命令生成、健康、失败恢复和真实引擎 smoke test。
- 控制面：watch、CAS、lease、stale、Drain、回滚和 HA。
- 数据面：SSE、abort、超时、重试、熔断和大请求。
- 产品：真实硬件、不同引擎版本、原生 Cell、租户隔离和故障演练。

## 14. 成功指标


| 产品结果    | 衡量方式                                   |
| ------- | -------------------------------------- |
| 新模型更快上线 | 从创建 Deployment 到 Ready 的耗时、失败步骤和人工修改次数 |
| 升级风险下降  | canary 拦截率、回滚成功率、不可用时间                 |
| 兼容问题前移  | 部署前拦截的不兼容组合比例、Node 启动失败率               |
| 服务目标可运营 | TTFT / TPOT / 可用性达标率、告警可解释率            |
| 资源效率提高  | GPU 有效利用率、单位有效 token 成本、空闲容量           |
| 多引擎管理简化 | 可声明式交付的引擎 / 版本 / 硬件组合数                 |
| 故障定位加快  | 从告警到确定故障层级的步骤和平均诊断时长                   |
| 治理可信    | 自动动作审计覆盖率、回滚覆盖率、错误自动动作数                |


## 15. 风险与控制


| 风险                       | 控制措施                                         |
| ------------------------ | -------------------------------------------- |
| Nebula 越权管理原生 P/D worker | Cell 仅 External Ingress；契约和集成测试禁止 worker 写操作 |
| 指标版本漂移                   | 按版本 fixture、allowlist、unsupported 三态         |
| 兼容矩阵只存不消费                | BFF 校验与 Scheduler 过滤必须同时上线                   |
| SLO 数据不完整却自动治理           | 默认建议模式；数据质量门槛和置信度                            |
| 无 benchmark 的“智能推荐”      | 数据不足显式返回，不设置隐式默认引擎                           |
| 多租户增加热路径开销               | 低成本准入、基准测试、可关闭和 bypass                       |
| EngineShim 范围膨胀          | 使用启用门槛；保留 Passthrough 回退                     |
| 文档状态与实现脱节                | 每次阶段交付同步本文、架构状态和执行勾选                         |


## 16. 执行与文档治理

- 本文是产品定位到工程工作的全面映射，不替代架构设计或迭代任务系统。
- `[../arch/optimization.md](../arch/optimization.md)` 继续记录当前执行批次和状态。
- [`observability.md`](observability.md) / [`details/stats.md`](details/stats.md) 负责引擎可观测与 `/stats/` 契约细节。
- 每个阶段启动前应拆成可独立验收的工程项，并指定 owner、依赖和回滚策略。
- 每个阶段完成后必须更新当前基线、验收证据和产品文档中的能力状态。

