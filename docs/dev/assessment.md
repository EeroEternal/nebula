# 项目范围与设计评估

> 日期：2026-10-19。评估基线：`fa154c0`（自 `b3414f5` / `ea24c3a` 合入 PR `#35`–`#57` 后；含 #56 移除文档中的 HAMi 计划、#57 删除遗留 `admin/` 控制台），workspace v1.10.0。
> 性质：只读架构与代码设计评估，记录基线进展、当前发现与改进建议，不替代正式能力说明或排期。
> 代码引用约定：**只给文件与符号名，不带行号** —— 行号随版本漂移，本仓已多次因锚点失真返工（见 skill `verify-design-doc`）。
> 定位与排期分别以 [`vision.md`](../arch/vision.md)、[`roadmap.md`](../arch/roadmap.md) 为准；流式与缓存现状见 [`streaming-cache.md`](streaming-cache.md)；UI 规范入口为 [`docs/design.md`](../design.md)。

## 总体判断

**架构主轴（Zone C 状态与阶段感知编排、etcd 唯一控制面权威、流式首包承诺）清晰扎实，且近期合入的 PR `#35`–`#55` 实质性收敛了流式可靠性、运维自诊断与 `pd_split` 实测边界。**

下一阶段的重点不再是堆叠新概念，而是修复 Batch 6（KV 缓存治理）接口与引擎实例标识的接线偏差、消除 Gateway 热路径上的逐请求 etcd 读取，并拉平多协议入口与独立/嵌入式路由模式的行为一致性。

---

## 一、`b3414f5..fa154c0` 已收敛的关键问题

对照上一轮评估基线（`b3414f5` / `ea24c3a`），以下问题已在代码中闭环：

1. **流式首包延迟承诺（Pre-first-byte Commitment）与错误帧规范（`#35` / `#40` / `#41` / `#44` / `#45`）**
   - 新增 [`first_byte.rs`](../../crates/nebula-router/src/first_byte.rs)（[await_first_chunk](../../crates/nebula-router/src/first_byte.rs)、[first_chunk_of](../../crates/nebula-router/src/first_byte.rs)），将首包超时 `first_byte_timeout`（60s）及总预算 `first_byte_total_budget`（90s）与流式空闲超时 `stream_idle_timeout`（120s）解耦。
   - 独立 Router（[handlers.rs](../../crates/nebula-router/src/handlers.rs)）与嵌入式 Gateway（[proxy_common.rs](../../crates/nebula-gateway/src/proxy_common.rs)）均在拿到上游首个非空 body chunk 后才向客户端落定 `200 OK`，使首包前卡死可跨副本重试（`first_byte_retry_total`）；首包后中途断开则发送显式 OpenAI SSE `error` 帧且不发 `[DONE]`（`upstream_stream_error_total`）。
2. **嵌入式路由的 KV 事件运行时接线补齐**
   - Gateway 在 `NEBULA_ROUTER_MODE=embedded` 下已调用 [nebula_router::kv_events::start_kv_runtime](../../crates/nebula-gateway/src/main.rs)，消除了此前嵌入式模式下 `prefix_index` 恒为空的差距。
3. **`pd_split` 启动门禁、信号缺失自诊断与实测边界纠偏（`#49`–`#52`）**
   - 针对 LMCache 0.5.5 引擎早于 `lmcache_controller` 启动会导致注册成功但永不上报 KV key 的 sticky 缺陷，在 [wait_for_lmcache_controller](../../crates/nebula-common/src/model_deployment.rs) 中增加了启动前端口就绪门禁，并在 Router 中增加了 [stage_stats_missing_total](../../crates/nebula-router/src/metrics.rs) 计数器。
   - 在满足双前提（controller 先就绪 + `/stats/` 抓取生效）后重测单机 8×RTX 5090（7B / TP=1 / 1:1 池 / 96×1500 tok），诚实记录了 `pd_split` 相比 `replicated` + `prefix_affinity` 吞吐低 17%（10.75 vs 12.89 rps）、TTFT p99 更慢（343 vs 219 ms）的实测结论，将 `pd_split` 适用条件收窄至「取回显著比重算便宜或需拆开扩缩容/故障域」，见 [streaming-cache.md](streaming-cache.md) 与 [benefit-verification.md](benefit-verification.md)。
