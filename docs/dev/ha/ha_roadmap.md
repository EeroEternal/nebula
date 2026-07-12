# Nebula HA 执行路线图

> 状态：Phase A–D 主体完成；**生产 etcd 三节点迁移暂缓**（2026-07-11 决策）  
> 更新时间：2026-07-12  
> 原则与架构：[`../../arch/architecture.md`](../../arch/architecture.md)  
> 排期真源：[`../../arch/optimization.md`](../../arch/optimization.md)  
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

排期见 [`optimization.md`](../../arch/optimization.md)（O8 收尾 / 按需 N3–N4）；etcd 生产迁移不排期。

---

## 3. 组件角色

| 组件 | HA 角色 |
|------|---------|
| `gateway` / `bff` | 无状态多副本，LB 后；不参与选主 |
| `router` | 无状态多副本；只读 etcd endpoint；认 `plan_version` |
| `scheduler` | 2~3 副本 + etcd election；仅 leader 写；follower `/healthz`=503 |
| `etcd` | 权威存储；**生产默认单节点**；三节点旁路已验证，SLA 需要时再迁 |
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

### Phase C — 接入与元数据层多副本（已完成主体）

1. ~~etcd 3 节点生产迁移~~ → ⏸ 暂缓（旁路验证即可）
2. gateway / bff / router 至少 2 副本 + LB 健康检查 → ✅ 真机
3. postgres 主备或托管 → 可后置

**验收：** 杀单一接入副本，控制面与推理仍可用（见报告）。

### Phase D — 全链路演练（已完成）

1. 杀接入层副本
2. 杀 scheduler leader（含旧主复活脑裂）
3. 下线单台 GPU 节点
4. 记录成功率、RTO → [`report-20260711.md`](./report-20260711.md)

**验收：** 真机报告 PASS；CI election/fencing 单测仍为门禁。

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

| 优先级 | 工单 | 映射 | 状态 |
|--------|------|------|------|
| — | 生产 etcd 迁 3 节点 | Phase C 收尾 | ⏸ 暂缓 |
| — | 其余工程 | [`optimization.md`](../../arch/optimization.md) | O8 / 按需 N3–N4 |

N1 接入多副本、Phase D 报告、N2 CI/质量均已关闭，勿重复开 issue。

---

## 8. 维护

每完成一阶段更新本页状态与 `report-*.md`。排期只改 [`optimization.md`](../../arch/optimization.md)，避免与 architecture 双写「下一步」。
