# 开发文档

**排期与架构先读：** [`../arch/architecture.md`](../arch/architecture.md)、[`../arch/optimization.md`](../arch/optimization.md)。

## 入门与运维

- [开发环境设置](./setup.md)
- [部署指南（用户向）](../manual/deployment.md)
- [产品定位对齐开发与优化计划](./product_plan.md)（P0–P6 Batch 1 @ v1.3.0）
- [API Ownership](./api_ownership.md)
- [Engine `/stats/` 契约](./details/stats.md)

## 可观测

- [可观测性设计](./observability.md)
- [SLO / 告警 Runbook（O8）](./slo_alerts.md)
- [引擎可观测细节附录](./engine_observability_plan.md)（排期以 product_plan / optimization 为准）
- [Loki 日志路径](./loki.md)

## HA

- [HA 目录](./ha/) — 路线图、runbook、[真机报告](./ha/report-20260711.md)

## 组件短文

- [Gateway / Router](./gateway/)
- [BFF](./bff/)
- [模型 Catalog](./model_management/)
- [UniGateway 边界](./unigateway_integration.md)

## 前端

- [前端设计规范](./frontend_design_spec.md)
- [i18n 清单](./i18n_checklist.md)
