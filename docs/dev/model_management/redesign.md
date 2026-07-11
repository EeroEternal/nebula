# Nebula 模型管理体系重设计

## Goal

将 Nebula 的模型管理从"请求驱动"（ModelRequest）演进为"声明式实体驱动"（ModelSpec + ModelDeployment + ModelTemplate），实现持久化模型目录、Stop/Delete 双语义、模型预设模板、聚合状态查询、模型文件管理（接管下载 + 缓存感知 + 下载进度 + 磁盘报警）。

## 设计决策记录

| 问题 | 决策 |
|------|------|
| 核心实体 | 方案 A：独立 Model 实体作为期望状态声明 |
| 配置更新策略 | 暂不需要 Rolling Update，手动 stop → 修改 → start |
| 生命周期语义 | Stop（保留配置）+ Delete（彻底删除）两者都需要 |
| 模型预设模板 | 非常需要，作为核心能力设计 |
| 设计范围 | 全面重设计模型管理体系 |
| 模型文件来源 | HuggingFace + ModelScope + 本地路径 |
| 模型文件管理粒度 | 方案 A：Nebula 接管下载，引擎启动前确保模型文件就绪 |
| 缓存亲和性调度 | 不需要 |
| 磁盘空间管理 | 只做感知+报警，不做 GC/淘汰等自动操作 |
| 下载进度 | 精确优先，粗略也可接受 |
| 模型文件管理架构 | 独立的 model_cache_manager，不与 image_manager 合并 |

---

## 1. 核心实体设计

### 1.1 ModelSpec（模型规格 — 持久化身份）

模型的"身份证"，一旦创建就持久存在，直到用户显式删除。独立于运行状态。

```
etcd key: /models/{model_uid}/spec
```

```json
{
  "model_uid": "qwen2-5-7b-instruct",
  "model_name": "Qwen/Qwen2.5-7B-Instruct",
  "model_source": "huggingface",
  "model_path": null,
  "engine_type": "vllm",
  "docker_image": "vllm/vllm-openai:v0.8.3",
  "config": {
    "tensor_parallel_size": 2,
    "gpu_memory_utilization": 0.9,
    "max_model_len": 8192,
    "required_vram_mb": 16000,
    "lora_modules": []
  },
  "labels": { "team": "nlp", "priority": "high" },
  "created_at_ms": 1700000000000,
  "updated_at_ms": 1700000000000,
  "created_by": "admin"
}
```

**设计要点**：
- `model_uid` 是全局唯一标识。支持用户指定（需校验格式：`[a-z0-9][a-z0-9-]*`，最长 63 字符）或系统自动生成（从 model_name 转化，冲突时追加后缀）
- `model_name` 是用户可读的模型路径（如 HuggingFace ID）
- `model_source`：模型文件来源，`huggingface` | `modelscope` | `local`。决定 Node 用哪种方式下载
- `model_path`：仅 `local` 来源时使用，指定模型文件在节点上的绝对路径（如 `/DATA/Model/Qwen2.5-7B-Instruct/`）。`huggingface`/`modelscope` 来源时为 null，由 `model_name` 推断下载路径
- `config` 记录默认推理配置，是 Deployment 的"兜底值"
- `labels` 支持自由 key-value 标签，便于前端分组和筛选
- 创建后 model_uid 不可变，其他字段可更新

### 1.2 ModelDeployment（模型部署 — 期望运行状态）

声明"这个模型应该以什么姿态运行"。类似 K8s Deployment 的 spec 部分。

```
etcd key: /deployments/{model_uid}
```

```json
{
  "model_uid": "qwen2-5-7b-instruct",
  "desired_state": "running",
  "replicas": 2,
  "min_replicas": 1,
  "max_replicas": 4,
  "node_affinity": null,
  "gpu_affinity": null,
  "config_overrides": {
    "gpu_memory_utilization": 0.95
  },
  "version": 1700000000000,
  "updated_at_ms": 1700000000000
}
```

