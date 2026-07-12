# Gateway 优化方案（归档）

> **归档（2026-07-12）：** header/缓冲等已在 Wave C 落地。  
> 现状：[`../../arch/architecture.md`](../../arch/architecture.md)。排期：[`../../arch/optimization.md`](../../arch/optimization.md)。  
> 边界：[`../api_ownership.md`](../api_ownership.md)。Runbook：[`./p0_execution.md`](./p0_execution.md)。

Gateway 职责不变：外部入口、鉴权、审计、协议适配、abort；**不做**实例选择（交给 Router）。
