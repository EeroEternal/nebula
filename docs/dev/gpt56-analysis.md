






















# 结论

架构方向合理：etcd 声明式状态、Rust 控制面、外部引擎进程、HTTP Passthrough 都没有明显方向性问题。文档列出的生命周期优化基本准确，但结合当前代码，存在几个比文档更紧急的正确性问题。

## P0：优先修复

1. **同一模型无法在同一节点运行多个副本**

Scheduler 会为一个节点生成多个 assignment，但 Node 使用 `HashMap<String, RunningModel>`，以 `model_uid` 为唯一键，并且只取当前节点的第一个 assignment。因此同一模型在单节点配置多个副本时，实际只会启动一个：

- Scheduler 支持生成多个副本：`@/Users/xinference/github/nebula/crates/nebula-scheduler/src/planner.rs:300-343`
- Node 只执行第一个匹配 assignment：`@/Users/xinference/github/nebula/crates/nebula-node/src/reconcile.rs:191-205`
- 运行状态只能保存一个模型实例：`@/Users/xinference/github/nebula/crates/nebula-node/src/reconcile.rs:333-347`

建议把 Node 内部状态统一改为 `(model_uid, replica_id)` 作为 key，reconcile 以 assignment 集合为单位执行差量，而不是每个模型只处理一个 assignment。

2. **自动缩容目前实际上不会执行**

代码能算出 `desired_replicas < current_replicas`，也会增加缩容 metric，但 `need_update` 只处理副本不足，没有处理副本过多；同时也没有截断 assignment：

`@/Users/xinference/github/nebula/crates/nebula-scheduler/src/reconcile.rs:174-189`

`@/Users/xinference/github/nebula/crates/nebula-scheduler/src/reconcile.rs:247-299`

建议显式计算：

- `healthy > desired`：选择待移除副本并缩减 plan。
- `healthy < desired`：增加副本。
- `healthy == desired`：不更新。

副本选择还应优先移除负载低、无 session affinity、节点资源紧张的实例。

3. **Drain 可能永久停留，无法真正停止引擎**

