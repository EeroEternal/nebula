# Nebula HA 执行路线图

> 状态：执行中（与工程成熟度清单对齐）
> 更新时间：2026-07-10
> 主清单：[`../engineering_maturity_checklist.md`](../engineering_maturity_checklist.md)
> 原则：[`../absorb_powerllm_engineering_guidance.md`](../absorb_powerllm_engineering_guidance.md)

本文件从「规划中暂不改代码」转为**执行清单**。控制面正确性（选主 / fencing / CI）优先于基础设施扩容（etcd 三节点）；顺序与旧稿 Phase 1→3 对调。

---

## 1. 目标

多机下控制面与接入面高可用；GPU 执行层故障可隔离、可恢复。对齐 PowerLLM 的 HA **行为**（lease 选主、fencing、`/healthz` LB 门禁、可重复测试），用 etcd + Rust reconcile 实现，不引入 Actor / binlog 旧 HA。

---

## 2. 当前状态

- 多机部署基础：有
- Scheduler 多副本选主 + fencing：无（Wave 0 主线）
- Drain 闭环：弱（Draining 近似硬切）
- PR CI 跑 HA/契约测试：无
- etcd / gateway / bff / router 生产多副本：弱（Wave 2 / P1.4）

---

## 3. 组件角色

| 组件 | HA 角色 |
|------|---------|
| `gateway` / `bff` | 无状态多副本，LB 后；不参与选主 |
| `router` | 无状态多副本；只读 etcd endpoint；认 `plan_version` |
| `scheduler` | 2~3 副本 + etcd election；仅 leader 写；follower `/healthz`=503 |
| `etcd` | 权威存储；先单节点跑通选主，再上 3 节点 |
| `postgres` | 用户/会话等；不参与 Placement 权威；HA 可后置 |
| `node` | 按 GPU 机器扩展；拒落后 epoch；执行 drain stop |

---

## 4. 执行阶段（按顺序）

### Phase A — 调度正确性（= 清单 Wave 0 的 HA 部分）

对应清单：`W0.1`–`W0.6`。

1. `nebula-meta`：election / lease API，`leader_epoch`
2. `nebula-scheduler`：leader-only 写；follower healthz 503
3. Placement fencing + CAS；Node 拒落后 plan
4. In-process 双 scheduler 测试进 CI

**验收**：双实例单 leader；切换 &lt;10s；旧主写被拒；CI 绿。

### Phase B — Drain 与请求生命周期

对应清单：`W0.7`、`P0.4`。

1. Draining：不接新流量；in-flight 完成再停引擎、删 endpoint
2. Abort / disconnect：Gateway → Router → 断引擎；metrics 口径正确

**验收**：缩容与取消不误伤成功率口径；无孤儿 endpoint。

### Phase C — 接入与元数据层多副本（原 Phase 1–2，延后）

对应清单：`P1.4`。

1. etcd 3 节点；组件改连集群 endpoint
2. gateway / bff / router 至少 2 副本 + LB 健康检查
3. postgres 主备或托管（可与本阶段并行，不阻塞 A/B）

**验收**：杀单一 etcd/接入副本，控制面与推理仍可用。

### Phase D — 全链路演练

1. 杀接入层副本
2. 杀 scheduler leader（含旧主复活脑裂）
3. 下线单台 GPU 节点
4. 记录成功率、RTO；报告写入 `docs/ha/report-*.md`

**验收**：满足下方 Definition of Done；真机演练不替代 Phase A 的 CI 单测。

---

## 5. 建议拓扑（目标态）

- 接入：`LB → gateway(2+) →`（BFF 同理多副本）
- 控制：`router(2+)`，`scheduler(2~3, leader/follower)`
- 元数据：`etcd(3)`，`postgres(HA)`
- 执行：`node(N)` + engine instances

---

## 6. Definition of Done（HA 基线）

- 任一单节点故障不导致整体不可用（在 Phase C 拓扑下）
- gateway/bff 任一副本故障不影响对外 API
- scheduler leader 故障后在目标窗口内自动切换，且旧主 fencing 生效
- 单台 GPU 离线仅影响该节点副本
- 故障期间成功率、延迟、错误码、恢复时间可观测
- **election / fencing / healthz 行为有 CI 单测，且 PR 必跑**

---

## 7. 工单拆分（可直接建 issue）

| 优先级 | 工单 | 清单 ID |
|--------|------|---------|
| P0 | meta election + scheduler leader-only + healthz 503 | W0.1, W0.2 |
| P0 | placement epoch/CAS + node 拒写 + 脑裂单测 | W0.3, W0.4, W0.5 |
| P0 | GitHub Actions：`cargo test` gate | W0.6 |
| P0 | Drain 闭环 | W0.7 |
| P0 | Abort 传播 + CAS 清零 + API 边界 | P0.4, P0.5, P0.7 |
| P1 | etcd 3 + 接入多副本 + 演练报告 | P1.4, Phase D |

---

## 8. 维护

每完成一阶段更新：实施状态、风险与回滚、验收结果（或链接到 `report-*.md`）。与 `engineering_maturity_checklist.md` 的 `[x]` 同步勾选。
