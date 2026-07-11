## Engine Stats Pipeline 是什么

Engine Stats Pipeline 是把推理引擎内部运行指标搬到 Nebula 控制面，让 Router 和 Scheduler 能基于真实负载做决策。

## 当前架构（P0-2 已落地）

```text
vLLM /metrics
  └── scrape
Node Daemon
  ├── etcd /stats/{model_uid}/{replica_id}   # 最新状态，带 TTL/lease，供实时决策
  └── xtrace / Prometheus                    # 历史观测、面板、告警

Router
  └── watch /stats/                          # 路由、过载保护

Scheduler
  └── list /stats/                           # 扩缩容、缩容选副本

Node Drain
  └── get /stats/...                         # pending=0 或超时后再停引擎
```

Node 在 heartbeat 健康检查后 scrape，并 `put_with_lease` 写入 `/stats/`（与 status/endpoints 共用 lease）。Router 用与 endpoints 相同的 list→watch 模式同步本地 `Router.stats`。Scheduler reconcile 从 etcd list，不再查询 xtrace 做扩缩容。

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

- list `/stats/`，用于扩缩容和健康自愈辅助判断。
- 不依赖 xtrace 查询结果作为扩缩容信号。

观测系统：

- xtrace/Prometheus 保存历史指标（Node 仍可 push）。
- 前端面板通过观测后端查询趋势，不直接读取 etcd 作为历史库。