**设计要点**：
- `desired_state`：`running` | `stopped`（只有两个用户可写的值）
- `config_overrides`：覆盖 ModelSpec 中的默认 config 字段，采用 merge 语义（仅覆盖指定字段，未指定的 fallback 到 ModelSpec.config）
- `node_affinity` / `gpu_affinity`：可选的硬件绑定。null 表示由 Scheduler 自动选择
- `version`：单调递增，每次变更 bump。Scheduler 用于判断是否需要 re-plan
- **ModelDeployment 只在 desired_state=running 时存在**，stop 后保留（desired_state=stopped），delete 时删除

### 1.3 ModelTemplate（模型预设模板）

可复用的模型配置模板，支持一键部署。

```
etcd key: /templates/{template_id}
```

```json
{
  "template_id": "qwen2-5-7b-vllm-tp2",
  "name": "Qwen2.5-7B (vLLM, TP=2)",
  "description": "适合双卡部署的 Qwen2.5-7B 配置",
  "category": "llm",
  "model_name": "Qwen/Qwen2.5-7B-Instruct",
  "model_source": "huggingface",
  "engine_type": "vllm",
  "docker_image": "vllm/vllm-openai:v0.8.3",
  "config": {
    "tensor_parallel_size": 2,
    "gpu_memory_utilization": 0.9,
    "max_model_len": 8192,
    "required_vram_mb": 16000
  },
  "default_replicas": 1,
  "labels": { "size": "7B", "lang": "multilingual" },
  "source": "user",
  "created_at_ms": 1700000000000,
  "updated_at_ms": 1700000000000
}
```

**设计要点**：
- `template_id`：全局唯一（用户指定或自动生成）
- `source`：`system`（内置预设）| `user`（用户自建）| `saved`（从运行中模型保存）
- `category`：`llm` | `embedding` | `rerank` | `vlm` | `audio`，用于前端分组展示
- 从模板部署 = 用模板字段填充 ModelSpec + 创建 ModelDeployment(running)
- 从运行中模型保存为模板 = 读取当前 ModelSpec + 生效配置 → 写入 Template

### 1.4 已有实体的角色变化

| 实体 | 当前角色 | 新角色 |
|------|---------|--------|
| `ModelRequest` | 核心驱动实体 | **降级为内部事件**：Scheduler 内部消费的操作事件，不对外暴露 |
| `PlacementPlan` | Scheduler 输出 | **不变**：仍然是 Scheduler 写入的放置计划 |
| `EndpointInfo` | Node 运行时状态 | **不变**：仍然是 Node 上报的端点信息 |
| `EndpointStats` | 引擎指标 | **不变**：仍然是引擎指标采集 |

---

## 2. 模型生命周期状态机

```
                    ┌──────────────────────────────────────┐
                    │                                      │
    ┌───────┐   create    ┌──────────┐   start    ┌──────────────┐
    │ (none)│ ──────────► │ Stopped  │ ─────────► │ Downloading  │
    └───────┘             └──────────┘            └──────────────┘
                            ▲    ▲                      │
                    stop    │    │  start failed         │ 模型文件就绪（已缓存或下载完成）
                    done    │    │                       ▼
                            │    │               ┌────────────┐
                            │    │               │  Starting   │ (引擎进程启动中)
                            │    │               └────────────┘
                            │    │                      │
                            │    │                      │ Endpoints 注册
                            │    │                      ▼
                    ┌────────────┐              ┌────────────┐
                    │  Stopping  │ ◄─────────── │  Running   │
                    └────────────┘    stop      └────────────┘
                                                     │
                                                     │ health check
                                                     ▼
                                                ┌────────────┐
                                                │  Degraded  │ (部分 replica unhealthy)
                                                └────────────┘

    任何状态 ──── delete ───► 彻底清除所有 etcd 数据
```

**状态定义**：

