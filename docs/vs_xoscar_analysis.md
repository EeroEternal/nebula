# Nebula 路径评估：去 xoscar 是否更好 + 优化建议

> 2026-07。对照 powerllm（xoscar actor 运行时，见 powerllm `docs/xoscar_analysis.md`）评估 nebula 的架构路径，并给出优化建议。
> 结论先行：**方向是对的，而且是更好的路径**——用 etcd 声明式状态 + Node 显式进程管理替代 actor 运行时，边界更清晰、状态可外部审计、无小众框架绑架。但当前实现恰好缺的是 xoscar"免费"给的那几块：取消传播、进程树清理、恢复预算、跨节点多 rank。这些不补上，故障场景会重演 powerllm 踩过的坑，只是换了形态。

---

## 1. 路径对比：为什么 nebula 的方向更好

| 维度 | powerllm (xoscar) | nebula | 评价 |
|---|---|---|---|
| 状态权威 | actor 进程内存（supervisor 挂了状态丢） | etcd（/deployments /placements /endpoints，TTL+CAS+watch） | **nebula 优**：可外部审计、天然支持重建，是 K8s 式声明协调 |
| 进程模型 | 模型对象嵌在 actor subpool 里，Python 进程 | Node 显式持有 RunningModel，Docker 容器或本地进程 | **nebula 优**：故障域=容器/进程，`docker restart` 即恢复，不依赖框架恢复语义 |
| RPC | actor ref + 自定义序列化，异常面模糊 | 纯 HTTP/SSE，引擎原生端点 | **nebula 优**：标准协议、可 curl、可观测；powerllm 的 ServerClosed/挂死类问题结构性消失 |
| 引擎耦合 | 模型代码 import 进 worker 进程（thirdparty 87MB） | 引擎是外部进程（vllm serve / sglang），控制面 Rust | **nebula 优**：控制面与推理运行时彻底解耦，升级 vLLM 不动控制面 |
| 语言/生态 | Python + 小众 xoscar | Rust + etcd + axum/reqwest，全主流件 | **nebula 优** |
| 取消传播 | 有（虽有挂死坑，runtime.py:720） | **无**：客户端断连不会中止上游引擎请求 | **powerllm 优** |
| 恢复预算 | `POWERLLM_MODEL_ACTOR_AUTO_RECOVER_LIMIT` | 无上限概念；本地进程 `try_restart` 是空操作 | **powerllm 优** |
| 跨节点 TP/多 rank | vLLM 每 rank 一个 actor（distributed_executor.py） | 仅单节点 `--tensor-parallel-size`，无跨节点 rank 协调 | **powerllm 优** |
| 请求合批 | xoscar extensible/batch（embedding/rerank） | 无，纯代理 | powerllm 有此层，nebula 可下放给引擎 |

判断：**xoscar 解决的"分布式进程管理"难题，nebula 用 etcd + 引擎原生 HTTP + 容器边界重新分解了，分解方式更现代**。powerllm 那些坑（取消挂死、死 subpool 残留、恢复逻辑分裂）在 nebula 结构里不存在——但对应的能力也还没建出来，见 §2。特别注意：nebula 的引擎是"标准推理服务器进程"（vllm serve），而 powerllm 支持任意 Python 模型对象（embedding/rerank/image/audio/thirdparty），**nebula 路径隐含放弃了长尾模型的进程内托管**，这是取舍不是缺陷，但要想清楚长尾模型走什么路（建议：一律要求引擎化/HTTP 化，virtual engine 即此思路）。

## 2. 必须补的坑（对应 xoscar 曾"免费"提供的能力）

### N1. 取消传播（最高优先级）
- 现状：Gateway/Router 都是 `tokio::spawn` 分离任务转发 SSE 字节流，客户端断连只导致 channel 关闭，**上游引擎请求不会被 abort**（gateway/handlers.rs:51-73、router/handlers.rs:284-308）。长输出场景下客户端断开后 GPU 继续白算。
- 做法：channel send 失败（下游断开）时 drop 上游 `reqwest::Response`（drop 即断 TCP，vLLM 会感知 abort）；Responses API 路径同理。不需要 CancelRequest RPC 就能覆盖 90% 场景。
- 验收：curl 中途 Ctrl-C，vLLM 日志出现 aborted request，GPU 利用率回落。

