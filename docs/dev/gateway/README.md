# Gateway 文档索引

本目录用于集中维护 Nebula Gateway 相关方案文档，范围限定为内部调度语义（`gateway + router + etcd`），不引入 provider-domain 抽象。

## 目录

- [Gateway 优化方案](./optimization_plan.md)
- [Gateway P0 执行 Runbook](./p0_execution.md)
- [Gateway 可观测性面板规范](../observability.md)（统一可观测文档）
- [Gateway 面板 API 契约](./panel_api_contract.md)

## 维护原则

1. 每个改动必须绑定可观测指标。
2. 每个阶段必须有可执行验收用例。
3. 文档变更应与代码变更同 PR 更新。
