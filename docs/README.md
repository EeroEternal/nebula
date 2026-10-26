# Nebula 文档索引

当前发布：**[v1.10.0](versions/v1.10.0.md)**。

## 产品与运维（`manual/`）

给产品、运维和交付同学：能做什么、日常怎么看、出问题怎么处理。

| 文档 | 说明 |
|------|------|
| [`manual/positioning.md`](manual/positioning.md) | 产品定位与特性：是什么、模块说明、术语、能做什么 / 还在路上 |
| [`manual/module.md`](manual/module.md) | 各功能模块：目录/网关/BFF/监控告警/高可用 |
| [`manual/deployment.md`](manual/deployment.md) | 怎么安装、默认端口、常见问题 |
| [`manual/etcd-ha.md`](manual/etcd-ha.md) | 生产 etcd 三节点：拓扑、迁移要点、演练与排障 |

## 版本（`versions/`）

| 文档 | 说明 |
|------|------|
| [`versions/v1.10.0.md`](versions/v1.10.0.md) | v1.10.0 有界公平准入、单模型多副本、多引擎对齐与 replica_specs 放置 |
| [`versions/v1.9.2.md`](versions/v1.9.2.md) | v1.9.2 前端 console-kit lint 规则落地与本地 CI 门禁收敛 |
| [`versions/v1.9.1.md`](versions/v1.9.1.md) | v1.9.1 文档边界澄清（PowerLLM 兼容层定位）与 Admin 安装脚本修复 |
| [`versions/v1.9.0.md`](versions/v1.9.0.md) | v1.9.0 K8s 执行面控制器、PowerLLM 后端完全兼容与 Admin 控制台整合 |
| [`versions/v1.8.0.md`](versions/v1.8.0.md) | v1.8.0 L2 算力池化（HardwarePool）与整机优雅排空（Node Drain） |
| [`versions/v1.7.0.md`](versions/v1.7.0.md) | v1.7.0 架构升级：业务与资产全量迁入 PostgreSQL，下线单机 Lite |
| [`versions/v1.6.0.md`](versions/v1.6.0.md) | v1.6.0 移除 legacy Admin |
| [`versions/v1.5.0.md`](versions/v1.5.0.md) | v1.5.0 集成 Control API |
| [`versions/v1.3.0.md`](versions/v1.3.0.md) / [`v1.1.0.md`](versions/v1.1.0.md) / [`v0.2.0.md`](versions/v0.2.0.md) | 历史版本说明 |

## Agent 与 UI 规范（console-kit）

给编码 Agent 与前端改动：按需加载，不要整目录塞进上下文。

| 文档 | 说明 |
|------|------|
| [`../AGENTS.md`](../AGENTS.md) | Agent 站立约束与阅读地图（≤80 行） |
| [`design.md`](design.md) | Admin UI 设计入口；章节在 `design/` |
| [`ai/agents/docs-layout.md`](ai/agents/docs-layout.md) | `docs/` 目录生命周期 |
| [`ai/cloud.md`](ai/cloud.md) | 本机/云端环境桥 |
| [`dev/setup.md`](dev/setup.md) | 开发环境 |

## 架构与工程

给研发看的设计与排期。

