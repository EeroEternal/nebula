# Nebula Model Catalog 页面设计（MVP）

## 1. 目标与范围

目标：提供一个单独的模型管理页面，统一管理以下两类信息与操作：

1. 已下载模型（集群内已存在的模型文件与可运行模型）
2. 外部模型源（Hugging Face / ModelScope）的可检索模型

本设计仅覆盖 MVP，强调“可用优先、最少必要能力”，不引入复杂工作流（如自动淘汰、跨节点迁移编排、批量异步任务编排 UI）。

---

## 2. 页面定位

页面名称定稿：`Model Catalog`（中文可展示为“模型目录”）

菜单位置建议：
- 放在 `More` 菜单内，避免主导航拥挤
- 与现有 `Models` 页面并列，不替代 `Models`

定位：
- 运维入口：查看当前可用模型资产与状态
- 分发入口：从 Hugging Face / ModelScope 拉取新模型
- 清理入口：对无用模型执行删除/回收

与现有 Models 页关系：
- 现有 Models 页继续承载“部署视角”（副本、运行状态、端点）。
- 新页面承载“资产视角”（模型文件、来源、缓存、下载管理）。

---

## 3. 信息架构（MVP）

采用单页双区块结构：

## 3.1 区块 A：已下载模型

展示集群已存在模型（来自缓存汇总 + model spec/deployment 聚合）。

建议字段（表格）：
- 模型名称（model_name）
- 模型 UID（model_uid，可为空：仅缓存尚未注册）
- 来源（huggingface / modelscope / local）
- 版本/修订（revision，可为空）
- 总大小（bytes，前端格式化）
- 节点分布（节点数 + 节点列表摘要）
- 最后使用时间（last_used_at，可为空）
- 当前状态（stopped/downloading/starting/running/degraded/failed）

MVP 操作：
- 刷新状态
- 删除模型（需要确认）
- 进入详情（跳转现有 model-detail 或本页侧栏详情）

## 3.2 区块 B：模型源检索（Hugging Face / ModelScope）

支持在两个来源中检索并查看基础信息。

建议字段（列表）：
- 模型 ID（如 `Qwen/Qwen2.5-7B-Instruct`）
- 来源（HF / MS）
- 任务类型（text-generation / embedding 等）
- 估算大小（若可获取）
- 热度指标（downloads/likes，若可获取）
- 最近更新时间（若可获取）

MVP 操作：
- 下载（触发创建模型 + 拉取）
- 查看基础元数据

---

## 4. 核心交互流程

## 4.1 从源模型下载到集群

1. 用户在“模型源检索”输入关键字
2. 选择来源（HF 或 ModelScope）并点击搜索
3. 在结果中点击“下载”
4. 弹出最小参数表单：
   - model_uid（可自动生成）
   - engine_type（默认 vllm）
   - replicas（默认 1）
   - auto_start（默认 true）
5. 提交后：
   - 创建/更新 ModelSpec + ModelDeployment
   - 页面自动切换到“已下载模型”并显示下载进度与状态

## 4.2 删除已下载模型

1. 用户在“已下载模型”点击删除
2. 二次确认弹窗（显示将删除的模型名与影响）
3. 后端执行删除（spec/deployment/cache 清理策略按后端实现）
4. 列表刷新并反馈结果

## 4.3 查看已下载模型状态

1. 页面默认每 8-15 秒自动刷新（建议 10 秒）
2. 对于 `downloading` 状态显示进度条
3. 对于 `failed` 状态显示错误摘要（若后端可提供）

---

## 5. API 设计建议（在现有 v2 基础上最小扩展）

优先复用已有接口：
- `GET /api/v2/models`
- `GET /api/v2/models/{uid}`
- `POST /api/v2/models`
- `DELETE /api/v2/models/{uid}`
- `GET /api/v2/cache/summary`
- `GET /api/v2/nodes/{node_id}/cache`

建议新增（MVP）：

1. `GET /api/v2/catalog/search`
- query 参数：
  - `provider=hf|modelscope`
  - `q=<keyword>`
  - `page`
  - `limit`
- 返回：标准化模型目录项列表（id/source/task/size/downloads/updated_at）

2. `POST /api/v2/catalog/import`
- 请求体：
  - `provider`
  - `model_id`
  - `model_uid`（可选）
  - `engine_type`
  - `replicas`
  - `auto_start`
- 语义：将目录项转换为 ModelSpec/Deployment（等价于“从目录快速创建”）

说明：
- 若暂不新增 `catalog/import`，前端也可先走现有 `POST /api/v2/models`（将选中项映射为 model_name + model_source）。

---

## 6. 前端实现建议（MVP）

建议新增视图：
- `frontend/src/components/views/model-catalog.tsx`

建议新增 API 封装：
- `frontend/src/lib/api.ts` 中增加 `catalog` 命名空间

建议新增类型：
- `frontend/src/lib/types.ts`
  - `CatalogProvider`
  - `CatalogModelItem`
  - `DownloadedModelRow`（聚合展示类型）

路由建议：
- 在导航中新增 `model-catalog`
- 入口放在 `More` 菜单，文案使用 `Model Catalog`
- 与现有 `models` 并列，不替换现有页面

---

## 7. 权限与安全

建议 RBAC：
- viewer：查看已下载模型与目录检索
- operator：可触发下载/导入
- admin：可删除模型

安全约束：
- 输入校验：`model_uid` 格式限制（沿用现有约束）
- 防 SSRF：目录查询仅允许预置 provider，不允许任意 URL
- 审计：导入/删除操作写入 audit logs

---

## 8. 非功能要求

- 性能：
  - 列表分页（默认 20）
  - 搜索请求防抖（300-500ms）
- 可观测性：
  - 关键操作（search/import/delete）埋点
  - 后端失败原因可追踪到 request id
- 兼容性：
  - 无 catalog 能力时，页面降级为“仅已下载模型”模式

---

## 9. 分阶段实施计划

Phase 1（1-2 天，纯前端聚合）
- 新增页面框架
- 接入 `models + cache summary` 展示“已下载模型”
- 支持刷新、删除、跳详情

Phase 2（2-3 天，目录检索）
- 新增 `catalog/search` 后端接口
- 前端增加 HF / ModelScope 搜索区块

Phase 3（1-2 天，快速导入）
- 新增 `catalog/import` 或前端映射到 `POST /models`
- 打通下载流程与进度反馈

---

## 10. MVP 验收标准

1. 能在单页看到已下载模型清单与状态
2. 能检索 Hugging Face / ModelScope 模型并展示结果
3. 能从检索结果触发下载并看到状态变化
4. 能删除模型并在列表中消失
5. 所有写操作有权限控制与审计记录

---

## 11. 后续增强（非 MVP）

- 多选批量操作（批量下载/删除）
- 下载任务队列与并发控制 UI
- 模型版本对比与升级提示
- 节点级缓存占用可视化
- 自动清理策略（按最近使用时间/阈值）