| 状态 | 含义 | etcd 数据 |
|------|------|-----------|
| **Stopped** | 模型已注册但未运行 | ModelSpec ✅ ModelDeployment(stopped) ✅ PlacementPlan ✗ |
| **Downloading** | Node 正在主动下载模型文件 | ModelSpec ✅ ModelDeployment(running) ✅ PlacementPlan ✅ DownloadProgress 存在 |
| **Starting** | 引擎进程启动中 | ModelSpec ✅ ModelDeployment(running) ✅ PlacementPlan ✅ Endpoints 注册中 |
| **Running** | 所有期望副本就绪 | ModelSpec ✅ ModelDeployment(running) ✅ PlacementPlan ✅ Endpoints Ready |
| **Degraded** | 部分副本不健康 | 同 Running，但部分 Endpoint 状态为 Unhealthy |
| **Stopping** | 正在停止中 | PlacementPlan 被删除，Endpoints 陆续下线 |
| **Failed** | 启动失败 | ModelSpec ✅ ModelDeployment(running) ✅，但 Endpoints 未注册成功 |

**关键语义**：
- **Stop**：设置 `desired_state=stopped` → Scheduler 删除 PlacementPlan → Node 停止引擎 → ModelSpec 和 ModelDeployment 保留
- **Delete**：删除 ModelSpec + ModelDeployment + PlacementPlan + 清理 Endpoints
- **Start**：设置 `desired_state=running` → Scheduler watch 到变更 → 生成 PlacementPlan
- **实际状态是计算得出的**：BFF/前端通过聚合 ModelDeployment.desired_state + PlacementPlan + Endpoints 计算出展示状态（Running/Degraded/Starting 等），不额外存一个 `actual_state` 字段，避免一致性问题

---

## 3. etcd Key 布局

```
/models/{model_uid}/spec                    → ModelSpec（持久身份）
/deployments/{model_uid}                    → ModelDeployment（期望运行状态）
/placements/{model_uid}                     → PlacementPlan（Scheduler 产出，不变）
/endpoints/{model_uid}/{replica_id}         → EndpointInfo（Node 上报，不变）
/stats/{model_uid}/{replica_id}             → EndpointStats（引擎指标，不变）
/templates/{template_id}                    → ModelTemplate（预设模板）
/model_cache/{node_id}/{model_name_hash}    → ModelCacheEntry（节点模型缓存清单）
/download_progress/{model_uid}/{replica_id} → DownloadProgress（下载进度，临时）
/node_disk/{node_id}                        → NodeDiskStatus（节点磁盘状态）

# 兼容期保留，后续废弃
/model_requests/{request_id}                → ModelRequest（降级为内部事件）
```

---

## 4. Scheduler 变更

当前 Scheduler 的核心循环：`watch /model_requests/ → build plan → write /placements/`

**新设计**：

```
主循环 watch /deployments/ 前缀
  ├── deployment 出现/变更 且 desired_state=running
  │     → 读取 /models/{model_uid}/spec 获取完整配置
  │     → merge config_overrides
  │     → build PlacementPlan → 写入 /placements/{model_uid}
  │
  ├── deployment 变更 且 desired_state=stopped
  │     → 删除 /placements/{model_uid}
  │
  └── deployment 被删除
        → 删除 /placements/{model_uid}

reconcile loop（已有，小调整）
  └── 检查 PlacementPlan 是否有对应的 running deployment
      → 如果没有（孤儿 placement），清理
```

**兼容策略**：过渡期同时 watch `/model_requests/` 和 `/deployments/`。BFF 新 API 写 `/deployments/`，旧 API 继续写 `/model_requests/`。当前端和 CLI 全部迁移后，移除旧路径。

---

## 5. BFF API 设计

### 5.1 模型 CRUD

| 方法 | 路径 | 说明 | 角色 |
|------|------|------|------|
| `GET` | `/api/v2/models` | 列出所有模型（含聚合状态） | viewer |
| `GET` | `/api/v2/models/{model_uid}` | 模型详情（spec + deployment + endpoints + stats） | viewer |
| `POST` | `/api/v2/models` | 创建模型（注册 spec，可选 auto_start） | operator |
| `PUT` | `/api/v2/models/{model_uid}` | 更新模型 spec（引擎、配置等） | operator |
| `DELETE` | `/api/v2/models/{model_uid}` | 删除模型（彻底清除） | admin |

