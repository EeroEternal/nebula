# 引擎镜像管理系统

引擎镜像管理系统已完成，编译通过，单元测试通过。

## 数据结构 (nebula-common)

- **EngineImage** — 镜像注册表记录，存储在 etcd `/images/{id}`，字段：id, engine_type, image, platforms, version_policy(Pin/Rolling), pre_pull, description, created_at_ms, updated_at_ms
- **NodeImageStatus** — 节点镜像拉取状态，存储在 `/image_status/{node_id}/{image_id}`，字段：node_id, image_id, image, status(Pending/Pulling/Ready/Failed), error, updated_at_ms
- **VersionPolicy** — Pin（本地有则跳过）/ Rolling（每次 re-pull）
- **PlacementAssignment.docker_image** — 新增字段，per-model 镜像覆盖，优先于 Node CLI 默认镜像

## Gateway API (nebula-gateway handlers.rs)

- GET /v1/admin/images — 列出所有注册镜像
- GET /v1/admin/images/:id — 获取单个镜像
- PUT /v1/admin/images/:id — 创建/更新镜像
- DELETE /v1/admin/images/:id — 删除镜像（同时清理 image_status）
- GET /v1/admin/images/status — 列出所有节点的镜像拉取状态

## Node 镜像管理 (nebula-node image_manager.rs)

- **image_manager_loop** — 启动时扫描 `/images/` 全量拉取 + watch 增量拉取
- **pull_if_missing** — Pin 模式检查本地是否存在，Rolling 模式总是 re-pull
- **report_status** — 拉取状态上报到 etcd `/image_status/{node_id}/{image_id}`
- **run_image_gc** — 每小时清理不在注册表且未被容器使用的引擎镜像（仅清理含 vllm/sglang/xllm 的镜像）
- **is_engine_image** — 启发式判断是否为引擎镜像

## docker_image 覆盖机制 (nebula-node)

- `create_engine` 签名新增 `docker_image_override: Option<&str>` 参数
- `reconcile_model` 从 `assignment.docker_image` 传入覆盖值
- scheduler 的 PlacementAssignment 构造处已加 `docker_image: None`