第一次执行 [drain_then_stop()](cci:1://file:///Users/xinference/github/nebula/crates/nebula-node/src/reconcile.rs:121:0-162:1) 只标记 `Draining` 后直接返回，后续必须再次调用才能检查 pending 和超时：

`@/Users/xinference/github/nebula/crates/nebula-node/src/reconcile.rs:122-161`

但 Node 目前只依赖 placement watch 触发 reconcile，没有文档所说的 Node periodic full reconcile：

`@/Users/xinference/github/nebula/crates/nebula-node/src/main.rs:142-201`

如果 placement 删除后没有新事件，引擎可能一直处于 Draining。建议增加固定周期的 Node reconcile，或者为每个 drain 启动一个可取消的定时任务。

4. **恢复机制没有预算，且本地进程无法自愈**

健康检查达到阈值后会无限循环重启，没有 24h 次数预算、指数退避或 Failed 状态：

`@/Users/xinference/github/nebula/crates/nebula-node/src/heartbeat.rs:254-279`

同时 [try_restart()](cci:1://file:///Users/xinference/github/nebula/crates/nebula-node/src/engine/vllm.rs:399:4-407:5) 默认是空操作，本地进程实际上不会重启；本地停止也只杀直接子进程，无法清理 vLLM/SGLang 进程树：

`@/Users/xinference/github/nebula/crates/nebula-node/src/engine/vllm.rs:356-369`

`@/Users/xinference/github/nebula/crates/nebula-node/src/engine/sglang.rs:324-343`

建议优先选一个明确策略：

- 生产环境强制 Docker-only；或者
- 本地启动时建立独立进程组，停止时 SIGTERM→等待→SIGKILL 整个进程组，并由 Node 完整重建 `EngineHandle`。

## P1：控制面一致性

5. **Router 的 list→watch 存在丢事件窗口**

Router 先 [list_prefix()](cci:1://file:///Users/xinference/github/nebula/crates/nebula-meta/src/etcd.rs:68:4-81:5)，然后从当前 revision 开始 watch，两个操作之间的修改可能永久丢失：

`@/Users/xinference/github/nebula/crates/nebula-router/src/sync.rs:10-58`

Node 已经记录 revision 并续传，方向更正确：

`@/Users/xinference/github/nebula/crates/nebula-node/src/main.rs:107-155`

Router 应使用快照 revision 启动 watch，并处理 etcd compact 错误；重新连接后执行全量校正。`endpoints` 和 `placements` 两条同步链路都要修改。

6. **多模型路由没有严格执行 `plan_version`**

Router 只维护一个全局 `AtomicU64`，并且只为启动参数指定的“主模型”设置版本；其他模型走不带版本过滤的 [route()](cci:1://file:///Users/xinference/github/nebula/crates/nebula-router/src/lib.rs:478:4-484:5)：

`@/Users/xinference/github/nebula/crates/nebula-router/src/sync.rs:61-117`

`@/Users/xinference/github/nebula/crates/nebula-router/src/handlers.rs:150-177`

这与文档“Router 只使用最新 plan_version endpoint”的约束不一致。应改为 `model_uid -> plan_version` 映射，并对所有模型统一过滤。

7. **Placement version 不是严格单调版本**

文档要求 `version: u64` 单调递增，但新 plan 直接使用 [now_ms()](cci:1://file:///Users/xinference/github/nebula/crates/nebula-router/src/lib.rs:21:0-26:1)：

`@/Users/xinference/github/nebula/crates/nebula-scheduler/src/planner.rs:336-343`

同一毫秒内更新或系统时间回拨，都可能产生重复或倒退。建议在 CAS 更新时使用 `old_plan.version + 1`，时间戳单独存为 `updated_at_ms`。

8. **声明式和遗留路径仍然同时运行**

Scheduler 同时 watch `/model_requests/` 和 `/deployments/`：

`@/Users/xinference/github/nebula/crates/nebula-scheduler/src/main.rs:143-169`

周期 reconcile 也同时读取两套状态并进行 fallback：

`@/Users/xinference/github/nebula/crates/nebula-scheduler/src/reconcile.rs:125-172`

这会增加状态冲突、测试矩阵和运维理解成本。建议尽快让 API/BFF 把旧请求转换成 `ModelDeployment`，Scheduler 内部只认 deployment。

## P2：性能与工程质量

9. **Gateway 和 Router 都会完整缓冲请求体**

Gateway 完整读取一次：

`@/Users/xinference/github/nebula/crates/nebula-gateway/src/handlers.rs:472-505`

Router 再读取、解析、重写一次：

`@/Users/xinference/github/nebula/crates/nebula-router/src/handlers.rs:100-143`

高并发、大 embedding 输入时会造成两层内存放大。由于 Router 需要读取 `model` 并重写 body，不能简单改成无脑流式代理。更合理的方案是 Gateway 解析模型后写入可信内部 header，Router 根据 header 路由并流式转发原始 body；模型名转换尽量在 Gateway 完成。

10. **Node 在持有全局锁时执行网络 I/O**

健康检查和指标抓取期间一直持有整个 `running` 锁：

`@/Users/xinference/github/nebula/crates/nebula-node/src/heartbeat.rs:145-191`

placement reconcile 同样在持锁状态下可能执行模型下载、引擎启动和 readiness 等待：

`@/Users/xinference/github/nebula/crates/nebula-node/src/main.rs:161-173`

一个模型冷启动可能阻塞该节点所有模型的 reconcile。完成多副本改造时，建议采用每 replica 状态机或细粒度锁，锁内只做状态快照和提交，网络及进程操作放在锁外。

11. **etcd TTL 每次心跳都会创建新 lease**

每次带 TTL 的 [put()](cci:1://file:///Users/xinference/github/nebula/crates/nebula-meta/src/etcd.rs:35:4-48:5) 都调用 `lease_grant()`：

`@/Users/xinference/github/nebula/crates/nebula-meta/src/etcd.rs:34-48`

Node 又会周期刷新节点和所有 endpoint，造成 lease 持续创建。建议节点启动时创建 lease，节点状态及 endpoint 复用同一个 lease，并通过 keepalive 维持；节点退出或失联后由 lease 一次性清理。

## 文档需要更新的部分

文档中的部分“未来能力”已经落地：

- Scheduler election、leader gate、fencing epoch 已实现：`@/Users/xinference/github/nebula/crates/nebula-scheduler/src/main.rs:29-97`
- Router/Gateway SSE 断连检测和 abort metric 已实现：`@/Users/xinference/github/nebula/crates/nebula-router/src/handlers.rs:284-330`
- Drain 已部分实现，但缺少持续推进机制：`@/Users/xinference/github/nebula/crates/nebula-node/src/reconcile.rs:122-161`

因此原计划中 C1 不应再标为纯目标态；A1 的重点应从“实现取消”调整为“补真实引擎契约测试”；C2 应标为“部分完成”。

# 推荐实施顺序

第一批先处理 **Node 多副本状态模型、缩容无效、Drain 永久停留**，这三个属于功能正确性问题。第二批处理 **恢复预算、进程树清理、Router revision/plan_version**。第三批再做 **双路径删除、请求体流式化、etcd lease 复用和锁粒度优化**。

**审查完成：架构方向不需要调整，但当前最优先的问题应从文档中的生命周期增强，提升为多副本、缩容和 Drain 三个正确性修复。**
