# Nebula 产品定位

> 产品视角说明：Nebula 是什么、解决谁的什么问题、相对推理引擎自身 serving 栈的边界与附加价值。  
> 工程架构见 [`../arch/architecture.md`](../arch/architecture.md)；排期见 [`../arch/optimization.md`](../arch/optimization.md)。

**一句话：** Nebula 是**本地 / 专有化推理机房**的集群操作系统——帮客户在多样加速器、多样引擎、多样模型之上，稳定地「部署、调度、治理、增强」推理服务；算得快交给引擎，管得好交给 Nebula。

**明确不做：** 不把「在线云 API 代理 / 虚拟模型服务」做成产品能力。那与本地部署的调度、GPU 匹配、版本矩阵是另一类问题，掺在一起会稀释定位。Nebula 只管**跑在自有（或专有）算力上的引擎进程**。

---

## 1. 我们服务谁

**主用户：** 需要自建或专有化 LLM 推理能力的平台团队、AI 基础设施团队、ISV / 私有化交付团队。

**他们真正要的结果：**

- 某个模型在现有硬件上能跑、跑得稳、能扩缩
- 对外有统一 API、鉴权、审计与可观测
- 换卡、换引擎、换版本时尽量少折腾

他们通常**不想**先成为 vLLM / SGLang / TensorRT / MLX / llama.cpp 的版本与镜像专家，再成为加速器机房的调度专家。

---

## 2. 市场变化与痛点

推理引擎正在「向上长」：vLLM 有 production-stack，SGLang 有 Model Gateway，各自补路由、PD、cache-aware、可观测。这对单引擎集群是好事，但也带来新问题：

| 痛点 | 表现 |
|------|------|
| **引擎多样性** | GPU 大流量用 vLLM/SGLang，极致吞吐用 TensorRT-LLM，Apple Silicon 用 MLX，轻量用 llama.cpp……栈只增不减 |
| **版本 × 硬件矩阵** | 同引擎不同版本、不同 CUDA/驱动/Metal、不同卡型镜像；用户选型成本极高 |
| **模型多样性** | 稠密 / MoE、多模态、embedding、rerank、小模型本地化，能力与资源画像不同 |
| **加速器多样性** | 多代 NVIDIA、国产卡、Apple Silicon 等并存；库存、占用、适配关系不透明 |
| **匹配关系会变** | 「哪块硬件跑哪个引擎的哪个版本、挂哪个模型」不是一次性配置，需要可调整 |

单引擎自带的 serving / gateway 擅长**把一种引擎跑快**；不会替客户消化「多引擎 × 多硬件 × 多模型 × 多版本」的机房级复杂度。这正是 Nebula 的产品空间。

---

## 3. 定位：管引擎、用引擎、增强引擎

### 3.1 Nebula 是什么

- **声明式控制面：** 模型部署期望 → 放置计划 → 节点启停**本地引擎** → 统一接入
- **多引擎编排层：** 把各引擎（含其原生拓扑 / gateway）当作可调度单元，而不是重做一套「更懂 PD 的引擎」
- **企业增强层：** 统一协议与鉴权审计、跨副本调度与 Drain、可观测与 SLO、控制台与运维闭环

简单副本模式保持 **Engine-Passthrough**：客户流量经 Nebula Gateway / Router 到达引擎原生 HTTP。对 PD、DP、KV Transfer 等复杂拓扑，Nebula 将引擎原生 gateway 作为一个整体服务入口接入，不绕过它直达 worker，也不管理其内部 worker 池；引擎升级尽量不牵动控制面。

### 3.2 Nebula 不是什么

- **不是**又一个 vLLM / SGLang Model Gateway（不与引擎比同拓扑内的 cache-aware / PD 路由谁更强）
- **不是**进程内托管任意 Python 模型的旧式 inference server（那是已放弃的 Xinference/PowerLLM 路径）
- **不是**云厂商 API 的聚合网关 /「虚拟模型服务」（在线调用与本地调度两回事，不纳入产品范围）
- **不是**通用 K8s 替代品；类比是「推理 workload 的集群 OS」，可与容器/K8s 共存

### 3.3 和引擎 serving 栈怎么相处

| 层级 | 谁负责 | 例子 |
|------|--------|------|
| 算力与同引擎拓扑 | 引擎（及引擎原生 gateway） | PD worker、prefix/KV 感知、gRPC tokenizer 管线 |
| 跨节点放置、扩缩、Drain、多模型 | Nebula | PlacementPlan、副本生命周期、计划版本一致性 |
| 跨引擎选择与版本/镜像 | Nebula | 按模型与硬件选引擎发行版 |
| 统一 API、租户、审计、机房可观测 | Nebula | Gateway、BFF、stats / trace / 日志 |

