# Gateway 优化方案（归档）

> **归档（2026-07-11）：** 文中「Router 完整缓冲 body / header 注入待做」等项已在 Wave C / P0-6 落地。  
> 现状：[`../../arch/architecture.md`](../../arch/architecture.md)。下一步：[`../../arch/optimization.md`](../../arch/optimization.md)（N1 HA、N2 工程质量）。  
> 边界：[`../api_ownership.md`](../api_ownership.md)。可执行 runbook：[`./p0_execution.md`](./p0_execution.md)。

Gateway 职责不变：外部入口、鉴权、审计、协议适配、abort；**不做**实例选择（交给 Router）。