### N2. 进程树清理 + 本地进程重启
- 现状：`Child::kill()` 杀不掉 vLLM 的子进程（EngineCore），docs/testing.md:40-48 已记录孤儿进程占 GPU 显存；`try_restart` 只实现了 Docker 路径，本地进程崩溃后无重启（vllm.rs:400-414）。
- 做法：本地 spawn 时 `setsid` 建进程组，stop 时对进程组发 SIGTERM→超时 SIGKILL；`try_restart` 补本地路径（复用 start 逻辑）。或者干脆**收敛为 Docker-only 生产路径**，本地进程仅限开发，文档写死。
- 验收：kill 引擎主进程后 `nvidia-smi` 无残留；本地模式崩溃能自愈。

### N3. 恢复预算与退避
- 现状：固定 3 次不健康→5 次重启→120s 冷却，无上限（heartbeat.rs:16-22）。坏模型（必崩配置）会无限重启循环。
- 做法：每 model_uid 恢复预算（如 24h 内 N 次）+ 指数退避；超限后置 Failed 状态写回 etcd，等人工/scheduler 干预。这正是 powerllm `AUTO_RECOVER_LIMIT` 的经验。

### N4. 跨节点多 rank（按需，可延后）
- 现状：多 GPU 仅单节点 TP。若目标包含跨节点大模型，需要 placement 支持 rank group（一个 plan 多 node_id）+ rendezvous（vLLM 原生 Ray/multiproc 或 `--distributed-executor-backend`）。
- 建议：**不要自建 rank 协调**（那等于重写 xoscar 最难的部分），直接编排 vLLM 原生多节点方式（Ray cluster 或 torchrun 式启动），nebula 只负责把参数和地址下发。

## 3. 工程收尾类优化

- **N5. 修复 workspace 测试**：`nebula-node/tests/test_virtual.rs` 引用二进制 crate 私有模块导致 `cargo test --workspace` 编译失败。把 nebula-node 拆出 lib target 或删除/重写该测试；CI 应跑全 workspace。
- **N6. 删除双路径**：scheduler 同时留着 `/deployments/` 声明式和 `/model_requests/` 遗留路径（scheduler/main.rs:131-225）；`Scheduler` lib 抽象是空壳（lib.rs:3-13）。收敛到声明式一条路，空壳删掉。
- **N7. 热路径去缓冲**：Gateway/Router 都整体缓冲请求体再转发（router/handlers.rs:100-143）。改为流式转发（header 驱动路由，architecture_analysis.md 已识别）。
- **N8. watch 断线用 revision 续传**：Router 重连后 `watch_prefix(..., None)` 从头 watch（sync.rs:32-39），依赖 list-then-watch 兜底。记录 last revision 续传 + 定期全量 reconcile，符合 architecture.md:87-91 的要求。
- **N9. HA 落地**：ha_roadmap 已列 etcd 三节点、scheduler leader election（可直接用 etcd election API）、Router/Gateway 多副本。powerllm 的 DB lease 选举经验可平移，但 nebula 有 etcd，用原生 lease 更简单。
- **N10. 文档对齐现实**：README 还是 stub 时代示例；`GET /v1/responses` 501、embeddings/rerank 是纯代理这些边界该写清。memory MetaStore 不实现 TTL，测试语义与 etcd 不等价，至少注释标明。

## 4. 优先级建议

1. **N1 取消传播** + **N2 进程树清理**：这是"去 xoscar 后"最先暴露的两个真实故障面，成本低（各一两天）。
2. **N3 恢复预算**、**N5 测试修复**：小改动，立刻做。
3. **N6/N7/N8**：结构收敛，随迭代做。
4. **N9 HA**、**N4 跨节点 TP**:按业务需要排期；N4 坚持"编排 vLLM 原生分布式，不自建 rank 层"。

总评：nebula 的去 xoscar 路径成立且优于在 powerllm 里继续修补 actor 运行时。真正的风险不是架构方向，而是把 xoscar 隐性承担的取消/清理/预算三件事当成"以后再说"——它们是这条路径的完成条件，不是可选项。
