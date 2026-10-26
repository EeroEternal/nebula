# 多机集群配额与故障演练 — Railway Sandboxes

| 字段 | 值 |
|------|-----|
| 日期 | 2026-10-26 |
| 环境 | Railway Sandboxes（Pro 计划，`us-west2`），**3 台独立 VM**：8 vCPU / 7 GB / Ubuntu glibc 2.43，私有网络互通 |
| 被测对象 | 本文提交的 `NEBULA_GATEWAY_QUOTA_MEMBERS` 分片 + 并发 guard 生命周期修复 |
| 二进制 | 本机 `cargo build --release -p nebula-gateway`（glibc 2.38 符号）上传，非沙箱内编译 |
| 复现 | 单机版等价断言见 `scripts/ha_quota_drill.sh`；多机步骤见文末「复现」 |
| 清理 | 3 个 sandbox 已 `destroy`，演练用 environment `ha-drill` 已删除（无残留计费） |

## 1. 拓扑

```text
client (laptop)
   │ HTTPS
   ├─▶ Railway edge ─▶ sandbox A (10.250.23.61)  gateway-A :18091 ┐
   └─▶ Railway edge ─▶ sandbox B (10.178.81.179) gateway-B :18092 ┤
                                                                  ├─▶ sandbox C: mock engine :19999
   etcd 3 节点（互为主备，AA/AAAA 私有域名 + 2380 跨机 raft）:     │
     sandbox A: n1   sandbox B: n2   sandbox C: n3  ◀──────────────┘
```

- 两台 Gateway 各自直连同一 3 节点 etcd（`ETCD_ENDPOINT` 三段逗号分隔），共享同一份 `/tenants/`。
- 客户端只能通过 Railway 边缘的公开 HTTPS 域名进入（每次请求多约 0.8 s 边缘代理开销，纯功能验证可接受）。

## 2. 结果

| 场景 | 配置 | 结果 | 判定 |
|---|---|---|---|
| T1 控制组：`members=1` | 租户 rps=10/min | A 放行 10 / B 放行 10，**合计 20** | 复现 N× 超卖 |
| T2 分片：`members=2` | 同上 | **5 + 5 = 10**，其余 10 × `429 tenant_rps_exceeded` | 集群硬上限成立 |
| T3 并发额度生命周期 | `max_concurrency=2`（每副本 1 槽） | 10 s 流式请求进行中：A 第二个请求 `429 tenant_concurrency_exceeded`；B `200`；流结束后 A `200` | 槽位跟着**流**走，跨机不串味 |
| T4 `limit < members` | `max_concurrency=1`, members=2 | A、B 均 `429` | 严格优先，不超卖 |
| T5 etcd 单节点故障 | kill n1（A 上的 etcd） | n2 在 term 3 当选，2/3 保持 quorum；两台 Gateway 继续 `200`；n1 用 `--initial-cluster-state existing` 归队后 3/3 健康（index 一致） | 控制面 HA 成立，数据面不中断 |
| T5b 无 quorum | 再 kill n3（仅剩 n2，1/3） | 请求仍 `200`（**存量行为的 fail-open**），但 `nebula_gateway_tenant_lookup_failed_total` +1/次并打 WARN | 降级可见，但不该静默 |
| T6 Gateway 故障 | kill gateway-A | A 域名 `502`，B 继续 `200`；A 重启后 `200`；此后租户 rps=6 仍是 **3 + 3 = 6** | 副本故障不影响对端，配额不因重启漂移 |

### 2.1 演练发现并修掉的真缺陷：热路径租户读取 fail-open

T5 首次执行时 `fail` 租户（rps=4, members=2）在 B 上放行了 **3** 次而不是 2 次。日志给出根因：

```text
WARN nebula_gateway::proxy_common: tenant lookup failed; allowing request
     error=grpc request error: status: Unavailable, message: "tcp connect error" tenant_id=fail
```

