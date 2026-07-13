# HA 演练报告 — 2026-07-11

| 字段 | 值 |
|------|-----|
| 环境 | 真机 `bodesi@61.163.103.118:60001` / 8×RTX 5090 |
| 主机内核 | Linux 6.8.0-134-generic x86_64 |
| 执行人 | agent + `scripts/phase_d_ha_drill.sh` |
| 开始 | 2026-07-11T14:14:59Z (UTC) |
| 结束 | 2026-07-11T14:29:32Z (UTC)（含鉴权重启与补测） |
| 代码路径 | `/data/nebula` |
| 模型 | `gemma4_31b`（vLLM docker `nebula-gemma4_31b-1`，TP=4，GPU 4–7） |
| 基线拓扑 | 单 etcd（docker `nebula-etcd`）+ 单 gateway/router/scheduler/node |
| 演练拓扑 | nginx LB + gateway×2 + router×2 + scheduler×2；并行 etcd 3 节点集群 |

原始日志：`/data/nebula/logs/ha-drill/`（远程）。

---

## 场景结果

| 场景 | 结果 | 成功率 | RTO | 备注 |
|------|------|--------|-----|------|
| 杀 gateway 副本 (a) | **PASS** | 100.0% (30/30) | ~9.7s 探测窗 | nginx → gateway-b；healthz 无失败 |
| 杀 router 副本 (a) | **PASS** | 100.0% (30/30) | ~9.7s 探测窗 | nginx → router-b；补测再杀 a 仍 20/20 |
| 杀 bff 副本 | **SKIP** | n/a | n/a | 本机未部署 BFF / Postgres |
| 杀 etcd 单节点（并行 3 节点之成员 3） | **PASS** | n/a | ~2s | `get /ha-drill/ping` 仍为 `ok`；见下文「生产 etcd」限制 |
| 杀 scheduler leader | **PASS** | n/a | **10s** | leader `:18082` → follower `:18083` healthz=200 |
| 旧主复活 fencing | **PASS** | n/a | n/a | 复活后 a=503 / b=200，**恰好一个 leader** |
| 下线 GPU 节点 agent | **PASS** | n/a | node key drop **58s** | 仅停 `nebula-node`；vLLM 容器仍 Up；node 重新注册 |
| 演练后推理冒烟 | **PASS**（补测） | 100% | n/a | gateway LB → router → engine 返回 200 |

---

## 观察指标

### 基线（演练前）

| 探针 | HTTP |
|------|------|
| gateway `/healthz` | 200 |
| router `/healthz` | 200 |
| scheduler `/healthz` | 200 |
| etcd `/health` | 200 |
| `POST /v1/chat/completions` | 200 |

### 扩容后（nginx + 双副本）

```
Client → nginx:8081 → gateway-a:18091 / gateway-b:18092
       → nginx:18081 → router-a:19081 / router-b:19082 → engine :10815
scheduler-a:18082 + scheduler-b:18083  (etcd election)
node:9091 + 生产 etcd:2379 (nebula-etcd 单节点)
并行 etcd: ha-etcd1/2/3 → 12379/12380/12381
```

- gateway 直连 healthz：a=200 b=200  
- router 直连 healthz：a=200 b=200  
- scheduler：leader=200 / follower=503（符合 fencing / LB 门禁约定）  
- 终态推理（`NEBULA_AUTH_DISABLED=1` 与主机原配置对齐后）：**200**，模型 `gemma-4-31b-it`  
- GPU 下线期间 engine 容器：`Up 6 hours`（故障域 = agent 进程，非引擎进程）  
- endpoint 在 node 恢复后仍为 ready：`/endpoints/gemma4_31b/1` → `http://127.0.0.1:10815`

### Scheduler 切换细节

1. 扩容后短暂双 503（选主中），随后 a=200 b=503  
2. kill leader a → **约 10s** 内 b 升为 200  
3. 复活 a → a=503 b=200（旧主不再写；fencing 行为符合预期）  
4. placement `leader_epoch` 观测到 **2**（相对演练前升高）

### etcd 3 节点（并行集群）

- 镜像：`quay.io/coreos/etcd:v3.5.16`  
- kill `ha-etcd3` 后，经 `ha-etcd1` 读写成功  
- **注意：生产控制面仍使用单节点 `nebula-etcd`**；客户端仍为单 `ETCD_ENDPOINT`。3 节点存活能力已在旁路集群验证，尚未切入生产数据面。

---

## 问题与跟进

1. **生产 etcd 仍单节点**  
   完整 DoD「杀 1/3 etcd 后控制面仍可用」需迁移生产 keyspace 到 3 节点，并启用客户端多 endpoint（源码已支持逗号分隔 `ETCD_ENDPOINT`，本机 release 二进制需重编部署）。

2. **BFF HA 未演练**  
   主机无 BFF/Postgres；控制台路径 SKIP。

3. **鉴权环境变量**  
   重启 gateway/router 时若未带主机原有 `NEBULA_AUTH_DISABLED` / `NEBULA_AUTH_TOKENS`，默认鉴权开启且无 token → 推理 401。属运维配置问题，非 HA 逻辑故障；补测对齐后 200。建议 `nebula-up`/systemd unit 固化 env。

4. **Node lease 过期 ~58s**  
   停 agent 后 `/nodes/` 约 58s 消失；可按需缩短 lease/TTL 以加快摘流（产品取舍）。

5. **本地 rustup 曾中断**  
   远程 toolchain 曾在升级中损坏；本演练使用已有 `target/release` 二进制。后续生产 etcd 多 endpoint 切换前应 `cargo build --release` 一次。

---

## Definition of Done 对照

| 条目 | 本报告 |
|------|--------|
| 任一单接入副本故障不影响对外 API | **满足**（gateway/router） |
| gateway/bff 任一副本故障不影响对外 API | gateway **满足**；bff **未测** |
| scheduler leader 故障自动切换且 fencing | **满足**（~10s；旧主 503） |
| 单台 GPU 离线仅影响该节点副本 | **满足**（agent 下线；引擎容器保留；可恢复） |
| 故障期间成功率/延迟/恢复时间可观测 | **满足**（本报告 + 远程 `logs/ha-drill`） |
| election/fencing CI 单测 | **仓库已有**（非本真机项） |
| 生产 etcd 三节点 + 客户端多 endpoint | **未完成**（旁路集群 PASS） |

**结论：** Phase D 真机演练在**接入多副本、scheduler HA/fencing、GPU agent 故障隔离、旁路 etcd 多数派**上通过。  
**剩余签字项：** 生产 etcd 迁 3 节点 + 多 endpoint 客户端；可选 BFF 多副本。

---

## 复现

```bash
ssh -p 60001 bodesi@61.163.103.118
bash /data/nebula/scripts/phase_d_ha_drill.sh
# 报告草稿：/data/nebula/logs/ha-drill/report.md
# 仓库归档：docs/dev/ha/report.md
```

相关文档：[`runbook.md`](./runbook.md)、[`../../arch/roadmap.md`](../../arch/roadmap.md)。
