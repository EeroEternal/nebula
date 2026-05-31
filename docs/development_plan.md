# 开发计划

## 1. 现状概览
- 控制面与执行面链路已打通：动态模型加载、调度、Node 多进程多模型、Embeddings、CLI 基础管理能力（list/load/unload/status）。
- ✅ 已完成：Prometheus /metrics 标准暴露（Gateway/Router/Node/Scheduler 全部完成）、结构化 JSON 日志、CLI 流式日志。
- ✅ 已完成：统一鉴权（共享 auth 库 + Gateway/BFF/Router 全部迁移）。
- ✅ 已完成：审计日志、多引擎抽象（vLLM + SGLang）、镜像管理、前端多引擎集成。
- ✅ 已完成：模型推理路由修复（model_name ↔ model_uid 双向映射）。
- ✅ 已完成：前端 Inference 页面指标修复。
- 仍缺：一键部署（helm chart）、容量感知调度、Web Console 完善。

## 2. 目标
- 提供“可观测、可管控、可扩展”的生产级控制面：统一 Control API、鉴权、审计、可观测性、容量感知调度、友好管理体验（CLI + Web）。

## 3. 近期里程碑与优先级
1) **Control API + 鉴权基线** ✅（已完成）
   - ✅ 统一控制面 API（模型请求、节点/端点状态、操作审计、日志/metrics 拉取）。
   - ✅ 共享 auth 库（nebula-common），Gateway/BFF/Router 统一 API key + RBAC（viewer/operator/admin），请求限流与审计日志。
2) **可观测性与健康信号** ✅（已完成）
   - ✅ Gateway/Router/Node/Scheduler 暴露 Prometheus 指标；结构化 JSON 日志（--log-format json）；trace-id 贯穿（xtrace OTLP）。
   - ✅ 健康/容量信号：vLLM /health、GPU 显存与利用率、端口占用、plan_version 一致性。
   - ✅ CLI `metrics`、`logs --follow` 流式日志；前端 Inference 页面指标修复。
3) **CLI 能力补全** ✅（已完成）
   - ✅ 已完成：`chat`（流式对话）、`logs`（含 `--follow` 流式）、`metrics`、`drain`（端点优雅下线）、`scale`（副本调整）、`whoami`。
   - 待做：多集群上下文配置、模板化/批量操作。
4) **后台服务 + Web Console MVP**（中优先级）
   - 后台服务：轻量 Rust/Go，封装 Control API，内置 auth/RBAC/审计，供 CLI 与前端共用。
   - Web Console：总览（节点/模型/端点健康）、模型生命周期操作、事件与审计流、基础日志/metrics 视图（可链接 Grafana/Tempo/Loki）。
5) **容量感知与多 GPU 生产化**（中优先级）
   - Node Manager 自动发现多 GPU 并派生子 worker；Scheduler 加入显存/利用率/端口分配器与容量校验；支持模型预拉取/缓存。
6) **运维与交付**（次优先级）
   - 一键部署：docker-compose/helm；升级/回滚脚本；备份/恢复 etcd 元数据流程。
   - 风险防护：重试/回退策略，自愈与 backoff。
7) **功能扩展（可并行）**（低优先级）
   - LoRA 支持（ModelLoadRequest adapters，Node 启动 --enable-lora）。
   - `/v1/models` 聚合、function calling/rerank 等 OpenAI 兼容特性。

## 4. 交付物与验证
- Control API 与鉴权：API 定义文档、RBAC 配置示例、E2E 测试（含拒绝/通过场景）。
- 可观测性：Prometheus 指标清单、采样 dashboards、trace 示例、日志格式样例；健康检查脚本。
- CLI：新增子命令使用示例与回归测试（chat/logs/metrics/drain/scale/auth）。
- 后台服务 & Web Console：MVP 截图/录屏，核心流（登录、总览、模型操作、日志/metrics 查看）的手动/自动化用例。
- 容量感知：调度与 Node 端容量校验单测 + 集成测试（显存不足拒绝、端口分配冲突检测）。

## 5. 风险与缓解
- **安全欠缺**：API 未鉴权可能暴露管理面 → 优先加 token/RBAC 与限流。
- **观测缺失**：无指标/日志会放大排障成本 → 在功能开发并行落地指标与日志。
- **容量误配**：显存/端口冲突导致服务不可用 → 引入容量校验与端口分配器，预检查失败即拒绝调度。
- **多 GPU 复杂度**：自动发现与子进程管理可能引入不稳定 → 先在单机多卡做灰度与回归，再推广多节点。

## 6. 粗略时间顺序
- Week 1-2: Control API + 鉴权，指标/日志基线，CLI auth/metrics/logs。
- Week 3-4: drain/scale、健康信号完善，容量校验与端口分配器；Node 多 GPU 原型。
- Week 5-6: 后台服务与 Web Console MVP；一键部署脚本；观测仪表盘。
- Week 7+: LoRA 与高级 OpenAI 兼容特性，运维回滚/自愈完善。

