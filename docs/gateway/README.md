# Gateway 文档索引

本目录用于集中维护 Nebula Gateway 相关方案文档，范围限定为内部调度语义（`gateway + router + etcd`），不引入 provider-domain 抽象。

## 目录

- [Gateway 优化方案](./gateway_optimization_plan.md)
- [Gateway P0 执行 Runbook](./gateway_p0_execution.md)
- [Gateway 可观测性面板规范](../observability.md)（已并入统一可观测文档）
- [Gateway 面板 API 契约](./gateway_panel_api_contract.md)

## 建议后续拆分

1. `gateway_optimization_plan.md`
   - 总体路线、优先级、验收标准（已完成）
2. `gateway_p0_execution.md`
   - P0 任务分解、参数默认值、回滚策略
3. `gateway_p1_design.md`
   - 熔断状态机、候选缩减策略、异常分类
4. `../observability.md`（原 `gateway_observability.md` 已合并）
   - 指标字典、告警阈值、看板建议（统一可观测文档 §5）

## 维护原则

1. 每个改动必须绑定可观测指标。
2. 每个阶段必须有可执行验收用例。
3. 文档变更应与代码变更同 PR 更新。
