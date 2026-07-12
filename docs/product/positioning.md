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

简单副本模式保持 **Engine-Passthrough**：客户流量经 Nebula Gateway / Router 到达引擎原生 HTTP。对 PD、DP、KV Transfer 等复杂拓扑，Nebula 将引擎原生 gateway 作为服务入口纳管，不绕过它直达 worker；引擎升级尽量不牵动控制面。

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

简单模式由 Nebula Router 在同质 worker 间选路；复杂模式由 vLLM Router、SGLang Model Gateway 等原生组件负责 Cell 内请求编排。Nebula 负责创建、放置、扩缩、升级和观测整个 Cell，并只把 Cell Ingress 暴露给上层。

同一状态必须只有一个 owner：Nebula 是部署期望、进程生命周期和资源放置的权威；引擎 gateway 是 Cell 内请求路由、worker 实时负载和 KV 协同的权威。不得让两边同时扩缩同一 worker 池，也不得叠加语义冲突的重试、熔断和负载均衡。

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

采集各引擎运行指标（排队、KV/显存、prefix hit 等），翻译成统一 stats 契约，供路由、过载保护与扩缩容使用。同引擎内的高级路由可委托引擎原生能力；Nebula 做跨组、跨模型、跨租户的集群级决策。

### 4.6 企业级接入与运维闭环

统一 OpenAI 兼容接入、鉴权与审计、abort/drain、可观测三平面（trace / metrics / 日志）。客户买到的是「可运营的本地推理服务」，不是「能 curl 通的端口」。

### 4.7 用 SLO 和成本驱动推理舰队

客户声明 TTFT、TPOT、吞吐、可用性和预算目标，Nebula 结合真实流量、队列、KV、GPU 利用率与硬件成本，调整副本数、Prefill/Decode 比例、放置和引擎配置。优化对象是整个 Serving Cell 和集群，而不是单个进程的 GPU 利用率。

### 4.8 让引擎选择从经验变成证据

对候选硬件、引擎版本和参数做标准化 benchmark 与线上反馈，形成可复用的性能画像。Nebula 给出可解释的推荐、灰度验证与回滚，而不是宣称存在一个适合所有模型和负载的默认引擎。

---

## 5. 产品能力地图（加强方向）

以下能力与定位直接对应，按客户感知优先级排列：

1. **能力声明与 Serving Cell 拓扑**  
   建立 `EngineCapability` 与 `ServingTopology`：描述引擎版本支持的 PD / DP / TP、gRPC、LoRA、结构化输出、KV Connector 和指标能力；支持 `standalone`、`replicated`、`native_gateway`、`pd_disaggregated` 等拓扑。控制面按能力选节点、生成参数并把整个 Cell 作为部署单元编排，不能把所有引擎压成最低公共能力。

2. **Engine Adapter 与原生 Gateway 纳管**  
   Engine 抽象从启动单个进程扩展为能力发现、配置校验、拓扑编译、部署、服务发现、Drain 和指标转换。优先让 SGLang Model Gateway、vLLM Router 成为可管理的 Cell Ingress，同时明确控制权边界。

3. **引擎指标方言 → 统一服务语义**  
   稳定适配各引擎 metrics，统一 TTFT、TPOT、排队、KV、吞吐、错误和成本口径；保留引擎特有指标，避免统一抽象丢失关键能力。

4. **SLO / 成本驱动的弹性与调优**  
   从固定阈值扩缩升级到面向服务目标的决策：调整普通副本或 Prefill/Decode 池，支持流量预测、容量保护和成本约束，并对每次自动决策提供原因和回滚点。

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
- 客户是否仍需直接运维引擎原生 gateway 才能完成日常扩缩与发布（目标：默认不需要）  
- 引擎覆盖：在真实硬件上可声明式交付的引擎种类持续增加（含 MLX 等非 CUDA 路径）
- 服务目标：TTFT / TPOT / 可用性达标率提高，单位有效 token 成本下降
- 自动决策：Serving Cell 扩缩、P/D 比例调整、升级回滚均可解释、可审计、可恢复

---

## 8. 文档关系

| 文档 | 角色 |
|------|------|
| 本文 | 产品定位与价值 |
| [`../arch/architecture.md`](../arch/architecture.md) | 工程架构与组件边界 |
| [`../arch/optimization.md`](../arch/optimization.md) | 排期与工程项 |
| [`../manual/deployment.md`](../manual/deployment.md) | 部署与运维手册 |
