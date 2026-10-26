# 自动成员发现 / 重分片 / 队列分片 / 真实引擎 / LB 摘流 — Railway 多机演练（第二轮）

| 字段 | 值 |
|------|-----|
| 日期 | 2026-10-26（第二轮，接 [`railway-drill.md`](railway-drill.md)） |
| 环境 | Railway Sandboxes（Pro），**3 台独立 VM**（8 vCPU / 7 GB / Ubuntu glibc 2.43），`--private-network` + 各自公开端口 |
| 被测 | `/gateways/` lease 注册表 + 自动重分片、未使用分片再分配、引擎队列成员分片、真实引擎（Ollama + TinyLlama CPU）、入口 LB 主动摘流 |
| 二进制 | 本机 `cargo build --release -p nebula-gateway` 上传（非沙箱编译） |
| 复现 | 单机等价断言：`scripts/ha_quota_drill.sh`（T1–T9，真 etcd + 2 个真实 Gateway 进程） |
| 清理 | 3 个 sandbox 已 destroy、environment `ha-drill2` 已删除 |

## 1. 拓扑

```text
client ──HTTPS──▶ Railway edge ──▶ sandbox C: nginx :8080 (upstream A,B；max_fails=1)
                                      │  proxy_next_upstream error timeout 5xx
                                      ├──▶ sandbox A: gateway-A :18091  (etcd n1)
                                      └──▶ sandbox B: gateway-B :18092  (etcd n2)
                                                    │
                                      sandbox C 另有 etcd n3 + Ollama :11434 (TinyLlama, CPU)

三节点 etcd（跨机 raft）＋ 每个 Gateway 自带 `/gateways/{instance_id}` lease（TTL 8s / refresh 1s）
```

## 2. 结果

| 场景 | 期望 | 实测 |
|---|---|---|
| 自动发现（t-live rps=10，不设 `NEBULA_GATEWAY_QUOTA_MEMBERS`） | 两副本各 5 | 经 LB 连发 12 次：**10 放行**（5+5）、2 × `429 tenant_rps_exceeded` |
| 副本离开 → 分片回收（kill gateway-A） | B 看到 members=1，独占 10 | B 的 `nebula_gateway_quota_members` 变 1；连发 10 次 **10 放行**；nginx 访问日志 **0 个 5xx**（客户端无感） |
| 副本回归 → 重新分片（重启 gateway-A） | 回到 5+5 | B 的 members 回 2；LB 连发 10 次 → **9 放行 / 1 拒**（见 §4 已知边界：LB 分发不均） |
| 引擎队列额度按成员数分片（`QUEUE_MAX_CONCURRENCY=8`, 2 成员） | 每副本 4 | 直连 A 的 6 个并发流：**4 × 200 + 2 × `429 queue_full`**，`nebula_gateway_queue_denied_total{reason="queue_full"} 2` |
| 真实引擎流量（Ollama + TinyLlama，CPU，无 GPU） | 真实 token/usage | 非流式：真实补全 + 真实 usage（`total_tokens` 61–68）；流式：SSE 真实分片，客户端 TTFT 1.0 s、`x-nebula-upstream-ttft-ms: 177`、`x-nebula-queue-wait-ms: 0` |
| 真实引擎 + 集群配额（t-real rps=4，2 成员） | 4 放行 | 6 次真实请求：**4 × 200 + 2 × 429** |
| 真实引擎 + 副本故障 | LB 摘流，流量继续 | kill gateway-A 后经 LB 的 2 个真实 SSE 请求均 200（各 14 帧） |
| lease 丢失自愈（`etcdctl del /gateways/gw-a`） | 重新注册而不是永久 fail-closed | 1 s 内日志 `gateway registration was missing ...; re-registered`，members 回 2，请求正常 |

## 3. 本轮改了什么（代码）

1. **`/gateways/` lease 注册表**（`crates/nebula-gateway/src/quota_members.rs`）：每副本一个 lease 绑定 key（TTL + keepalive），后台 `list_prefix` 刷新 → `nebula_common::quota_shard::MemberView` 快照（带观测时间戳，过期即拒绝回答）。启动是「先注册、等一个刷新周期、再开始按分片限流」，避免自己按旧成员数放行。
2. **按 rank 的余数分配**（`shard_quota_ranked`）：`limit % n` 由排位靠前的成员多拿 1，Σ 分片**恰好等于**限额（不再浪费 `n * floor(limit/n)` 的余数）。
3. **未使用分片再分配**：成员离开 → 存活成员的分母变小 → 分片自然扩大；优雅退出会立即注销 key（不等 TTL）。
4. **降级显式化**：视图不可信时 `fail_closed`（默认，`503 tenant_quota_unavailable`）或 `fail_open`（按全局限额服务并计数）；指标 `nebula_gateway_quota_members` / `quota_view_fresh` / `quota_degraded_total`。
5. **引擎队列成员分片**：`EngineQueues::set_members(n)`，`EngineQueue::cluster_cap()` 在准入与交接两个位置生效 —— 队列容量随成员数变化自动跟随，不需要重启。
6. **注册丢失自愈**：刷新时发现 `/gateways/` 里没有自己，就用同一 lease 重新注册（否则一次 TTL 抖动会让副本永久 fail-closed）。
7. 关闭信号：SIGTERM 先注销再退出（否则装了信号处理器后进程不再退出 —— 本轮演练前就是这样）。

## 4. 已知边界（未在本次解决）

- **LB 分发不均会浪费分片**：T-LB3 的 9/10 就是 nginx 把 6 个请求给了同一副本、另一副本只用 4 个。确定分片下容量不可转移，这是设计取舍（不做跨副本令牌借用），要在容量规划里留余量。
- **`members` 视图仍是「最终一致 + 有界窗口」**：注册→其它副本看到之间有一个刷新周期（本轮 1 s）；新副本在同一周期内不会放行，所以不会超卖，但会有一次短暂的能力空档。
- **全体 etcd 失联时**：逐请求的 `/tenants/{id}` 读取会先失败 → 走上一轮的 fail-open（按全局限额、计入 `tenant_lookup_failed_total`），此时 registry 视图的 fail-closed 根本轮不到。真正的修法是 HA-P1 的租户快照（一个快照同时承载配额与成员，只做一次降级决策），仍未实现。
- 真实引擎是 **Ollama + TinyLlama（CPU，无 GPU）**：验证的是 Nebula 侧的准入/转发/SSE 语义，不代表 vLLM GPU serving 的 TTFT 或 PD 拓扑。
- 入口 LB 用 **nginx（`max_fails`/`proxy_next_upstream`）** 代替 xLB：xLB 不在本仓、也不在本仓门禁内，本次只验证「Nebula 的 `/healthz` + 错误语义能被主动摘流的 LB 正确利用」这一半。
