# 模型管理（Catalog MVP）

控制面写路径：[`../migrate_deployments.md`](../migrate_deployments.md)。部署视角继续用现有 Models 页；本页是**资产视角**。

## 定位

菜单位置建议放在 `More`，与 `Models` 并列。覆盖：

1. 已下载模型（缓存 + spec/deployment 聚合）  
2. 外部源检索（Hugging Face / ModelScope）并触发下载  

不做：自动淘汰、跨节点迁移编排、批量异步任务编排 UI。

## 信息架构

**区块 A — 已下载：** 名称、UID、来源、revision、大小、节点分布、最后使用、状态。操作：刷新、删除（确认）、详情。  

**区块 B — 源检索：** 模型 ID、来源、任务类型、大小/热度（若有）。操作：搜索、下载、看元数据。

## 主流程

下载：检索 → 选源 → 下载 → BFF 写 Spec/缓存任务 → 节点拉取 → 列表刷新。  
删除：确认 → 校验无运行 deployment → 清缓存 → 刷新。  
与部署衔接：Catalog「部署」跳转现有 Models/部署流，写 `/deployments/`。

## API

走 BFF `/api/v2/models/*` 与镜像相关接口；owner 见 [`../api_ownership.md`](../api_ownership.md)。
