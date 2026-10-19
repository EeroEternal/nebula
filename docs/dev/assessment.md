# 项目范围与设计评估

> 日期：2026-10-19。评估基线：`c0256b1`（在上一轮基线 `fa154c0` 之上合入 PR `#58`–`#60`；含 #59 修复 LMCache 按副本寻址与 DRAM 告警口径、#60 在 roadmap 记录 K8s 两项已实现部分的缺口），workspace v1.10.0。
> 性质：只读架构与代码设计评估，记录基线进展、当前发现与改进建议，不替代正式能力说明或排期。
> 代码引用约定：**只给文件与符号名，不带行号** —— 行号随版本漂移，本仓已多次因锚点失真返工（见 skill `verify-design-doc`）。
> 定位与排期分别以 [`vision.md`](../arch/vision.md)、[`roadmap.md`](../arch/roadmap.md) 为准；流式与缓存现状见 [`streaming-cache.md`](streaming-cache.md)；UI 规范入口为 [`docs/design.md`](../design.md)。

## 总体判断

**架构主轴（Zone C 状态与阶段感知编排、etcd 唯一控制面权威、流式首包承诺）清晰扎实；本地门禁全绿（`cargo fmt` / `clippy -D warnings` / 267 个 Rust 测试 / 前端 `tsc` + `eslint`）。** 上一轮的 P0（LMCache `instance_id` 寻址）已在 #59 闭环。

剩余问题集中在三处：**Router 侧 `pd_split` 暖信号没有跟上 BFF 的修复**（不按模型过滤、关键环境变量未写进手册且未配置时连告警都不触发）、**Gateway 热路径仍逐请求读 etcd 且租户查询失败时放行**、以及 #59 后残留的**配置提示与代码不一致**。

---

## 一、`fa154c0..c0256b1` 已收敛的问题

1. **LMCache Pin / Clear / Lookup 按副本寻址（上一轮 P0，#59）**
   - [`lmcache_instance_id`](../../crates/nebula-common/src/model_deployment.rs) 成为 `{model_uid}-{replica_id}` 的唯一定义，执行面（[`vllm.rs`](../../crates/nebula-node/src/engine/vllm.rs)、[`nebula-k8s-controller`](../../crates/nebula-k8s-controller/src/main.rs)）注入与控制面寻址共用同一函数。
   - BFF [`lmcache_instances_for`](../../crates/nebula-bff/src/handlers_v2.rs) 从 `/endpoints/{model_uid}/` 取 Ready 副本逐个下发；[`LmcacheController::pin`](../../crates/nebula-control/src/lmcache.rs) / `clear` 返回 `(tokens, failures)`，部分失败对运维可见。
   - [`LmcacheController::lookup`](../../crates/nebula-control/src/lmcache.rs) 经 [`max_matched_tokens_for_model`](../../crates/nebula-control/src/lmcache.rs) 按 `{model_uid}-` 前缀过滤，多模型共用 controller 时不再串号。
2. **控制面不再把 ZMQ 端点当 HTTP 地址（上一轮 P1 前半，#59）**
   - [`LmcacheController::from_env`](../../crates/nebula-control/src/lmcache.rs) 只读 `NEBULA_LMCACHE_CONTROLLER_BASE_URL`，不再回退到执行面使用的 `NEBULA_LMCACHE_CONTROLLER_URL`（ZMQ `LMCACHE_CONTROLLER_PULL_URL`）；[`deployment.md`](../manual/deployment.md) §KV 缓存操作已写明两者区别。
3. **DRAM 水位只计实际生效的 KV 层（上一轮 P1 告警部分，#59）**
   - [`capacity.rs`](../../crates/nebula-common/src/capacity.rs) 仅对 `pd_split` 或存在非 `Mixed` 副本的部署累计 `kv_sharing.local_cpu_size_gb`，`replicated` 部署不再触发虚假的 `kv_l1_oversubscribed`。
