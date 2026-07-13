# 模型管理（Catalog）

控制面写路径：etcd `/models/{uid}/spec` + `/deployments/{uid}`（见 [`../../arch/architecture.md`](../../arch/architecture.md)）。部署视角用 Models 页；本页是**资产视角**（缓存 / 目录检索）。

## 覆盖

1. 已下载模型（缓存 + spec/deployment 聚合）
2. 外部源检索（Hugging Face / ModelScope）并触发下载

不做：自动淘汰、跨节点迁移编排、批量异步任务编排 UI。

## 主流程

下载：检索 → 选源 → 下载 → BFF 写 Spec/缓存任务 → 节点拉取 → 列表刷新。  
删除：确认 → 校验无运行 deployment → 清缓存 → 刷新。  
与部署衔接：Catalog「部署」跳转 Models 流，写 `/deployments/`。

## API

走 BFF `/api/v2/models/*` 与镜像相关接口；owner 见 [`../api_ownership.md`](../api_ownership.md)。
