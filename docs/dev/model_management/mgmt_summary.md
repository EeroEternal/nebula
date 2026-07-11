# Nebula 模型管理体系重设计 — 实施总结

## 概述

本文档总结了 Nebula 模型管理体系从"请求驱动"（ModelRequest）到"声明式实体驱动"（ModelSpec + ModelDeployment + ModelTemplate）的完整重设计实施过程。该项目分 7 个阶段、5 个 Wave 完成，涉及 6 个 Rust crate 和 React 前端的全栈改造。

## 核心变更

### 架构演进

| 维度 | 旧架构 | 新架构 |
|------|--------|--------|
| 核心实体 | ModelRequest（请求驱动） | ModelSpec + ModelDeployment（声明式） |
| 生命周期 | 创建即运行，删除即消失 | 支持 Stop（保留配置）和 Delete（彻底删除） |
| 模型模板 | 无 | ModelTemplate 支持一键部署和保存 |
| 状态管理 | 单一 status 字段 | 聚合计算（Stopped/Downloading/Starting/Running/Degraded/Failed/Stopping） |
| 模型文件 | 引擎自行下载 | Nebula 接管下载，缓存感知，进度上报 |
| 磁盘管理 | 无 | 磁盘状态监控 + 告警（85%/95% 阈值） |
| API 版本 | /api/（v1） | /api/v2/（新增），v1 保留兼容 |

### 新增 etcd Key 布局

```
/models/{model_uid}/spec                    → ModelSpec（持久身份）
/deployments/{model_uid}                    → ModelDeployment（期望运行状态）
/templates/{template_id}                    → ModelTemplate（预设模板）
/model_cache/{node_id}/{model_name_hash}    → ModelCacheEntry（节点模型缓存）
/download_progress/{model_uid}/{replica_id} → DownloadProgress（下载进度，TTL 30s）
/node_disk/{node_id}                        → NodeDiskStatus（节点磁盘状态）
/alerts/{node_id}/{alert_type}              → DiskAlert（磁盘告警）
```

## 实施阶段

### Phase 1：数据层类型定义 ✅

**Crate**: `nebula-common`

新增类型文件：
- `model_spec.rs` — `ModelSpec`、`ModelSource`（HuggingFace/ModelScope/Local）
- `model_deployment.rs` — `ModelDeployment`、`DesiredState`（Running/Stopped）
- `model_template.rs` — `ModelTemplate`、`TemplateCategory`、`TemplateSource`
- `model_cache.rs` — `ModelCacheEntry`、`DownloadProgress`、`DownloadPhase`、`NodeDiskStatus`、`DiskAlert`、`AlertType`

所有类型通过 `lib.rs` 统一导出。

### Phase 2：Scheduler 部署监听 ✅

**Crate**: `nebula-scheduler`

变更文件：
- `main.rs` — 新增 `deployment_watch_loop` 函数，watch `/deployments/` 前缀
  - `desired_state=running` → 读取 ModelSpec → merge config → 生成 PlacementPlan
  - `desired_state=stopped` → 删除 PlacementPlan
  - deployment 被删除 → 删除 PlacementPlan
- `planner.rs` — 新增 `merge_config()`、`build_plan_from_deployment()`、`select_node_and_gpus_for_deployment()`
- `reconcile.rs` — 同时加载 `/deployments/` 和 `/model_requests/` 进行副本数决策

**兼容策略**：过渡期同时 watch 旧 `/model_requests/` 和新 `/deployments/`。

### Phase 3：Node 模型缓存管理器 ✅

**Crate**: `nebula-node`

新增文件：
- `model_cache_manager.rs`（~931 行）— 完整的模型文件管理模块
  - **缓存扫描**：支持 HuggingFace Hub、ModelScope、直接路径三种目录布局
  - **磁盘状态上报**：跨平台（Linux `df -B1`、macOS `df -k` fallback）
  - **磁盘告警**：85% warning、95% critical 阈值
  - **模型下载器**：HuggingFace CLI、ModelScope CLI、本地路径校验
  - **下载进度**：30s TTL 续约，字节级精度
  - **错误处理**：3 次重试，指数退避

变更文件：
- `reconcile.rs` — 引擎启动前调用 `download_model_if_needed()` 预下载模型
- `main.rs` — spawn `model_cache_scan_loop` 后台任务