4. **K8s 已实现部分的缺口入册（#60）**
   - `nodeName` 硬绑跳过 kube-scheduler、`runtime=process|docker|k8s` 契约字段未落地，已记入 [`roadmap.md`](../arch/roadmap.md) 与 [`k8s.md`](k8s.md)；`controller_warm` 返回命中 token 数需改 `route*` 签名，也已在 roadmap 6.4 说明工作量。

---

## 二、当前基线（`c0256b1`）需要处理的问题

### 1. Router `pd_split` 暖信号：不按模型过滤、配置未入手册、未配置时静默（P1 / major）

- **现状：**
  - **不按模型过滤：** [`Router::controller_warm`](../../crates/nebula-router/src/lib.rs) 用本地的 [`max_matched_tokens`](../../crates/nebula-router/src/lib.rs) 解析 `/lookup`，对 `layout_info` 中**所有**实例取最大值；请求体 `instance_id` 取 `NEBULA_PD_SPLIT_INSTANCE_ID`（默认 `lmcache_default_instance`）。#59 只修了 BFF 侧（`max_matched_tokens_for_model` 在 `nebula-control`，而 `nebula-router` 不依赖 `nebula-control`）。多模型共用一个 controller 时，模型 A 会把模型 B 的缓存当成暖命中，把冷请求送进 Decode 池。
  - **配置未入手册：** 暖信号的开关 `NEBULA_PD_SPLIT_LOOKUP_URL` 与 `NEBULA_PD_SPLIT_INSTANCE_ID` 只出现在 `nebula-router/src/lib.rs`，[`deployment.md`](../manual/deployment.md) §7.6 `pd_split` 部署矩阵、`scripts/`、`deploy/` 均未提及。
  - **未配置时连告警也不触发：** `controller_warm` 在读不到 `NEBULA_PD_SPLIT_LOOKUP_URL` 时直接返回 `false`，早于 `pd_lookup_total` 计数与 50 次全冷告警（`stage_stats_missing_total{decision="warm_signal_cold"}`）。按手册部署的 `pd_split` 会全部走 Prefill 池，且没有任何指标或日志提示。
  - **返回值与熔断：** 返回类型仍是 `bool`，`pd_split` 下 `saved_prefill_tokens_total` 恒为 0（已记入 roadmap 6.4）；controller 不可达时无冷却，每个超过 `NEBULA_PD_BYPASS_TOKENS` 的请求都要等满 `PD_LOOKUP_TIMEOUT_MS`（200ms）。
- **建议：**
  - 把 `/lookup` 响应解析（含 `{model_uid}-` 前缀过滤）下沉到 `nebula-common`，Router 与 `nebula-control` 共用一份实现。
  - 存在 `pd_split` 角色池但未配置 `NEBULA_PD_SPLIT_LOOKUP_URL` 时，启动日志告警并计入 `stage_stats_missing_total`；同时把两个变量补进 §7.6 部署矩阵。
  - 为 `/lookup` 增加连续失败后的短期熔断；返回命中 token 数按 roadmap 6.4 推进。

### 2. Gateway 热路径逐请求读 etcd，且租户查询失败时放行（P1 / major）

- **现状：**
  - [`prepare_upstream`](../../crates/nebula-gateway/src/proxy_common.rs) 在 `NEBULA_MULTI_TENANT=1` 时每个请求调用 [`load_tenant`](../../crates/nebula-gateway/src/proxy_common.rs)（`store.get("/tenants/{id}")`）；[`check_tooling_gate`](../../crates/nebula-gateway/src/interface/gate.rs) 与 [`check_score_gate`](../../crates/nebula-gateway/src/interface/gate.rs) 经 [`resolve_engine_type_for_model`](../../crates/nebula-gateway/src/interface/gate.rs) 逐请求读 etcd。违背 [`AGENTS.md`](../../AGENTS.md) Rule #8（*Gateway/Router hot path reads `/endpoints/` only*）。
  - `load_tenant` 出错时只打 `warn`（`tenant lookup failed; allowing request`）并放行，没有计数器。etcd 抖动期间所有租户配额（RPS、并发、token）同时失效，且事后无法从指标发现。
  - [`ConcurrencyGuard::drop`](../../crates/nebula-common/src/admission.rs) 每个请求结束时 `tokio::spawn` 一个任务去拿 `tokio::sync::Mutex` 递减计数；在 runtime 之外 drop 会 panic。