4. **控制台 Endpoint 状态真实化**
   - [endpoints.tsx](../../frontend/src/components/views/endpoints.tsx) 已改为每 10 秒轮询前缀亲和指标、区分请求失败与未启用，并根据实时 Endpoint 列表计算协议数与网格健康度（[endpoints.tsx](../../frontend/src/components/views/endpoints.tsx)）。

---

## 二、当前基线（`fa154c0`）需要优先处理的问题

### 1. Batch 6.3 `LmcacheController` 与引擎副本的 `instance_id` 不匹配，且 BFF 忽略 `model_uid`（P0 / major）

- **现状：**
  - [nebula-node/src/engine/vllm.rs](../../crates/nebula-node/src/engine/vllm.rs) 与 [nebula-k8s-controller/src/main.rs](../../crates/nebula-k8s-controller/src/main.rs) 在拉起引擎时，向 LMCache 注入的 `LMCACHE_LMCACHE_INSTANCE_ID` 格式均为 `format!("{}-{}", model_uid, replica_id)`（如 `qwen25-7b-0`）。
  - 但 [LmcacheController::from_env](../../crates/nebula-control/src/lmcache.rs) 将 `instance_id` 固定为进程级环境变量 `NEBULA_LMCACHE_INSTANCE_ID`（默认 `"lmcache_default_instance"`）。
  - 同时，BFF 的 [cache_lookup](../../crates/nebula-bff/src/handlers_v2.rs)、[cache_pin](../../crates/nebula-bff/src/handlers_v2.rs)、[cache_clear](../../crates/nebula-bff/src/handlers_v2.rs) 均将路径参数写为 `Path(_model_uid): Path<String>` 并直接忽略。
- **影响：**
  - LMCache `/lookup` 虽会扫描 `layout_info` 中所有实例，但 **`POST /pin` 与 `POST /clear` 依赖 `instance_id` 定位目标 worker**。BFF 传入 `"lmcache_default_instance"` 时无法命中注册为 `{model_uid}-{replica_id}` 的真实引擎副本；多模型共用同一个 `lmcache_controller` 时也缺乏按 `model_uid` 的隔离。
- **建议：**
  - [LmcacheController::pin](../../crates/nebula-control/src/lmcache.rs) 与 [clear](../../crates/nebula-control/src/lmcache.rs) 应支持按目标 `instance_id`（或 `model_uid` 关联的活跃 `{model_uid}-{replica_id}` 列表）下发；`/lookup` 在解析 `layout_info` 时也应按 `{model_uid}-` 前缀过滤。

### 2. `NEBULA_LMCACHE_CONTROLLER_URL` 端口语义冲突，且 `pd_split` 下 `saved_prefill_tokens_total` 恒为 0（P1 / major）

- **现状：**
  - **端口语义冲突：** [vllm.rs](../../crates/nebula-node/src/engine/vllm.rs) 与 [k8s-controller/src/main.rs](../../crates/nebula-k8s-controller/src/main.rs) 将 `NEBULA_LMCACHE_CONTROLLER_URL` 作为 `LMCACHE_CONTROLLER_PULL_URL`（ZMQ PULL 端口，通常为 `8300`）传给引擎；而 [LmcacheController::from_env](../../crates/nebula-control/src/lmcache.rs) 在未设置 `NEBULA_LMCACHE_CONTROLLER_BASE_URL` 时回退读取同一个 `NEBULA_LMCACHE_CONTROLLER_URL` 并拼接 `http://` 作为 HTTP API 地址（通常为 `9000`）。
  - **Token 记账与匹配深度丢失：** [Router::controller_warm](../../crates/nebula-router/src/lib.rs) 调用 `/lookup` 时已通过 [max_matched_tokens(&v)](../../crates/nebula-router/src/lib.rs) 算出命中 token 数 `u64`，但返回类型退化为 `Option<bool>`；而 [route_internal](../../crates/nebula-router/src/lib.rs) 仅从 ZMQ `prefix_index`（`scoped_prefix_matches`）累加 [record_saved_prefill_tokens](../../crates/nebula-router/src/lib.rs)。由于 `LMCacheConnectorV1` 下 `prefix_index` 为空，`pd_split` 模式的 `nebula_router_saved_prefill_tokens_total` 恒为 `0`。此外，`controller_warm` 在 controller 不可达时无熔断冷却，每次长请求都会在热路径等待 200ms 超时。
