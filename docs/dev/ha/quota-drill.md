# 集群租户配额实测 — issue #68 第一切片

| 字段 | 值 |
|------|-----|
| 日期 | 2026-10-26 |
| 环境 | 单机（Arch / Linux 7.2.5），**真实进程**：1 etcd 3.7.2 + 1 mock 引擎 + 2 nebula-gateway |
| 代码 | `feat(common,gateway): cluster tenant quotas no longer multiply by the Gateway count`（`target/debug` 构建） |
| 复现 | `cargo build -p nebula-gateway && ./scripts/ha_quota_drill.sh` |
| 原始日志 | `logs/ha-quota-drill/`（`/logs/` 已 gitignore） |

## 拓扑

```text
curl ─┬─▶ gateway-A 127.0.0.1:18091 ─┬─▶ mock engine 127.0.0.1:19999
      └─▶ gateway-B 127.0.0.1:18092 ─┘
                     │
                etcd 127.0.0.1:12379  (/tenants/{rps,conc,tiny})
```

两个 Gateway 都是独立进程、各自直连同一 etcd、共享同一份租户配置；`NEBULA_MULTI_TENANT=1` +
`NEBULA_AUTH_TOKENS=rps-token:operator:rps,...` 把 token 绑到租户。`NEBULA_GATEWAY_QUOTA_MEMBERS`
只在两次运行之间变化。

## 结果

| 租户配额 | members | gateway-A 放行/拒绝 | gateway-B 放行/拒绝 | 集群合计 | 判定 |
|---|---|---|---|---|---|
| rps=10/min | **1**（改造前行为） | 10 / 0 | 10 / 0 | **20** | 超卖 2×，复现了缺口 |
| rps=10/min | **2** | 5 / 5 | 5 / 5 | **10** | 硬上限成立，10 个 `429 tenant_rps_exceeded` |
| max_concurrency=2 | 2 | 1 槽 | 1 槽 | 2 | 分片正确 |
| max_concurrency=1 | 2 | 0 | 0 | 0 | `limit < N` 全拒，不超卖 |

并发额度生命周期（`max_concurrency=2`、`members=2` ⇒ 每副本 1 槽）：

| 时刻 | gateway-A | gateway-B | 说明 |
|---|---|---|---|
| 流式请求进行中（10 s SSE） | 第 2 个请求 `429 tenant_concurrency_exceeded` | `200` | 槽位跟着**流**走，不是跟着首字节；另一副本用自己的分片不受影响 |
| 流结束后 | `429` 变 `200` | — | 槽位在流结束时释放 |

修复前同一场景第二个请求会拿到 `200`（guard 在 handler 返回时就释放），这条是本次顺带修的真 bug。

## 这个演练证明了什么 / 没证明什么

**证明了**：多 Gateway 下租户 RPS / 并发配额不再按副本数翻倍；拒绝码与 `x-nebula-deny-code` 稳定；
准入路径无 etcd/PG 调用（进程内纯算术，etcd 只在启动时读 `/tenants/`）；流式请求的并发槽覆盖
整个响应生命周期；`limit < N` 的严格行为符合文档。

**没证明**（未覆盖，仍属 #68 未完成部分）：

- 成员数仍是**部署不变量**：本演练靠改环境变量切换，没有 `/gateways/` lease 注册表，因此
  「扩缩 Gateway 自动重分片」和「成员变化时旧 permit 的过期/释放」都没有验证 —— 相关代码也不存在。
- 单 Gateway 崩溃后的重分片/再分配未验证（当前设计下可用量掉到 `(N-1)/N`，不自动补回）。
- 引擎级队列额度（`NEBULA_GATEWAY_QUEUE_MAX_CONCURRENCY`）仍是 per-Gateway，本演练未涉及。
- 单机多进程不等于多机：未覆盖 xLB、跨机时钟/网络抖动、etcd 抖动下的降级路径。