`ETCD_ENDPOINT` 里有一个死节点时，`etcd-client` 的负载均衡会把**逐请求**的 `/tenants/{id}` 读取打到死节点，读失败后 Gateway 直接**放行且不做任何计数**——即配额可被绕过，且静默。这违反 [`../dev/etcd.md`](../../dev/etcd.md) / [`../../arch/ha.md`](../../arch/ha.md) §4「热路径读取失败不能静默放行」。

修复（本次提交，最小切面）：读失败重试一次（死节点在第二次通常已被跳过）；仍失败则放行但**计数 + WARN**：

```text
nebula_gateway_tenant_lookup_failed_total   # 0 = 正常；非 0 = 正在无配额服务
```

修复后同场景复测：`fail` 租户在 `members=2` + 死 etcd 节点下**恰好 2 次**放行，`tenant_lookup_failed_total 0`；无 quorum 场景下计数按请求递增（2 次请求 → 2），WARN 明确写 "serving without quota (counted)"。

**这不是最终方案**：逐请求 etcd 读本身就是 HA-P1 要替换的东西（Gateway 后台租户快照）。修复的价值是把「静默放行」变成「显式、可告警的降级」。

## 3. 没有证明的

- **`members` 仍是部署不变量**：本演练靠改环境变量在两次运行间切换；`/gateways/` lease 注册表与自动重分片不存在，因此「扩缩 Gateway 自动生效」「成员变化时旧 permit 过期/释放」都未实现（#68 保持 open）。
- 单 Gateway 故障后未使用分片的**动态再分配**未做：T6 里 A 挂掉期间可用量掉到 `1/2`，靠重启补回。
- 引擎级队列额度（`NEBULA_GATEWAY_QUEUE_MAX_CONCURRENCY`）仍 per-Gateway，本次未涉及。
- 没有真实引擎/真实模型流量；mock 引擎只验证准入与转发语义（引擎行为不是本次目标）。
- 未覆盖 xLB/外部 LB 自己的健康检查与摘流（Railway 边缘在 A 死后仍返回 502，而不是摘除路由）。

## 4. 复现（多机）

```bash
# 1) 3 台 sandbox，同一个 environment，--private-network；两台带公开端口
railway sandbox create -p <project> -e <env> --private-network --domain 18091 --idle-timeout-minutes 60 --json
railway sandbox create -p <project> -e <env> --private-network --domain 18092 --idle-timeout-minutes 60 --json
railway sandbox create -p <project> -e <env> --private-network --idle-timeout-minutes 60 --json
#    私有域名取 GraphQL sandbox.privateDomain（形如 <id-prefix>.railway.internal，A+AAAA）

# 2) 每台：下载 etcd 3.5.17 + 上传 release 版 nebula-gateway 与 scripts/mock_openai_engine.py
# 3) 三台按 n1/n2/n3 起 3 节点 etcd（listen 0.0.0.0，advertise 用私有域名；peer 2380）
# 4) 在任意节点写 /tenants/{rps,conc,tiny,fail,recover}
# 5) A/B 各起一个 gateway：NEBULA_MULTI_TENANT=1、token→tenant 绑定、
#    NEBULA_GATEWAY_QUOTA_MEMBERS={1|2}、ETCD_ENDPOINT=三节点、--router-url=http://<C 私有域名>:19999
# 6) 按 §2 的表逐个场景发请求；kill 用 pkill -f 'etcd --name n1' / 'nebula-gateway --listen-addr'
# 7) 收尾：railway sandbox destroy ×3 + environmentDelete
```

注意两个坑：沙箱私有域名只解析到 **IPv6 优先**且 A/AAAA 并存，所以 etcd 与 mock 引擎都要 `0.0.0.0` 监听（不要同时绑 `[::]`，会 `address already in use`）；上传正在运行的二进制会报 `text file busy`，先 `pkill` 再上传。
