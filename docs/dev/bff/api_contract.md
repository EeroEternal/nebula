# BFF API 契约

> **状态（2026-07-11）：** 早期「BFF 写 `/model_requests/`」草稿已废弃。  
> 以代码与 [`../api_ownership.md`](../api_ownership.md) 为准：声明式模型管理走 BFF `/api/v2/models/*`（写 Deployment/Spec）；会话走 `/api/auth/*`；观测聚合走 `/api/v2/observability/*`。  
> 部署迁移见 [`../migrate_deployments.md`](../migrate_deployments.md)。

OpenAPI / 字段级契约随代码演进；勿再按旧 `model_requests` 草案实现客户端。