## 7. 镜像与模型文件管理（Roadmap）

当前 Nebula 已切换为全容器化部署 vLLM（类似 k8s 思路），由此引出对**引擎镜像**和**模型文件**的统一管理需求：

### 7.1 多引擎 × 多硬件 × 多模型适配（引擎抽象 ✅，硬件感知调度待做）

现实中不同硬件、引擎、模型之间存在复杂的适配关系，极端情况下某张卡 + 某个引擎 + 某个模型才有最优性能：

| 硬件平台 | 推理引擎 | 典型场景 |
|----------|---------|---------|
| NVIDIA GPU (CUDA) | vLLM, SGLang | 通用模型推理 |
| 华为昇腾 NPU | vLLM-Ascend, xLLM, SGLang-Ascend | GLM-5 等国产模型优化 |
| 特定卡型 | 厂商魔改引擎 | 针对特定模型的深度优化（如 GLM-5-w4a8 量化） |

参考项目：
- vLLM-Ascend: https://github.com/vllm-project/vllm-ascend
- xLLM: https://github.com/jd-opensource/xllm
- SGLang Ascend: https://github.com/sgl-project/sglang (ascend_npu 支持)
- GLM-5-w4a8 量化: https://modelers.cn/models/Eco-Tech/GLM-5-w4a8

**设计要点**：
- **引擎抽象层**：Node 不绑定 vLLM，而是抽象为 `Engine` trait，支持 vLLM / SGLang / xLLM 等多种后端，每种引擎对应不同的 docker 镜像和启动参数。
- **硬件感知调度**：节点上报硬件类型（NVIDIA/昇腾/型号），Scheduler 根据模型的硬件兼容性和引擎偏好做匹配调度。
- **适配矩阵**：维护 `(硬件, 引擎, 模型) → 镜像` 的映射表，前端 Load Model 时自动推荐最优引擎和镜像组合。
- **镜像族**：同一引擎可能有多个镜像变体（CUDA 12.4 / 12.8 / 昇腾 CANN 8.0 等），按节点硬件自动选择。

### 7.2 引擎镜像管理 ✅
- **镜像注册表**：`EngineImage` 数据结构存储在 etcd `/images/{id}`，Gateway CRUD API (`/v1/admin/images`)。
- **镜像预拉取**：Node 启动时扫描 + watch `/images/` 前缀，自动 `docker pull` 匹配镜像；状态上报到 `/image_status/{node_id}/{image_id}`。
- **版本策略**：`VersionPolicy::Pin`（本地有则跳过）和 `Rolling`（每次 re-pull 获取最新 digest）。
- **清理**：Node 每小时 GC，清理不在注册表且未被容器使用的引擎镜像。
- **per-model 镜像覆盖**：`PlacementAssignment.docker_image` 字段，优先于 Node CLI 默认镜像。

### 7.3 模型文件管理
- **统一缓存目录**：所有节点使用约定路径（如 `/DATA/Model`）存放模型文件，容器挂载该目录。
- **模型预热/推送**：支持将模型文件从中心存储（NAS/S3/ModelScope）预推送到指定节点，避免首次加载时下载。
- **缓存清单**：节点上报已缓存的模型列表，Scheduler 优先调度到已有缓存的节点（亲和性调度）。
- **空间管理**：监控各节点模型缓存磁盘占用，支持 LRU 淘汰或手动清理。

### 7.4 前端集成 ✅（多引擎 + 镜像管理已完成）
- ✅ Images 页面：镜像注册表 CRUD、各节点镜像拉取状态查看。
- ✅ Load Model 对话框：Engine Type 选择（vLLM / SGLang）+ Docker Image 下拉（从注册表按引擎类型过滤）。
- ✅ Models 页面：显示每个部署请求的引擎类型；FAILED 状态含镜像缺失提示时显示 "Go to Images →" 快捷跳转。
- ✅ Endpoints 页面：Engine 列显示每个端点使用的引擎类型（vLLM / SGLang）。
- 待做：Load Model 时提示"该节点已缓存此模型，预计启动更快"。
- 待做：从 UI 触发模型预热操作。

### 7.5 多引擎支持 ✅

**架构**：
- `Engine` trait 抽象引擎生命周期（start / stop / health_check / scrape_stats / try_restart / try_reuse）。
- `create_engine` 工厂函数根据 `engine_type` 创建对应实现，支持 `docker_image` 覆盖。
- 去掉全局 engine 单例，每个 `RunningModel` 持有自己的 `Arc<dyn Engine>`。
- `reconcile_model` 根据 `PlacementAssignment.engine_type` 动态创建引擎实例。