| 文档 | 说明 |
|------|------|
| [`arch/vision.md`](arch/vision.md) | 统一模型服务平台总纲：定位、能力分层（L0–L4）、原则与演进 |
| [`arch/architecture.md`](arch/architecture.md) | 系统架构（L0 组件级实现） |
| [`arch/ha.md`](arch/ha.md) | HA-first 目标架构与重构路线：多活、共享配额/租约、Recipe 一致性、故障注入验收 |
| [`arch/selection.md`](arch/selection.md) | L3 智能选择层：画像、打分、草稿与半自动落地 |
| [`arch/pool.md`](arch/pool.md) | L2 资源池与 `pd_split`：HardwarePool、池约束、共享 KV 角色化放置/路由与故障注入实测 |
| [`arch/roadmap.md`](arch/roadmap.md) | 排期（与 vision Phase 0–3 对齐） |
| [`arch/data-plane-perf-optimization.md`](arch/data-plane-perf-optimization.md) | 极速数据面优化：嵌入式路由、零拷贝流式代理与引擎感知分流 |
| [`arch/control-plane-adapters.md`](arch/control-plane-adapters.md) | 控制面无关数据面适配契约（草案）：Auth / EndpointDirectory / Policy；`CONTROL_PLANE=none` 默认可单独跑 |
| [`dev/plan.md`](dev/plan.md) | 开发计划 |
| [`dev/setup.md`](dev/setup.md) | 开发环境 |
| [`dev/ownership.md`](dev/ownership.md) | 接口归属 |
| [`dev/etcd.md`](dev/etcd.md) | etcd 放什么 / 不放什么 |
| [`dev/contracts.md`](dev/contracts.md) | L1 一致性契约套件验收条目 |
| [`dev/hotpath-takeover.md`](dev/hotpath-takeover.md) | **网关热路径接管与通用契约架构**：Nebula 与 PowerLLM 等推理系统协同、收益、双模兼容与改造细节 |
| [`dev/integration.md`](dev/integration.md) | **上层平台集成**：Gateway 推理 / Admin 编排 / 鉴权 / trace / ID 对齐 |
| [`dev/integration-plan.md`](dev/integration-plan.md) | Nebula 对外暴露与 Control API 优化计划（Tier A–D，I0–I3） |
| [`dev/openapi-control.yaml`](dev/openapi-control.yaml) | Gateway Control API OpenAPI 现状（I0） |
| [`dev/k8s.md`](dev/k8s.md) | K8s 运行时边界与分阶段方案 |
| [`dev/stats.md`](dev/stats.md) | 引擎状态数据格式 |
| [`dev/unigateway.md`](dev/unigateway.md) | 接口层规范（UG 协议适配 + C5 tooling 门控） |
| [`dev/ha/report.md`](dev/ha/report.md) | 高可用验收报告 |
| [`dev/ha/quota-drill.md`](dev/ha/quota-drill.md) | 集群租户配额实测（issue #68 第一切片） |
| [`dev/ha/railway-drill.md`](dev/ha/railway-drill.md) | 多机（3 台 Railway VM）配额与故障演练：etcd 单节点、Gateway 故障、fail-open 缺陷 |
| [`dev/hw/phase0-pro6000.md`](dev/hw/phase0-pro6000.md) | Phase 0 真机（pro6000 / RTX PRO 6000）验证摘要 |
| [`dev/hw/vllm-serving-perf.md`](dev/hw/vllm-serving-perf.md) | vLLM 服务层（Rust 前端 vs Python）与 Nebula 数据面性能实验（RTX 5090） |
| [`dev/landscape.md`](dev/landscape.md) | 推理编排框架五方对照（`llm-d` / `Dynamo` / `AIBrix` / `Mooncake` / `Nebula`）与能力演进 |
| [`dev/heterogeneous-compute.md`](dev/heterogeneous-compute.md) | **异构算力高效利用与细粒度计算契约方案**：去容器虚拟化包装，聚焦阶段/子阶段计算抽象、Roofline 物理契约与无强耦合协同约定 |
| [`dev/direction.md`](dev/direction.md) | **发展方向与编排边界备忘**：State- & Stage-Aware 定位、MapReduce 五原则、Zone A–C / S1–S3 边界与 2026 趋势映射 |
| [`dev/streaming-cache.md`](dev/streaming-cache.md) | **流式可靠性与 KV 缓存治理现状**：失败契约、运维三件事、pd_split 收益边界、Batch 6 已落地/未落地 |
| [`dev/kv-governance.md`](dev/kv-governance.md) | **企业级 KV Cache 资源化治理（Batch 6）**：把 KV Cache 作为 L2/L3/L4 一等公民资源（声明·隔离·预热·记账）及 Tensormesh 对照 |
| [`dev/assessment.md`](dev/assessment.md) | **范围与设计评估**（2026-10-19，`c0256b1`）：#58–#60 收敛复盘、Router 暖信号与 Gateway 热路径缺口、工程卫生建议 |
| [`dev/benefit-verification.md`](dev/benefit-verification.md) | **收益验证报告（客户端口径）**：前缀亲和在冷启动与暖态过载下的 Goodput / P99 / 拒绝率实测 |
| [`dev/queue-admission.md`](dev/queue-admission.md) | **P0-#2 设计**：引擎级有界公平队列准入（不做 AIMD） |
| [`dev/prefix-affinity.md`](dev/prefix-affinity.md) | **P1 多副本与前缀亲和**：ZMQ KV events、`sha256_cbor` 链式哈希、`chat_template` 对齐与 `nebula-indexd` 全局索引 |
| [`dev/engines.md`](dev/engines.md) | 多引擎对齐（vLLM / SGLang）与 LMCache 共享 KV 选型实测 |
| [`dev/tokenspeed.md`](dev/tokenspeed.md) | **TokenSpeed 架构剖析与 Nebula 演进对标**：Zone A/B 与 Zone C 对标、动态块粒度、远端 `/lookup` 收敛、三类注意力 KV 容量模型与持久化算子编译缓存 |