**POST /api/v2/models 请求体**：
```json
{
  "model_name": "Qwen/Qwen2.5-7B-Instruct",
  "model_uid": "qwen2-5-7b",          // 可选，不填则自动生成
  "model_source": "huggingface",       // "huggingface" | "modelscope" | "local"
  "model_path": null,                   // 仅 local 来源时必填
  "engine_type": "vllm",
  "docker_image": null,                 // 可选
  "config": { ... },
  "labels": {},
  "auto_start": true,                   // 创建后自动启动
  "replicas": 1,                        // auto_start=true 时的副本数
  "node_id": null,                      // 可选硬件绑定
  "gpu_indices": null                   // 可选 GPU 绑定
}
```

**GET /api/v2/models 响应体**（聚合视图）：
```json
[
  {
    "model_uid": "qwen2-5-7b-instruct",
    "model_name": "Qwen/Qwen2.5-7B-Instruct",
    "engine_type": "vllm",
    "state": "running",
    "replicas": { "desired": 2, "ready": 2, "unhealthy": 0 },
    "endpoints": [
      { "replica_id": 0, "node_id": "node-1", "status": "ready", "base_url": "..." },
      { "replica_id": 1, "node_id": "node-1", "status": "ready", "base_url": "..." }
    ],
    "labels": { "team": "nlp" },
    "created_at_ms": 1700000000000,
    "updated_at_ms": 1700000000000
  }
]
```

### 5.2 模型生命周期控制

| 方法 | 路径 | 说明 | 角色 |
|------|------|------|------|
| `POST` | `/api/v2/models/{model_uid}/start` | 启动已停止的模型 | operator |
| `POST` | `/api/v2/models/{model_uid}/stop` | 停止模型（保留配置） | operator |
| `PUT` | `/api/v2/models/{model_uid}/scale` | 调整副本数 | operator |

**POST /api/v2/models/{model_uid}/start 请求体**（可选覆盖）：
```json
{
  "replicas": 2,                        // 可选，不填用上次的值
  "config_overrides": ,               // 可选
  "node_id": null,
  "gpu_indices": null
}
```

### 5.3 模板 CRUD

| 方法 | 路径 | 说明 | 角色 |
|------|------|------|------|
| `GET` | `/api/v2/templates` | 列出所有模板 | viewer |
| `GET` | `/api/v2/templates/{id}` | 模板详情 | viewer |
| `POST` | `/api/v2/templates` | 创建模板 | operator |
| `PUT` | `/api/v2/templates/{id}` | 更新模板 | operator |
| `DELETE` | `/api/v2/templates/{id}` | 删除模板 | operator |
| `POST` | `/api/v2/templates/{id}/deploy` | 从模板部署 | operator |
| `POST` | `/api/v2/models/{model_uid}/save-as-template` | 从运行中模型保存为模板 | operator |

**POST /api/v2/templates/{id}/deploy 请求体**：
```json
{
  "model_uid": "my-qwen",              // 可选，不填则自动生成
  "replicas": 2,                        // 可选，不填用模板默认值
  "config_overrides": ,               // 可选，覆盖模板配置
  "node_id": null,
  "gpu_indices": null
}
```

### 5.4 兼容旧 API

旧的 `/api/models/load`、`/api/models/requests/:id` 等在过渡期保留，内部转换为新实体写入。

---

## 6. 前端变更概要

### 6.1 Models 页面改造

**当前**：显示 model_requests 列表，状态来自 request.status。
**新版**：
- 显示 ModelSpec 列表，状态通过聚合计算
- 每个模型卡片显示：名称、引擎、状态指示灯、副本数（desired/ready）、标签
- 操作按钮根据状态变化：Running → [Stop] [Scale]，Stopped → [Start] [Delete] [Edit]
- 支持按 label/state/engine_type 过滤

### 6.2 Load Model Dialog 改造

**当前**：Search → Configure → Submit（一步到位创建并启动）。
**新版**：
- 新增"从模板部署"入口（与 Search 并列）
- 模板列表按 category 分组，显示预设配置
- 选择模板后预填配置，用户可覆盖
- 新增"仅注册不启动"选项（auto_start toggle）
- 支持"保存为模板"按钮

### 6.3 Model Detail 页面（新增）