### Phase 4：BFF v2 API ✅

**Crate**: `nebula-bff`

新增文件：
- `handlers_v2.rs`（~1832 行）— 20 个 v2 API handler

**API 端点**：

| 类别 | 端点 | 说明 |
|------|------|------|
| 模型 CRUD | `POST /api/v2/models` | 创建模型（支持 auto_start） |
| | `GET /api/v2/models` | 列出所有模型（含聚合状态） |
| | `GET /api/v2/models/{uid}` | 模型详情（spec + deployment + endpoints + stats + 下载进度 + 缓存状态） |
| | `PUT /api/v2/models/{uid}` | 更新模型 spec |
| | `DELETE /api/v2/models/{uid}` | 删除模型 |
| 生命周期 | `POST /api/v2/models/{uid}/start` | 启动模型 |
| | `POST /api/v2/models/{uid}/stop` | 停止模型 |
| | `PUT /api/v2/models/{uid}/scale` | 调整副本数 |
| 模板 | `GET /api/v2/templates` | 列出模板 |
| | `GET /api/v2/templates/{id}` | 模板详情 |
| | `POST /api/v2/templates` | 创建模板 |
| | `PUT /api/v2/templates/{id}` | 更新模板 |
| | `DELETE /api/v2/templates/{id}` | 删除模板 |
| | `POST /api/v2/templates/{id}/deploy` | 从模板部署 |
| | `POST /api/v2/models/{uid}/save-as-template` | 保存为模板 |
| 缓存/磁盘 | `GET /api/v2/nodes/{node_id}/cache` | 节点缓存清单 |
| | `GET /api/v2/nodes/{node_id}/disk` | 节点磁盘状态 |
| | `GET /api/v2/cache/summary` | 全集群缓存汇总 |
| | `GET /api/v2/alerts` | 活跃告警 |
| 迁移 | `POST /api/v2/migrate` | v1 → v2 数据迁移 |

**聚合状态计算**：`compute_aggregated_state()` 函数从 ModelDeployment + PlacementPlan + Endpoints + DownloadProgress 聚合计算出 7 种展示状态。

### Phase 5：前端改造 ✅

**目录**: `frontend/src/`

变更文件：
- `lib/types.ts` — 新增 v2 类型：`AggregatedModelState`、`ModelView`、`ModelDetailView`、`ModelSpec`、`ModelDeployment`、`ModelTemplate`、`DownloadProgress`、`NodeDiskStatus`、`DiskAlert` 等
- `lib/api.ts` — 新增 `v2` 命名空间，包含所有 v2 API 调用函数
- `components/views/models.tsx` — 重设计 Models 页面，使用 v2 API 展示聚合状态
- `components/LoadModelDialog.tsx` — 新增"从模板部署"Tab、model_source 选择器

新增文件：
- `components/views/model-detail.tsx`（253 行）— 模型详情页
  - 基本信息（spec）、部署状态、端点列表
  - 下载进度条、缓存状态
  - 快速操作：Start/Stop/Scale/Delete
  - 8 秒自动刷新

变更文件：
- `App.tsx` — 新增 `model-detail` 页面路由和导航

### Phase 6：CLI 扩展 ✅

**Crate**: `nebula-cli`

新增子命令：

```bash
# 模型管理（v2）
nebula model list                              # 列出所有模型（含聚合状态）
nebula model get <model_uid>                   # 模型详情
nebula model create --name <path> [--uid <uid>] [--engine vllm] [--start]
nebula model start <model_uid> [--replicas N]
nebula model stop <model_uid>
nebula model delete <model_uid>
nebula model scale-model <model_uid> --replicas N

# 模板管理
nebula template list
nebula template create --name "..." --model-name <path> --engine vllm
nebula template deploy <template_id> [--uid <uid>] [--replicas N]
nebula template save <model_uid> --name "..."

# 缓存/磁盘
nebula cache list [--node <node_id>]
nebula disk status

# 数据迁移
nebula admin migrate

# 旧命令保留兼容
nebula model load ...
nebula model unload <request_id>
nebula scale --id <id> --replicas N
nebula drain --model-uid <uid> --replica-id N
```