- **建议：**
  - 拆分 ZMQ 端口与 HTTP API 的环境变量命名（如 `NEBULA_LMCACHE_ZMQ_PULL_URL` vs `NEBULA_LMCACHE_HTTP_URL`）。
  - 将 [controller_warm](../../crates/nebula-router/src/lib.rs) 返回值改为 `Option<u64>`（命中 token 数），既用于在路由到 Decode 副本时计入 `saved_prefill_tokens_total`，也可支持按命中深度比例判定是否直通 Decode；同时为 `/lookup` 增加连续失败后的短期熔断。

### 3. Batch 6.1 单池模式（`replicated`）声明了 `KvSharingSpec` 但未对引擎生效（P1 / major）

- **现状：**
  - [capacity.rs](../../crates/nebula-common/src/capacity.rs) 会对所有携带 `kv_sharing.local_cpu_size_gb` 的部署计算宿主机 DRAM 预留与 `kv_l1_oversubscribed`。
  - 但 [kv_transfer_config](../../crates/nebula-common/src/model_deployment.rs) 与 [kv_sharing_env](../../crates/nebula-common/src/model_deployment.rs) 对 `ReplicaRole::Mixed`（即 `replicated` 模式副本）直接返回 `None` 与空 `Vec`。
- **建议：**
  - 要么为 `ReplicaRole::Mixed` 补齐 `kv_both` 角色的 LMCache 本地 L1/L2 Offload 注入，要么在部署校验层显式拒绝 `replicated + kv_sharing` 组合，避免「容量告警计入了 L1 预算、但引擎实际未启用 L1/L2 卸载」的不一致。

### 4. Gateway 数据面热路径仍存在逐请求读 etcd 与 `Drop` 中 `tokio::spawn`（P1 / major）

- **现状：**
  - [prepare_upstream](../../crates/nebula-gateway/src/proxy_common.rs) 在每次推理请求上顺序执行 [load_tenant](../../crates/nebula-gateway/src/proxy_common.rs)（直读 etcd `/tenants/{id}`）、[check_tooling_gate](../../crates/nebula-gateway/src/proxy_common.rs)（直读 `/deployments/`、`/model_requests/`）与 [check_score_gate](../../crates/nebula-gateway/src/proxy_common.rs)。这违背了 [`AGENTS.md`](../../AGENTS.md) Rule #8（*Gateway/Router hot path reads `/endpoints/` only*）。
  - [ConcurrencyGuard::drop](../../crates/nebula-common/src/admission.rs) 在每个请求结束时通过 `tokio::spawn` 异步获取 `Mutex` 锁以递减并发计数。
  - 多 Gateway 实例间的租户并发/速率计数（[main.rs](../../crates/nebula-gateway/src/main.rs)）与引擎排队器（[main.rs](../../crates/nebula-gateway/src/main.rs)）仍为进程内状态，文档与交付契约需保持「单 Gateway 进程级保护」口径。
- **建议：**
  - 在 Gateway 侧用后台 Watch 循环维护 `/tenants/`、`/deployments/` 等低频元数据的内存快照，使 `prepare_upstream` 热路径彻底零网络 I/O。
  - 将 `TenantAdmission` 的并发计数改为 `DashMap<String, Arc<AtomicU32>>`（或 `RwLock<HashMap<String, Arc<AtomicU32>>>`），在 `ConcurrencyGuard::drop` 中直接 `fetch_sub(1, Ordering::Relaxed)`，消除逐请求 `tokio::spawn`。

### 5. 多协议入口（`/v1/responses`、`/v1/messages`）、独立 Router `Tool-Pause` 与 K8s 控制器的一致性缺口（P2 / moderate）