点击模型名称进入详情页：
- 基本信息（spec）
- 部署状态（deployment + endpoint 列表）
- 实时指标（从 engine stats 获取）
- 操作历史（从审计日志获取）
- 快速操作：Start/Stop/Scale/Edit/Save as Template

---

## 7. CLI 变更概要

```bash
# 模型管理（新增）
nebula model list                              # 列出所有模型（含状态）
nebula model get <model_uid>                   # 模型详情
nebula model create --name <path> [--uid <uid>] [--engine vllm] [--start]
nebula model start <model_uid> [--replicas 2]
nebula model stop <model_uid>
nebula model delete <model_uid>
nebula model scale <model_uid> --replicas 3

# 模板管理（新增）
nebula template list
nebula template create --name "..." --model-name <path> --engine vllm ...
nebula template deploy <template_id> [--uid <uid>] [--replicas 2]
nebula template save <model_uid>               # 从运行中模型保存

# 保持兼容
nebula model load ...                          # 旧命令，内部转为 create + start
nebula model unload <request_id>               # 旧命令，内部转为 stop 或 delete
```

---

## 8. 数据迁移策略

对于已有集群中 `/model_requests/` 下的数据：

1. **Scheduler 双路 watch**：过渡期同时处理旧 `/model_requests/` 和新 `/deployments/`
2. **BFF 迁移命令**：提供 `POST /api/v2/migrate` 一次性将旧 model_requests 转换为 ModelSpec + ModelDeployment
3. **前端版本检测**：前端检测 BFF 是否支持 v2 API，有则用新 API，无则 fallback 旧 API
4. **时间线**：新 API 稳定后，下一个大版本移除旧路径

---

## 9. 聚合状态计算规则

BFF 在返回模型列表/详情时，需要聚合多个 etcd 前缀的数据来计算展示状态：

```
输入：
  spec = get /models/{uid}/spec
  deployment = get /deployments/{uid}
  placement = get /placements/{uid}
  endpoints = list /endpoints/{uid}/
  stats = list /stats/{uid}/

计算规则：
  if spec 不存在 → 模型不存在（404）
  if deployment 不存在 或 deployment.desired_state == "stopped" → state = Stopped
  if deployment.desired_state == "running":
    if placement 不存在 → state = Starting（等待 Scheduler）
    if download_progress 存在 且 未完成 → state = Downloading（附带进度信息）
    if endpoints 全部 ready → state = Running
    if endpoints 部分 ready → state = Degraded
    if endpoints 全部不存在 且 创建超过阈值 → state = Failed
    else → state = Starting

副本计数：
  desired = deployment.replicas
  ready = count(endpoints where status == Ready)
  unhealthy = count(endpoints where status == Unhealthy)
  starting = desired - ready - unhealthy
```

---

## 10. 非目标（本次不做）

- Rolling Update（配置变更时的滚动更新）
- 模型版本管理（同一 model_uid 的多个版本并存）
- 自建模型仓库/Registry（使用 HuggingFace Hub / ModelScope 现有生态）
- 缓存亲和性调度（不根据缓存选节点）
- 磁盘 GC/LRU 淘汰（只报警，不自动清理）
- 模型权重预热/推送
- A/B 测试 / 金丝雀发布
- 跨集群模型同步

---

## 11. 实施阶段建议

| 阶段 | 内容 | 依赖 |
|------|------|------|
| **Phase 1：数据层** | ModelSpec / ModelDeployment / ModelTemplate / ModelCacheEntry / DownloadProgress / NodeDiskStatus 类型定义（nebula-common）；etcd 读写逻辑 | 无 |
| **Phase 2：Scheduler** | 新增 watch `/deployments/` 路径；兼容旧路径 | Phase 1 |
| **Phase 3：Node 模型下载** | model_cache_manager 模块：模型下载器（HF/ModelScope/local）+ 缓存扫描 + 磁盘状态上报 + 下载进度上报；reconcile 流程改造（下载 → 启动引擎） | Phase 1 |
| **Phase 4：BFF API** | 实现 v2 模型 API + 模板 API；聚合状态计算（含 Downloading 状态和进度）；磁盘报警 API | Phase 1 + Phase 3 |
| **Phase 5：前端** | Models 页面改造；模板选择 UI；Model Detail 页（含下载进度条、缓存信息、磁盘状态） | Phase 4 |
| **Phase 6：CLI** | 新 model / template / cache 子命令 | Phase 4 |
| **Phase 7：迁移** | 数据迁移工具；废弃旧 API | Phase 2-6 |