### Phase 7：数据迁移工具 ✅

**变更文件**：
- `crates/nebula-bff/src/handlers_v2.rs` — 新增 `migrate_v1_to_v2` handler
- `crates/nebula-bff/src/main.rs` — 注册 `/migrate` 路由
- `crates/nebula-cli/src/args.rs` — 新增 `Admin` / `Migrate` 子命令
- `crates/nebula-cli/src/main.rs` — 实现 `admin migrate` 命令

**迁移逻辑**：
1. 读取所有 `/model_requests/` 条目
2. 对每个 ModelRequest：
   - 检查 `/models/{model_uid}/spec` 是否已存在（幂等性）
   - 已存在 → 跳过
   - 不存在 → 创建 ModelSpec + ModelDeployment
   - Running/Scheduled → `desired_state: running`
   - 其他状态 → `desired_state: stopped`
3. 返回迁移统计：`{ total, migrated, skipped, failed, details }`

**幂等性保证**：多次运行安全，不会覆盖已有数据。

## 变更文件清单

### 新增文件

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/nebula-common/src/model_spec.rs` | ~69 | ModelSpec、ModelSource 类型 |
| `crates/nebula-common/src/model_deployment.rs` | ~62 | ModelDeployment、DesiredState 类型 |
| `crates/nebula-common/src/model_template.rs` | ~60 | ModelTemplate、TemplateCategory、TemplateSource 类型 |
| `crates/nebula-common/src/model_cache.rs` | ~100 | 缓存、下载进度、磁盘状态、告警类型 |
| `crates/nebula-node/src/model_cache_manager.rs` | ~931 | 模型缓存管理器 |
| `crates/nebula-bff/src/handlers_v2.rs` | ~1832 | v2 API 全部 handler |
| `frontend/src/components/views/model-detail.tsx` | ~253 | 模型详情页 |

### 修改文件

| 文件 | 说明 |
|------|------|
| `crates/nebula-common/src/lib.rs` | 新增模块声明和 re-export |
| `crates/nebula-scheduler/src/main.rs` | 新增 deployment_watch_loop |
| `crates/nebula-scheduler/src/planner.rs` | 新增 deployment 相关 plan 构建函数 |
| `crates/nebula-scheduler/src/reconcile.rs` | 加载 deployments 进行副本决策 |
| `crates/nebula-node/src/main.rs` | spawn model_cache_scan_loop |
| `crates/nebula-node/src/reconcile.rs` | 引擎启动前预下载模型 |
| `crates/nebula-bff/src/main.rs` | 注册 v2 路由 |
| `crates/nebula-cli/src/args.rs` | 新增所有 v2 子命令 |
| `crates/nebula-cli/src/main.rs` | 实现所有 v2 命令 handler |
| `frontend/src/lib/types.ts` | 新增 v2 类型定义 |
| `frontend/src/lib/api.ts` | 新增 v2 API 函数 |
| `frontend/src/components/views/models.tsx` | 重设计 Models 页面 |
| `frontend/src/components/LoadModelDialog.tsx` | 新增模板部署 Tab |
| `frontend/src/App.tsx` | 新增 model-detail 路由 |

## 向后兼容性

- **旧 API 保留**：`/api/` 下的所有 v1 端点不变，`handlers.rs` 未修改
- **旧 CLI 命令保留**：`model load`、`model unload`、`scale`、`drain` 仍可用
- **Scheduler 双路 watch**：同时处理 `/model_requests/` 和 `/deployments/`
- **数据迁移工具**：`nebula admin migrate` 可将旧数据一键转换为新格式
- **前端 v2 API**：前端已切换到 v2 API，但 v1 数据仍可通过旧 API 访问

## 后续工作

以下功能在本次重设计中明确标记为非目标，可作为后续迭代方向：

1. **Rolling Update** — 配置变更时的滚动更新（当前需手动 stop → 修改 → start）
2. **移除旧 API** — 待新 API 稳定后，在下一个大版本中移除 v1 路径
3. **模型版本管理** — 同一 model_uid 的多版本并存
4. **缓存亲和性调度** — 根据节点缓存状态优化调度
5. **磁盘自动清理** — GC/LRU 淘汰策略（当前仅报警）
6. **系统内置模板** — 预置常用模型配置模板