原则：**引擎负责算得快；引擎原生 serving 负责同拓扑内路由；Nebula 负责声明式调度、异构舰队、企业接入，并在需要时把引擎原生 serving 当作可编排组件接入。**

### 3.4 Serving Cell：Nebula 与原生 serving 的结合点

Nebula 不再只把服务理解为一组同质副本，而是把一个模型的一套完整运行拓扑抽象为 **Serving Cell**：

```text
Nebula Gateway（租户 / 鉴权 / 配额 / 模型与 Cell 选择）
  └─ Serving Cell
      ├─ Cell Ingress（Nebula Router 或引擎原生 Gateway）
      ├─ Regular / Prefill / Decode Workers
      └─ KV Transfer / Cache 等引擎配套组件
```

简单模式由 Nebula Router 在 Nebula 已管理的同质副本间选路；复杂模式由 vLLM Router、SGLang Model Gateway 等原生组件负责 Cell 内请求编排与 worker 管理。Nebula 只接入 Cell Ingress，提供统一发现、健康检查、流量治理和可观测，不承诺创建、扩缩或调整其内部 Prefill / Decode 池。

同一状态必须只有一个 owner：普通副本的部署期望、进程生命周期和资源放置由 Nebula 管理；原生 Serving Cell 的内部拓扑、worker 生命周期、请求路由和 KV 协同由引擎 serving 栈管理。Nebula 不对原生 Cell 的 Prefill / Decode 池做第二套扩缩，也不叠加语义冲突的重试、熔断和负载均衡。

---

## 4. 核心产品价值（客户感知）

### 4.1 藏住复杂性：兼容与发行版管理

对外呈现「模型 + 目标（吞吐/延迟/成本）+ 可用资源」；对内维护 **引擎 × 版本 × 硬件** 矩阵（镜像、驱动/CUDA/Metal、已知兼容与禁忌）。升级可灰度、失败可回滚。客户跟 Nebula，Nebula 跟上游 release 节奏。

### 4.2 多引擎、多模型统一交付

同一套声明式入口覆盖尽量多的**本地推理引擎**：vLLM、SGLang、TensorRT-LLM、MLX、llama.cpp 等；按模型类型、硬件与 SLA 自动或半自动选引擎，而不是强迫用户先选技术栈。Engine 差异关在 Node 适配层，对外 API 尽量一致。**引擎种类越多，Nebula 作为统一编排层的价值越大。**

### 4.3 加速器台账与资源平面

管理并统计多样加速器（GPU / 国产卡 / Apple Silicon 等）：型号、显存/统一内存、驱动、占用与历史利用率。调度与排障建立在「看得见的库存」上，而不是脚本里的机器列表。

### 4.4 动态匹配：硬件 ↔ 引擎 ↔ 模型

绑定关系可配置、可调整：通过控制台灵活声明亲和与策略（例如某批卡固定某引擎发行版，Apple 节点优先 MLX，某模型优先某硬件）。关系变化走声明式与 reconcile，而不是改启动脚本。

### 4.5 读懂引擎，再调度

当前已采集各引擎的少量运行指标（排队、KV/显存、部分 prefix hit），翻译成精简的统一 stats 契约，供普通副本路由、过载保护与扩缩容使用。强化方向是形成三层观测：少量实时控制面 stats、跨引擎统一 SLI、保留引擎方言的原始指标。Nebula 将模型、引擎版本、硬件、发布事件和请求表现关联起来，帮助客户判断问题发生在哪一层。

对于 vLLM Router、SGLang Model Gateway 等原生 Serving Cell，Nebula 默认只读取 Cell Ingress 暴露的指标、健康和官方只读状态；只有上游稳定提供 worker 级观测接口时才展示内部角色状态。观测不等于控制，Nebula 不据此接管 Cell 内部调度或 Prefill / Decode worker。

### 4.6 企业级接入与运维闭环

统一 OpenAI 兼容接入、鉴权与审计、abort/drain、可观测三平面（trace / metrics / 日志）。客户买到的是「可运营的本地推理服务」，不是「能 curl 通的端口」。

### 4.7 用 SLO 和成本治理推理舰队

客户声明 TTFT、TPOT、吞吐、可用性和预算目标，Nebula 统一观测不同引擎服务是否达标，并据此执行准入、跨 Cell 流量治理、普通副本扩缩或给出配置建议。原生 Serving Cell 内部的 Prefill / Decode 比例、worker 扩缩和 KV 调度仍由对应引擎 serving 栈负责。

### 4.8 让引擎选择从经验变成证据

对候选硬件、引擎版本和参数做标准化 benchmark 与线上反馈，形成可复用的性能画像。Nebula 给出可解释的推荐、灰度验证与回滚，而不是宣称存在一个适合所有模型和负载的默认引擎。

---

## 5. 产品能力地图（加强方向）

以下能力与定位直接对应，按客户感知优先级排列：

