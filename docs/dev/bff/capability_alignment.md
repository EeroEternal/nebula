# BFF 能力对齐

> **归档（2026-07-11）：** 原文按「Scheduler 只 watch `/model_requests/`」编写，已不成立。  
> 现状见 [`../../arch/architecture.md`](../../arch/architecture.md)；边界见 [`../api_ownership.md`](../api_ownership.md)；下一步见 [`../../arch/optimization.md`](../../arch/optimization.md)。

BFF 首期目标（独立管理 API、直连 etcd、不依赖 Gateway 做控制台）仍然成立；实现路径改为 **只写 `/deployments/` + Spec**。