---

## 12. 模型文件管理（Model File Management）

### 12.1 设计原则

- **Nebula 接管下载**：Node 在引擎启动前主动下载模型文件，确保模型就绪后再启动引擎（类似 image_manager 对 Docker 镜像的管理）
- **下载与引擎解耦**：下载阶段和引擎启动阶段分离，下载失败不会产生引擎孤儿进程
- **精确进度控制**：因为是 Nebula 自己在下载，可以精确上报进度（字节级）
- **Nebula 做感知**：Node 上报"本地缓存了哪些模型"和"磁盘空间状态"，BFF 聚合展示
- **Nebula 做报警**：磁盘空间不足时产生告警事件，不做自动 GC/淘汰

### 12.2 核心数据结构

#### ModelCacheEntry（节点模型缓存条目）

```
etcd key: /model_cache/{node_id}/{model_name_hash}
```

```json
{
  "node_id": "node-1",
  "model_name": "Qwen/Qwen2.5-7B-Instruct",
  "cache_path": "/DATA/Model/.cache/huggingface/hub/models--Qwen--Qwen2.5-7B-Instruct",
  "size_bytes": 15032000000,
  "file_count": 12,
  "complete": true,
  "last_accessed_ms": 1700000000000,
  "discovered_at_ms": 1700000000000
}
```

**字段说明**：
- `model_name_hash`：model_name 的 SHA256 前 16 位，用作 etcd key 的安全路径
- `cache_path`：在节点上的实际路径
- `size_bytes`：模型文件总大小
- `file_count`：文件数量
- `complete`：是否完整下载（通过检查是否存在 `config.json` 或 `model.safetensors.index.json` 判断）
- `last_accessed_ms`：最后访问时间（文件系统 atime 或推理引擎最后加载时间）

#### DownloadProgress（下载进度 — 临时数据）

```
etcd key: /download_progress/{model_uid}/{replica_id}
TTL: 30s（自动过期，需持续续约）
```

```json
{
  "model_uid": "qwen2-5-7b-instruct",
  "replica_id": 0,
  "node_id": "node-1",
  "model_name": "Qwen/Qwen2.5-7B-Instruct",
  "phase": "downloading",
  "total_bytes": 15032000000,
  "downloaded_bytes": 7500000000,
  "progress_pct": 49.9,
  "speed_bytes_per_sec": 125000000,
  "eta_seconds": 60,
  "files_total": 12,
  "files_done": 6,
  "updated_at_ms": 1700000000000
}
```

**字段说明**：
- `phase`：`downloading` | `verifying` | `complete` | `failed`
- 进度信息精确到字节级（Nebula 控制下载过程），fallback 到粗粒度：
  - **精确**：解析 huggingface-cli / modelscope CLI 的 stdout 获取 bytes 级进度
  - **粗略**：通过文件系统轮询统计已下载文件大小（files_done / files_total）
  - **最粗**：只有 phase 变化（downloading → complete）
- TTL 机制：进度数据带 30s TTL，Node 每次更新时续约。如果 Node 异常，数据自动过期消失

#### NodeDiskStatus（节点磁盘状态）

```
etcd key: /node_disk/{node_id}
```

```json
{
  "node_id": "node-1",
  "model_dir": "/DATA/Model",
  "total_bytes": 2000000000000,
  "used_bytes": 1500000000000,
  "available_bytes": 500000000000,
  "usage_pct": 75.0,
  "model_cache_bytes": 300000000000,
  "model_count": 8,
  "updated_at_ms": 1700000000000
}
```

### 12.3 Node 侧实现：model_cache_manager

独立于 `image_manager.rs` 的新模块 `model_cache_manager.rs`，包含两个职责：

#### A. 后台缓存扫描循环（Node 启动时 spawn）

