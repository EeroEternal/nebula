# Nebula 文档索引

当前发布：**[v1.4.0](versions/v1.4.0.md)**。

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
| [`versions/v1.4.0.md`](versions/v1.4.0.md) | 当前版本更新说明 |
| [`versions/v1.3.0.md`](versions/v1.3.0.md) / [`v1.1.0.md`](versions/v1.1.0.md) / [`v0.2.0.md`](versions/v0.2.0.md) | 历史版本说明 |

## 架构与工程

给研发看的设计与排期。

| 文档 | 说明 |
|------|------|
| [`arch/vision.md`](arch/vision.md) | 统一模型服务平台总纲：定位、能力分层（L0–L4）、原则与演进 |
| [`arch/architecture.md`](arch/architecture.md) | 系统架构（L0 组件级实现） |
| [`arch/selection.md`](arch/selection.md) | L3 智能选择层：画像、打分、草稿与半自动落地 |
| [`arch/pool.md`](arch/pool.md) | L2 资源池：HardwarePool、池约束、拓扑与故障隔离（设计） |
| [`arch/roadmap.md`](arch/roadmap.md) | 排期（与 vision Phase 0–3 对齐） |
| [`dev/plan.md`](dev/plan.md) | 开发计划 |
| [`dev/setup.md`](dev/setup.md) | 开发环境 |
| [`dev/lite.md`](dev/lite.md) | Lite 单机：单进程 vLLM/SGLang，无 etcd/BFF |
| [`dev/ownership.md`](dev/ownership.md) | 接口归属 |
| [`dev/etcd.md`](dev/etcd.md) | etcd 放什么 / 不放什么 |
| [`dev/contracts.md`](dev/contracts.md) | L1 一致性契约套件验收条目 |
| [`dev/integration.md`](dev/integration.md) | **上层平台集成**：Gateway 推理 / Admin 编排 / 鉴权 / trace / ID 对齐 |
| [`dev/integration-plan.md`](dev/integration-plan.md) | Nebula 对外暴露与 Control API 优化计划（Tier A–D，I0–I3） |
| [`dev/k8s.md`](dev/k8s.md) | K8s / HAMi 运行时边界与分阶段方案 |
| [`dev/stats.md`](dev/stats.md) | 引擎状态数据格式 |
| [`dev/unigateway.md`](dev/unigateway.md) | 接口层规范（UG 协议适配 + C5 tooling 门控） |
| [`dev/ha/report.md`](dev/ha/report.md) | 高可用验收报告 |
