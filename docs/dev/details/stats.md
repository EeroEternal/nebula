## Engine Stats Pipeline 是什么

Engine Stats Pipeline 是把推理引擎内部运行指标搬到 Nebula 控制面，让 Router 和 Scheduler 能基于真实负载做决策。

## 当前代码状态

当前 Node 已经会在 heartbeat loop 中对运行中的 engine 做健康检查和 stats scrape，并将关键指标推送到 xtrace：

- `pending_requests`
- `kv_cache_usage`
- `prefix_cache_hit_rate`
- GPU memory、temperature、utilization

Router 侧也已经具备基于 stats 的路由能力，包括 `LeastKvCache`、`PrefixCacheAware`、stale stats 过滤和 overloading admission control。

但当前还没有完全收敛到“Node 写 etcd `/stats/`，Router/Scheduler watch `/stats/`”这条控制面热路径。也就是说，观测链路已经部分可用，控制面实时决策链路仍需定稿。

## 推荐目标架构

控制面实时状态和历史观测数据应分开：

```text
vLLM /metrics
  └── scrape
Node Daemon
  ├── etcd /stats/{model_uid}/{replica_id}   # 最新状态，带 TTL/lease，供实时决策
  └── xtrace / Prometheus                    # 历史观测、面板、告警

Router / Scheduler
  └── watch/list /stats/                     # 路由、过载保护、扩缩容
```

## `/stats/` 数据契约

`/stats/{model_uid}/{replica_id}` 存储 `EndpointStats`，只保存最新状态，必须带 TTL 或 lease，避免失效副本留下脏数据。

关键字段：

| 字段 | 来源 | 用途 |
|------|------|------|
| `pending_requests` | engine metrics 或本地代理计数 | least-pending 路由、扩容判断 |
| `kv_cache_used_bytes` | engine metrics | KV-aware 路由、过载保护 |
| `kv_cache_free_bytes` | engine metrics | 计算 KV 使用率 |
| `prefix_cache_hit_rate` | engine metrics | prefix-cache-aware 路由 |
| `last_updated_ms` | Node 写入时间 | stale stats 过滤 |

## 消费规则

Router：

- watch `/stats/` 并同步到本地 `Router.stats`。
- stats 超过 `NEBULA_ROUTE_STATS_MAX_AGE_MS` 后视为 stale。
- 有 fresh stats 时，应降低或剔除无 stats 的候选。
- 所有候选 KV 使用率超过阈值时返回 429。

Scheduler：

- list/watch `/stats/`，用于扩缩容和健康自愈辅助判断。
- 不应依赖 xtrace 查询结果作为唯一扩缩容信号。

观测系统：

- xtrace/Prometheus 保存历史指标。
- 前端面板通过观测后端查询趋势，不直接读取 etcd 作为历史库。

## 下一步

1. Node 在 scrape stats 后写入 etcd `/stats/{model_uid}/{replica_id}`。
2. Router 增加 `/stats/` watch loop，优先使用 etcd 最新 stats。
3. Scheduler reconcile 读取 `/stats/`，xtrace 只作为历史观测与辅助查询。
4. 更新测试，覆盖 stale stats、TTL 过期、stats 缺失降级和全过载 429。