- **建议：**
  - Gateway 后台 watch `/tenants/`、`/deployments/`、`/model_requests/`，热路径只读内存快照。
  - 租户查询失败的处理（放行或拒绝）做成显式配置，并加失败计数器。
  - 并发计数改为每租户 `Arc<AtomicU64>`，`Drop` 中直接 `fetch_sub`。

### 3. `replicated` + `kv_sharing` 仍被静默接受（P2 / moderate）

- **现状：** #59 修正了告警口径，但 [`validate_pd_split_with_context`](../../crates/nebula-common/src/model_deployment.rs) 对 `Replicated` 只拒绝显式 `role`，不拒绝 `kv_sharing`；而 [`kv_sharing_env`](../../crates/nebula-common/src/model_deployment.rs) 对 `Mixed` 返回空。用户在 `replicated` 部署上声明的 L1/L2 层既不生效也不报错。
- **建议：** 在校验层拒绝（或至少返回警告）`replicated + kv_sharing`，直到 `Mixed` 副本的本地卸载真正接线。

### 4. #59 之后残留的配置提示不一致（P2 / minor，改动小）

- [`lmcache.rs`](../../crates/nebula-control/src/lmcache.rs) 模块注释仍写"或 `http://` + `NEBULA_LMCACHE_CONTROLLER_URL`"，与 `from_env` 现行为不符。
- BFF [`cache_controller`](../../crates/nebula-bff/src/handlers_v2.rs) 的报错文案仍提示 "set `NEBULA_LMCACHE_CONTROLLER_URL` (host:port) or …"。运维按提示设置后功能依然关闭。
- 同处函数注释写 "Disabled (503)"，实际返回 `ServiceError::BadRequest`（400，与手册一致）。

### 5. 执行面与多模式一致性缺口（P2 / moderate）

- **`nebula-k8s-controller`：** [`main.rs`](../../crates/nebula-k8s-controller/src/main.rs) 直接 `list_prefix("/deployments/")` 而非消费 Scheduler 产出的 `/placements/`，写回 `/endpoints/` 时 `plan_version: dep.version`；6 处 `kubectl` 调用使用同步的 `std::process::Command`：5 处直接位于 async 函数中，另 1 处在同步的 `create_engine_pod` 里、由 async 的 `ensure_replica_running` 调用，都会阻塞 tokio 工作线程。整个控制器是一个 1222 行的文件，只有 2 个测试。
- **独立 Router 模式下 `Tool-Pause` 优先级失效：** [`acquire_queue_permit`](../../crates/nebula-gateway/src/handlers.rs) 只通过 `st.router.as_ref()` 调 `is_tool_continuation_for_tenant`，`NEBULA_ROUTER_MODE=remote` 时恒为 `false`。（上一版把该函数记在 `proxy_common.rs`，实际在 `handlers.rs`。）
- **建议：** `kubectl` 改 `tokio::process::Command`（或 `kube::Api`）并按 reconcile / kubectl 封装 / endpoint 写回拆模块；`plan_version` 与 `/placements/` 对齐；remote 模式的 Tool-Pause 要么由 Router 经响应头回传，要么在手册中写明不支持。

### 6. 文档行号锚点已漂移（P3 / minor）

- `docs/` 中仍有 30 处 `*.rs#L<n>` 锚点（`kv-governance.md` 12、`heterogeneous-compute.md` 11、`direction.md` 3、`roadmap.md` 3、`landscape.md` 1）。其中 6 处 `controller_warm` 指向 `lib.rs#L707`，该函数现已不在此行。按本文约定改为"文件 + 符号名"。