1. **能力声明与 Serving Cell 拓扑**
   建立 `EngineCapability` 与 `ServingTopology`：描述引擎版本支持的 PD / DP / TP、gRPC、LoRA、结构化输出、KV Connector 和指标能力；识别 `standalone`、`replicated`、`native_gateway`、`pd_disaggregated` 等拓扑。对原生复杂拓扑只做能力发现和整体接入，不把所有引擎压成最低公共能力，也不接管其内部 worker。

2. **Engine Adapter 与原生 Gateway 纳管**
   Engine 抽象补充能力发现、配置校验、服务发现、健康检查和指标转换。优先让 SGLang Model Gateway、vLLM Router 作为整体 Cell Ingress 接入，同时明确其内部拓扑和 worker 生命周期仍归原生 serving 栈。

3. **引擎指标方言 → 统一服务语义**
   稳定适配各引擎 metrics，分层处理：`/stats/` 只保留实时决策必需字段；Prometheus / xtrace 承载 TTFT、TPOT、排队、KV、吞吐、错误和成本等统一 SLI；原始引擎指标保留独立命名空间。统一语义不能以丢失引擎特有信息为代价。

4. **SLO / 成本驱动的治理与建议**
   从单一资源指标升级到面向服务目标的观测和决策：支持普通副本弹性、跨 Cell 流量治理、容量保护和成本约束；对于原生 Serving Cell，提供可解释的容量与配置建议，不自动调整 Prefill / Decode 池。

5. **发行版 / 镜像矩阵**
   硬件感知选镜像或运行时、版本兼容表、灰度与回滚——版本痛点产品化。

6. **加速器库存与统计**
   多样硬件的登记、健康、利用率与调度约束。

7. **Benchmark 与推荐系统**
   沉淀模型 × 引擎 × 版本 × 硬件 × 参数的性能画像，以真实目标推荐运行方案，并通过线上灰度校正推荐。

8. **多租户治理与企业运维**
   统一租户配额、优先级、准入、审计、成本归因、发布、Drain、故障迁移与回滚；控制台配置硬件–引擎–模型匹配，背后仍是声明式 Deployment / Placement。

9. **引擎覆盖面持续扩大**
   在 Engine 抽象上优先做深 vLLM / SGLang，并扩展 TensorRT-LLM、MLX、llama.cpp 等；多样性是产品目标，不是附属项。

10. **按需协议加深（EngineShim）**
   仅当 Passthrough 接不住引擎高级能力时启用；默认不与引擎 gateway 抢数据面。

---

## 6. 差异化一句话

| 对象 | 他们强在 | Nebula 强在 |
|------|----------|-------------|
| vLLM / SGLang / MLX 等引擎 | 单引擎性能与同拓扑 serving | 跨引擎编排、版本/硬件矩阵、机房治理 |
| 引擎自带 Gateway / production-stack | 把该引擎集群跑顺 | 多引擎统一入口 + 企业增强 + 硬件/模型匹配 |
| 云 API 聚合网关 | 多厂商在线模型一键调用 | **不做此类**；专注本地引擎与加速器 |
| 传统推理平台（进程内托管） | 快速试用多种模型 | 故障域清晰、状态可审计、引擎可独立升级 |

**Nebula 的附加价值不是「比引擎更会 serving」，也不是「再做一个云模型代理」，而是「让客户不用分别学会多套本地 serving 栈与版本矩阵，就能在异构加速器上稳定交付推理服务」。** 本地引擎越强、种类越多，编排与兼容层的杠杆越大。

---

## 7. 成功怎么衡量（产品）

- 新模型上线：从「选引擎/镜像/参数」到「可服务」的步骤与耗时下降  
- 换卡或升级引擎：回滚成功率、人为改配置次数下降  
- 多引擎并存时：仍只有一套对外 API 与一套运维视角（硬件 / 模型 / 副本）  
- 客户是否能通过 Nebula 统一查看、接入和治理原生 gateway，同时保持其内部管理权归属清晰
- 引擎覆盖：在真实硬件上可声明式交付的引擎种类持续增加（含 MLX 等非 CUDA 路径）
- 服务目标：TTFT / TPOT / 可用性达标率提高，单位有效 token 成本下降
- 治理决策：普通副本扩缩、跨 Cell 流量调整、升级回滚均可解释、可审计、可恢复

---

## 8. 文档关系

| 文档 | 角色 |
|------|------|
| 本文 | 产品定位与价值 |
| [`../arch/architecture.md`](../arch/architecture.md) | 工程架构与组件边界 |
| [`../arch/optimization.md`](../arch/optimization.md) | 排期与工程项 |
| [`../dev/engine_observability_plan.md`](../dev/engine_observability_plan.md) | vLLM / SGLang 可观测开发与优化计划 |
| [`../manual/deployment.md`](../manual/deployment.md) | 部署与运维手册 |
