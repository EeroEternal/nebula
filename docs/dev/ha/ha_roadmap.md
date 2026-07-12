# Nebula HA 执行路线图

> 状态：Phase A–D 主体完成；**生产 etcd 三节点迁移暂缓**（2026-07-11 决策）  
> 更新时间：2026-07-11  
> 原则与架构：[`../../arch/architecture.md`](../../arch/architecture.md)  
> 真机报告：[`report-20260711.md`](./report-20260711.md)

Scheduler 选主 / fencing / Drain、接入多副本真机演练已完成。生产 `nebula-etcd` 保持单节点即可；三节点能力已在旁路集群验证，需要生产 SLA 时再迁。

---

## 1. 目标

多机下控制面与接入面高可用；GPU 执行层故障可隔离、可恢复。对齐 PowerLLM 的 HA **行为**（lease 选主、fencing、`/healthz` LB 门禁、可重复测试），用 etcd + Rust reconcile 实现，不引入 Actor / binlog 旧 HA。

---

## 2. 当前状态（2026-07-11）

- 多机部署基础：有
- Scheduler 多副本选主 + fencing：✅（Wave D1）
- Drain 闭环：✅（Wave A2 / D2）
- 控制面正确性（多副本 / 缩容 / `/stats/` / deployments）：✅
- etcd / gateway / bff / router 多副本拓扑：✅（`docker-compose.ha.yml` + `deploy/ha/`；见 [runbook-phase-c.md](./runbook-phase-c.md)）
- 多 etcd endpoint 连接串：✅（`ETCD_ENDPOINT` 逗号分隔）
- PR CI 全量 gate：✅（`cargo test --workspace`）
- Phase D 真机演练报告：✅（[report-20260711.md](./report-20260711.md)）
- 生产 etcd 三节点：⏸ **暂缓**（旁路 3 节点已验证；不阻塞当前排期）

下一优先：按 [`optimization.md`](../../arch/optimization.md) 开 N3/N4（业务驱动）；etcd 迁移不排期。

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

### Phase A — 调度正确性（已完成）

对应已关闭的选主 / fencing / CAS 工作。

1. `nebula-meta`：election / lease API，`leader_epoch`
2. `nebula-scheduler`：leader-only 写；follower healthz 503
3. Placement fencing + CAS；Node 拒落后 plan
4. In-process 双 scheduler 测试进 CI

**验收**：双实例单 leader；切换 &lt;10s；旧主写被拒；CI 绿。

### Phase B — Drain 与请求生命周期（已完成）

1. Draining：不接新流量；in-flight 完成再停引擎、删 endpoint
2. Abort / disconnect：Gateway → Router → 断引擎；metrics 口径正确

**验收**：缩容与取消不误伤成功率口径；无孤儿 endpoint。

### Phase C — 接入与元数据层多副本（= optimization N1，当前主线）

1. etcd 3 节点；组件改连集群 endpoint
2. gateway / bff / router 至少 2 副本 + LB 健康检查
3. postgres 主备或托管（可与本阶段并行，不阻塞）

**验收**：杀单一 etcd/接入副本，控制面与推理仍可用。

### Phase D — 全链路演练

1. 杀接入层副本
2. 杀 scheduler leader（含旧主复活脑裂）
3. 下线单台 GPU 节点
4. 记录成功率、RTO；报告写入 `docs/dev/ha/report-*.md`

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

## 7. 工单拆分（剩余）

Phase A/B（选主、fencing、Drain、abort、CAS）已完成。当前只跟：

| 优先级 | 工单 | 映射 | 状态 |
|--------|------|------|------|
| P0 | etcd 3 + gateway/bff/router 多副本 + LB | optimization **N1** / Phase C | 拓扑 ✅；真机 gateway/router×2 ✅ |
| P0 | HA 演练报告 `report-*.md` | Phase D | ✅ [report-20260711.md](./report-20260711.md) |
| — | 生产 etcd 迁 3 节点 + 客户端多 endpoint | Phase C 收尾 | ⏸ 暂缓 |
| P1 | CI/工程质量（BFF 去重等） | optimization **N2** | ✅ |

已关闭项勿再开 issue 重复做。详见 [`../../arch/optimization.md`](../../arch/optimization.md)。

---

## 8. 维护

每完成一阶段更新：实施状态、风险与回滚、验收结果（或链接到 `report-*.md`）。进度与 [`../../arch/optimization.md`](../../arch/optimization.md) N1 同步。