```
model_cache_scan_loop (主循环，每 60s)
  ├── scan_model_cache()
  │     扫描 model_dir（默认 /DATA/Model）下的缓存目录结构
  │     支持两种目录布局：
  │       - HuggingFace Hub: .cache/huggingface/hub/models--{org}--{name}/
  │       - ModelScope: .cache/modelscope/hub/{org}/{name}/
  │       - 直接路径: model_dir/{org}/{name}/ 或 model_dir/{name}/
  │     计算每个模型的文件大小、完整性
  │     写入 etcd /model_cache/{node_id}/{hash}
  │
  ├── report_disk_status()
  │     读取 model_dir 所在分区的磁盘使用情况（statvfs）
  │     写入 etcd /node_disk/{node_id}
  │     如果 usage_pct > 阈值（默认 85%）→ 写入告警事件
  │
  └── clean_stale_entries()
        删除已不存在的缓存条目
```

#### B. 模型下载器（引擎启动前调用）

Node 的 reconcile 流程变更：

```
当前流程：
  watch placement → 直接启动引擎（引擎自行下载模型）

新流程：
  watch placement → 检查本地缓存 → 如需下载则先下载 → 下载完成后再启动引擎

详细步骤：
  1. 收到 PlacementAssignment
  2. 检查本地是否已有模型文件（查 /model_cache/{node_id}/ 或直接检查文件系统）
  3. 如果已有且完整 → 跳过下载，直接启动引擎
  4. 如果不存在或不完整 → 进入下载阶段：
     a. 写入 DownloadProgress (phase=downloading) 到 etcd
     b. 调用下载器下载模型文件
     c. 持续更新 DownloadProgress（字节级进度）
     d. 下载完成 → 更新 DownloadProgress (phase=complete)
     e. 更新 /model_cache/{node_id}/{hash}
  5. 启动引擎（此时模型文件已在本地，引擎无需再下载）
```

#### C. 下载器实现

```
ModelDownloader
  ├── 支持的来源：
  │     - HuggingFace Hub（通过 huggingface-hub CLI 或 HTTP API）
  │     - ModelScope（通过 modelscope CLI 或 HTTP API）
  │     - 本地路径（无需下载，只做完整性校验）
  │
  ├── 下载方式（按优先级）：
  │     1. 调用 huggingface-cli download（推荐，支持断点续传、并行下载）
  │     2. 调用 modelscope download（ModelScope 来源时）
  │     3. HTTP 直接下载（fallback，逐文件下载）
  │
  ├── 进度采集：
  │     - 解析 CLI 工具的 stdout 进度输出（精确到字节）
  │     - 同时轮询文件系统大小变化（双保险）
  │     - 每 3s 更新一次 DownloadProgress 到 etcd（带 TTL 续约）
  │
  ├── 错误处理：
  │     - 下载失败 → 重试 3 次（指数退避）
  │     - 最终失败 → DownloadProgress.phase=failed，endpoint 标记为 Failed
  │     - 磁盘空间不足 → 提前检查，拒绝下载并报错
  │
  └── 配置：
        - model_source: "huggingface" | "modelscope" | "local"（从 ModelSpec 或 Node 启动参数推断）
        - hf_endpoint: 可选 HuggingFace 镜像地址
        - hf_token: 可选 HuggingFace 认证 token（用于私有模型）
        - download_concurrency: 并行下载文件数（默认 4）
        - download_timeout_secs: 单文件下载超时（默认 3600）
```

### 12.4 下载进度采集

由于 Nebula 接管了下载，进度采集变得简单且精确：

```
下载器在下载过程中：
  ├── 解析 huggingface-cli / modelscope CLI 的 stdout 进度条
  │     输出类似: "Downloading model.safetensors: 45% 7.5G/15G [02:30<03:00, 125MB/s]"
  │     提取: downloaded_bytes, total_bytes, speed, eta
  │
  ├── 同时轮询目标目录文件大小（双保险）
  │     每 3s 统计已下载文件的总大小
  │
  └── 每 3s 更新 DownloadProgress 到 etcd（带 30s TTL 续约）
      下载完成后写入 phase=complete，TTL 到期后自动清除
```