---

## 三、工程与仓库卫生

| 项 | 现状 | 建议 |
| :--- | :--- | :--- |
| 测试分布 | `nebula-common` 113、`nebula-router` 78 个测试；`nebula-cli`、`nebula-observe` 为 0，`nebula-k8s-controller` 2、`nebula-meta` 3、`nebula-control` 9 | 优先给 k8s-controller 与 meta（election / lease / CAS）补测试 |
| 前端测试 | `frontend/src` 下没有任何 `*.test.*` / `*.spec.*`，门禁只有 `tsc` + `eslint` + `build` | 引入 vitest，先覆盖 `lib/` 的 API 契约与 `i18n.tsx`（1708 行）中英文键齐全 |
| 大文件 | `nebula-router/src/lib.rs` 3226 行、`nebula-bff/src/service.rs` 2089 行、`nebula-router/src/kv_events.rs` 1634 行 | Router 按选路策略 / `pd_split` 暖信号 / 指标拆模块，便于单测 |
| 游离脚本 | `scripts/debug_router.rs`、`scripts/test_ug.rs` 不属于任何 crate，CI 不编译，也无引用 | 删除，或改为 `examples/` / 测试 |
| `.gitignore` | 沿用 xinference 的 Python / Django 规则，并保留已删除 `admin/` 的条目；`build/`、`static/`、`lib64/`、`instance/` 等宽泛规则可能误伤新目录 | 重写为只覆盖本项目（Rust / Node / 本地日志与 env） |

---

## 四、建议优先级汇总

| 优先级 | 事项 | 涉及模块 | 预期收益 |
| :--- | :--- | :--- | :--- |
| **P1** | Router 暖信号按模型过滤（共享解析下沉 `nebula-common`）；未配置 `NEBULA_PD_SPLIT_LOOKUP_URL` 时告警并补手册；`/lookup` 熔断 | [`nebula-router/src/lib.rs`](../../crates/nebula-router/src/lib.rs)、[`nebula-control/src/lmcache.rs`](../../crates/nebula-control/src/lmcache.rs)、[`deployment.md`](../manual/deployment.md) | 多模型下冷暖判定正确；`pd_split` 不再静默退化 |
| **P1** | Gateway 租户 / Gate 查询改 watch 快照；租户查询失败策略显式化并计数；`ConcurrencyGuard` 改原子计数 | [`proxy_common.rs`](../../crates/nebula-gateway/src/proxy_common.rs)、[`interface/gate.rs`](../../crates/nebula-gateway/src/interface/gate.rs)、[`admission.rs`](../../crates/nebula-common/src/admission.rs) | 对齐 Rule #8；etcd 抖动不再让配额整体失效 |
| **P2** | 校验层拒绝 `replicated + kv_sharing` | [`model_deployment.rs`](../../crates/nebula-common/src/model_deployment.rs) | 声明即生效，避免静默忽略 |
| **P2** | 修正 `lmcache.rs` 模块注释与 BFF `cache_controller` 报错文案 / 注释 | [`lmcache.rs`](../../crates/nebula-control/src/lmcache.rs)、[`handlers_v2.rs`](../../crates/nebula-bff/src/handlers_v2.rs) | 运维按提示配置即可生效 |
| **P2** | k8s-controller 异步化 `kubectl`、对齐 `/placements/` 与 `plan_version`；remote 模式 Tool-Pause | [`nebula-k8s-controller`](../../crates/nebula-k8s-controller/src/main.rs)、[`handlers.rs`](../../crates/nebula-gateway/src/handlers.rs) | 控制器不阻塞 runtime；两种 Router 模式行为一致 |
| **P3** | 清理文档 `#L` 锚点、游离 `.rs` 脚本、`.gitignore`；补前端与 k8s-controller 测试 | `docs/dev/`、`scripts/`、`frontend/` | 降低文档失真与维护成本 |
