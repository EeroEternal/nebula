# 开发文档

## 入门

- [开发环境设置](./setup.md)
- [端到端测试记录（历史快照）](./testing.md)

## 计划与架构（canonical）

- [架构现状](../arch/architecture.md)
- [下一步优化计划 N1–N4](../arch/optimization.md) — N1 主体完成；etcd 三节点暂缓；按需 N3/N4
- [HA 文档](./ha/) — 路线图 / runbook / [2026-07-11 真机报告](./ha/report-20260711.md)

## 控制面与接入

- [API Ownership](./api_ownership.md)
- [Deployments 迁移](./migrate_deployments.md)
- [Engine Stats](./details/stats.md)
- [可观测性](./observability.md) — 设计权威；执行见 arch/optimization **N4-Obs**
- [Loki 日志路径](./loki.md)
- [xtrace 扩展需求](./xtrace_requirements.md)
- [UniGateway 集成](./unigateway_integration.md)
- [Gateway](./gateway/)
- [BFF](./bff/)
- [模型管理](./model_management/)

## 前端与其它

- [前端设计规范](./frontend_design_spec.md)
- [i18n 清单](./i18n_checklist.md)
- [LLM Service Mesh 愿景](./llm_service_mesh.md)
- [xtrace 需求](./xtrace_requirements.md)
- [团队 Memory 笔记](./memory/)
- [历史架构分析归档](../arch/history.md)