**关键设计**：
- 进度精确到字节级（因为是 Nebula 自己在下载，完全可控）
- 进度数据是临时的（TTL），不污染持久化存储
- 如果模型已在本地缓存，跳过下载，不产生 DownloadProgress 数据，状态直接从 Stopped → Starting
- 下载失败时 DownloadProgress.phase=failed，附带错误信息，便于前端展示

### 12.5 磁盘报警

```
告警规则：
  if node_disk.usage_pct > 85% → 写入 /alerts/{node_id}/disk_warning
  if node_disk.usage_pct > 95% → 写入 /alerts/{node_id}/disk_critical

告警数据结构：
{
  "node_id": "node-1",
  "alert_type": "disk_warning",
  "message": "Node node-1 model directory usage at 87% (500GB/2TB available)",
  "model_dir": "/DATA/Model",
  "usage_pct": 87.0,
  "available_bytes": 500000000000,
  "created_at_ms": 1700000000000
}

消费方：
  - BFF 在 Overview API 中聚合告警信息
  - 前端 Dashboard 显示告警横幅
  - CLI `nebula status` 显示磁盘告警
```

### 12.6 BFF API 扩展

| 方法 | 路径 | 说明 | 角色 |
|------|------|------|------|
| `GET` | `/api/v2/nodes/{node_id}/cache` | 某节点的模型缓存清单 | viewer |
| `GET` | `/api/v2/nodes/{node_id}/disk` | 某节点的磁盘状态 | viewer |
| `GET` | `/api/v2/cache/summary` | 全集群模型缓存汇总 | viewer |
| `GET` | `/api/v2/alerts` | 当前活跃告警（含磁盘告警） | viewer |

**GET /api/v2/models/{model_uid} 响应体扩展**（在模型详情中增加缓存和进度信息）：
```json
{
  "model_uid": "qwen2-5-7b-instruct",
  "state": "downloading",
  "download_progress": {
    "replicas": [
      {
        "replica_id": 0,
        "node_id": "node-1",
        "phase": "downloading",
        "progress_pct": 49.9,
        "speed_bytes_per_sec": 125000000,
        "eta_seconds": 60
      }
    ]
  },
  "cache_status": {
    "cached_on_nodes": ["node-1"],
    "total_size_bytes": 15032000000
  },
  ...
}
```

### 12.7 前端展示

#### Models 页面
- 状态为 Downloading 时，模型卡片显示下载进度条（百分比 + 速度 + 预估剩余时间）
- 如果只有粗粒度进度，显示 "Downloading files (6/12)" 或脉动进度条

#### Model Detail 页面
- "缓存" Tab：显示模型在哪些节点上有缓存、每个缓存的大小和最后访问时间
- "下载进度" 区域：多副本时显示每个 replica 的下载进度

#### Dashboard
- 新增"磁盘状态"区域：每个节点的磁盘使用柱状图
- 告警横幅：磁盘空间不足时顶部显示告警

### 12.8 CLI 扩展

```bash
# 缓存查看（新增）
nebula cache list                              # 全集群模型缓存汇总
nebula cache list --node <node_id>             # 某节点的缓存清单
nebula disk status                             # 各节点磁盘状态

# 模型详情中包含缓存信息
nebula model get <model_uid>                   # 输出中增加 cache_status 和 download_progress
```

## 13. Implementation Tasks


### Wave 1（无依赖）

- [ ] Sync spec to docs

- [ ] Phase 1: Define model management types in nebula-common

### Wave 2（依赖 Wave 1）

- [ ] Phase 2: Scheduler deployment watch

- [ ] Phase 3: Node model_cache_manager module

### Wave 3（依赖 Wave 1 + Wave 2）

- [ ] Phase 4: BFF v2 Model API + Templates + Aggregated State

### Wave 4（依赖 Wave 3）

- [ ] Phase 5: Frontend Models page + Templates + Detail

- [ ] Phase 6: CLI model/template/cache subcommands

### Wave 5（依赖 Wave 2-4）

- [ ] Phase 7: Data migration tool