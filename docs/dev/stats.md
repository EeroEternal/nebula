## Engine Stats Pipeline 是什么

Engine Stats Pipeline 是把推理引擎内部运行指标搬到 Nebula 控制面，让 Router 和 Scheduler 能基于真实负载做决策。

## 当前架构

```text
vLLM / SGLang /metrics
  └── scrape
Node Daemon
  ├── etcd /stats/{model_uid}/{replica_id}   # 最新状态，带 TTL/lease，供实时决策
  ├── Node /metrics                          # 含 scrape 成败结果
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

| 字段 | 单位 / 语义 | 用途 |
|------|-------------|------|
| `pending_requests` | 排队 + 运行中请求数 | least-pending 路由、扩容判断 |
| `kv_cache_usage` | `[0.0, 1.0]` 占用比例；`null` = 未知/不支持 | KV-aware 路由、过载保护、扩缩 |
| `prefix_cache_hit_rate` | `[0.0, 1.0]`；`null` = 未知/不支持 | prefix-cache-aware 路由 |
| `prompt_cache_hit_rate` | 预留；当前多为 `null` | 后续统一 SLI |
| `last_updated_ms` | Node 写入时间 | stale stats 过滤 |

**兼容：** 旧 etcd 值若仍含 `kv_cache_used_bytes` / `kv_cache_free_bytes`（历史 permille 伪字节刻度），反序列化时会换算为 `kv_cache_usage`。新写入不再输出这两个字段。

**禁止：** 用数字 `0` 表示「上游不支持」。缺失一律为 `null` / `None`，UI 显示 `n/a`。

## scrape 健康指标（Node `/metrics`）

| 指标 | 含义 |
|------|------|
| `nebula_node_engine_scrape_result{...,result="success\|unreachable\|timeout\|parse_failed"}` | 最近一次 scrape 结果（gauge=1） |

引擎不可达、超时、解析失败与成功可区分；观测后端故障不影响推理热路径与 etcd 控制面。

## 消费规则

Router：

- watch `/stats/` 并同步到本地 `Router.stats`。
- stats 超过 `NEBULA_ROUTE_STATS_MAX_AGE_MS` 后视为 stale。
- 有 fresh stats 时，应降低或剔除无 stats 的候选。
- 所有候选 `kv_cache_usage` 超过阈值时返回 429。

Scheduler：

- list `/stats/`，用于扩缩容和健康自愈辅助判断。
- 不依赖 xtrace 查询结果作为扩缩容信号。

观测系统：

- xtrace/Prometheus 保存历史指标（Node 仍可 push）。
- 前端面板通过观测后端查询趋势，不直接读取 etcd 作为历史库。
- BFF `/v2/observability/gateway/latency` 的数据源是 Router 的 `nebula_route_*`（响应含 `data_source: "router"`）。