**已实现引擎**：
- **VllmEngine**（`engine/vllm.rs`）：Docker + 本地二进制模式，vLLM Prometheus 指标解析。
- **SglangEngine**（`engine/sglang.rs`）：Docker + 本地二进制模式（`--ipc=host`），SGLang Prometheus 指标解析。

**端到端数据流**：
前端 Load Model → `engine_type` / `docker_image` 写入 `ModelLoadRequest` → Gateway 存入 etcd → Scheduler 写入 `PlacementAssignment` → Node reconcile 读取 → `create_engine` 创建对应引擎 → 启动前镜像预检查（`docker images -q`），缺失则 FAILED 并提示用户去 Images 页面注册拉取。

**扩展方式**：新增引擎只需在 `engine/` 下创建新文件实现 Engine trait，在 `create_engine` 工厂注册，在 `args.rs` 加对应参数。

## 8. Week 1-2 具体任务拆解 ✅（全部完成）
- ✅ **Control API 定义**：BFF 统一管理 API，标准错误码与幂等语义。
- ✅ **鉴权与 RBAC**：共享 auth 库（nebula-common），Gateway/BFF/Router 统一 API key + RBAC；CLI `whoami`、`--token` 支持。
- ✅ **可观测性基线**：各组件暴露 Prometheus /metrics（请求 QPS/latency、队列长度、调度结果、节点 GPU 内存/利用率）；结构化 JSON 日志；xtrace OTLP tracing。
- ✅ **CLI 能力**：`metrics`、`logs --follow`、`chat`（流式对话）、`drain`（端点优雅下线）、`scale`（副本调整）、`whoami` 全部完成。
- ✅ **测试与验收**：编译通过，现有测试不回归。

## 9. 控制面优化路线（当前基线）

> 详见 [optimization_plan.md](./optimization_plan.md)

基于当前代码状态，控制面优化已从“补齐基础能力”进入“收敛生产化风险”阶段。详见 [optimization_plan.md](./optimization_plan.md)。

### 当前完成度与剩余风险

| 阶段 | 内容 | 工作量 | 依赖 | 状态 |
|------|------|--------|------|------|
| **1.1 Engine Stats Pipeline** | Node 已采集 engine stats 并推送 xtrace；仍需定稿 etcd `/stats/` 作为 Router/Scheduler 热路径 | 1.5-2 天 | 无 | ⚠️ 需收敛 |
| **1.2 Router 请求级指标** | E2E latency / TTFT histogram，per-model 维度 | 1 天 | 无 | ✅ 已完成 |
| **2.1 路由策略插件化** | RoutingStrategy trait + LeastPending/LeastKvCache/PrefixCacheAware | 2 天 | 1.1 | ✅ 已完成 |
| **3.1 Scheduler 健康自愈** | reconcile loop，endpoint 超时自动清理与副本补充 | 2 天 | 1.1 | ✅ 已完成 |
| **3.2 负载驱动扩缩容** | 基于 kv_cache_usage / pending_requests 自动调整 replicas | 3 天 | 1.1 + 3.1 | ✅ 已完成 |
| **4.1 引擎健康检查** | Node 侧 /health 探测，连续失败标记 Unhealthy，自动 docker restart + 冷却期 | 1 天 | 无 | ✅ 已完成 |
| **4.2 GPU 状态增强** | nvidia-smi 增加 temperature / utilization | 0.5 天 | 无 | ✅ 已完成 |
| **4.3 Docker 容器管理** | 容器复用（Node 重启不杀容器）、正确停止（docker stop）、端口竞争修复 | 1 天 | 无 | ✅ 已完成 |
| **4.4 容器资产感知** | Node HTTP API 暴露容器/镜像信息（/api/containers, /api/images），BFF 按需拉取 | 0.5 天 | 无 | ✅ 已完成 |
| **5.1 Admission Control** | 所有 endpoint 过载时返回 429 + Retry-After | 1 天 | 1.1 | ✅ 已完成 |
| **6.1 可观测性** | 各组件暴露 Prometheus /metrics，日志接入 Loki，Tracing 接入 Jaeger | 3 天 | 无 | ✅ 已完成（Prometheus /metrics + xtrace OTLP） |
| **7.1 安全默认值** | Auth 默认 fail-closed，显式 dev 开关才允许免鉴权 | 0.5 天 | 无 | ✅ 已完成 |
| **7.2 Placement 全路径 CAS** | Scheduler 所有 placement 更新统一 compare-and-swap | 1 天 | 无 | ✅ 已完成 |
| **7.3 Header-driven routing** | Gateway 注入模型 header，Router 避免 full body buffer | 2-3 天 | Gateway/Router 边界 | ⏳ 待做 |

### 建议融入时间线

- **下一轮 P0**：`/stats/` 控制面契约。
- **下一轮 P1**：Gateway 注入模型 header，Router 改为 header-driven streaming proxy。
- **下一轮 P2**：补架构回归测试、统一 BFF telemetry/auth 初始化、完善观测面板。