- **现状：**
  - **首包重试覆盖范围（已核实，原判断有误）：** `/v1/responses` 与 `/v1/messages` 都经 [`post_router_chat`](../../crates/nebula-gateway/src/proxy_common.rs) 在 embedded 模式下转入 [`forward_embedded`](../../crates/nebula-gateway/src/proxy_common.rs)，并重放预取的第一个 chunk（[`proxy_chat_as_responses`](../../crates/nebula-gateway/src/handlers.rs)、[`proxy_chat_as_anthropic`](../../crates/nebula-gateway/src/handlers.rs)），因此**同样受首包前跨副本重试保护**；远端 router 模式则由 router 自身重试。此前记为"未覆盖"并引用 `handlers_responses.rs` / `handlers_anthropic.rs` / `send_to_upstream` 的说法有误，已更正。
  - **独立 Router 模式下 `Tool-Pause` 优先队列失效：** [acquire_queue_permit](../../crates/nebula-gateway/src/proxy_common.rs) 仅通过 `st.router.as_ref()` 检查 `is_tool_continuation_for_tenant`，当 `NEBULA_ROUTER_MODE=remote` 时恒为 `false`。
  - **`nebula-k8s-controller` 绕过 `/placements/` 且存在同步阻塞调用：** [k8s-controller/src/main.rs](../../crates/nebula-k8s-controller/src/main.rs) 直接监听 `/deployments/` 而非消费 Scheduler 产出的 `/placements/`，将 [ep.plan_version](../../crates/nebula-k8s-controller/src/main.rs) 写成了 `dep.version`，并在异步任务中使用同步阻塞的 `std::process::Command::new("kubectl")`（[main.rs](../../crates/nebula-k8s-controller/src/main.rs)）。
- **建议：**
  - 让 `/v1/responses` 与 `/v1/messages` 复用 `forward_embedded` 的首包承诺与跨副本重试逻辑。
  - 在 Gateway 侧根据请求体是否包含 `role="tool"` 消息 + `x-session-id` 判定 Tool-Continuation，或由独立 Router 通过响应头回传 `Tool-Pause` 标记。
  - 将 `nebula-k8s-controller` 演进为消费 `/placements/` 的纯执行器，并将 `kubectl` 调用替换为 `tokio::process::Command` 或 `kube::Api` 异步接口。

---

## 三、建议优先级汇总

| 优先级 | 事项 | 涉及模块 | 预期收益 |
| :--- | :--- | :--- | :--- |
| **P0** | 修复 BFF `/cache/{pin,clear,lookup}` 的 `instance_id` 映射与 `model_uid` 作用域 | [`nebula-control/src/lmcache.rs`](../../crates/nebula-control/src/lmcache.rs)、[`nebula-bff/src/handlers_v2.rs`](../../crates/nebula-bff/src/handlers_v2.rs) | 让 Batch 6.3 的 Pin/Clear API 真正命中注册为 `{model_uid}-{replica_id}` 的引擎副本 |
| **P1** | 拆分 `NEBULA_LMCACHE_CONTROLLER_URL` 的 ZMQ 与 HTTP 环境变量；`controller_warm` 返回 `Option<u64>` 并计入 `saved_prefill_tokens_total` | [`nebula-common`](../../crates/nebula-common/src/model_deployment.rs)、[`nebula-router/src/lib.rs`](../../crates/nebula-router/src/lib.rs) | 消除端口配置歧义，打通 `pd_split` 下的 Batch 6.4 成本归因与命中深度判断 |
| **P1** | 明确 `replicated` 模式下 `KvSharingSpec` 的行为（支持 `Mixed` 卸载或校验拒绝） | [`nebula-common/src/model_deployment.rs`](../../crates/nebula-common/src/model_deployment.rs)、[`capacity.rs`](../../crates/nebula-common/src/capacity.rs) | 消除容量告警与引擎实际启动参数的不一致 |
| **P1** | Gateway 热路径 `/tenants/` 与 Gate 查询改为本地 Watch 缓存；`ConcurrencyGuard` 改用 `AtomicU32` | [`nebula-gateway/src/proxy_common.rs`](../../crates/nebula-gateway/src/proxy_common.rs)、[`admission.rs`](../../crates/nebula-common/src/admission.rs) | 严格对齐 Rule #8，消除高并发下逐请求 etcd RTT 与 `tokio::spawn` 开销 |
| **P2** | 复核 `nebula-k8s-controller` 的状态查询是否阻塞 reconcile、`/endpoints/` 的 `plan_version` 是否与 `/placements/` 同步 | [`nebula-k8s-controller`](../../crates/nebula-k8s-controller/src/main.rs) | 消除控制器运行时阻塞与版本错位（**结论待复核后再定级**） |
