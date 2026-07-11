# 开发计划

> 当前冲刺对齐 [`../arch/architecture.md`](../arch/architecture.md) Wave A–C（2026-07-11 代码审查后）。  
> 历史里程碑（鉴权/观测/CLI/多引擎等）见文末「已完成基线」。

## 目标

在不改架构方向的前提下，先修 **多副本 / 缩容 / Drain** 三类正确性缺陷，再补恢复与控制面一致性，最后做热路径与工程质量。交付后 M1（多机调度闭环）才算真正可用。

## 总工期与批次

约 **3 周**（1 人全职估算；可两人并行压缩到 ~2 周）。

| 批次 | 天数 | 主题 | 对应 Wave |
|------|------|------|-----------|
| Sprint 1 | 5 天 | 正确性三件套 | A0 / A1 / A2 |
| Sprint 2 | 5 天 | 恢复 + Router 一致性 | B1–B3 / A3 |
| Sprint 3 | 5 天 | 声明式收敛 + 性能 | B4 / B5 / C1–C4 |

按需项（D3 无状态多副本、D4 跨节点 TP、D5 EngineShim）不进本计划。

---

## Sprint 1 — 正确性（P0，必须先做）✅

依赖顺序：**A0 → A2 → A1**（多副本状态模型是 Drain/缩容的前提；Drain 推进是缩容可观测完成的前提）。

### Day 1–2：A0 同节点多副本 ✅

| 项 | 内容 |
|----|------|
| 改动 | `nebula-node`：`running` / endpoint 本地态键改为 `(model_uid, replica_id)`；`reconcile` 对本节点 **全部** assignment 做集合差量（启/停/滚动），禁止 `find` 只取第一个 |
| 牵连 | `heartbeat.rs`、健康检查、stats scrape 凡按 `model_uid` 索引运行态处一并改 |
| 验收 | 单节点 `replicas=2` → 两个引擎进程/容器 + 两个 `/endpoints/{uid}/{rid}`；Router 可选中两者 |
| 测试 | Node reconcile 单测：两 assignment 同 node；set-diff 识别 excess/missing |

### Day 3：A2 Drain 周期推进 ✅

| 项 | 内容 |
|----|------|
| 改动 | Node 增加 `periodic_full_reconcile_loop`（3s）：对有本地 assignment/running 的 model 全量 reconcile；无 placement 的 orphan 走 drain |
| 验收 | 删除 placement 后：先标 Draining，pending=0 或超时后停引擎并删 endpoint；无新 watch 事件也能完成 |

### Day 4–5：A1 自动缩容 ✅

| 项 | 内容 |
|----|------|
| 改动 | `nebula-scheduler`：`healthy > desired` 时 `select_replicas_to_remove`（低 pending 优先）并截断 assignment CAS 写回；`need_update` 覆盖副本过多 |
| 验收 | scale 后 plan 变短；单测覆盖选副本逻辑 |

**Sprint 1 出口：** 同节点双副本可跑；缩容与卸载能真正停引擎。

---

## Sprint 2 — 恢复与控制面一致性 ✅

可并行：B1（Node）∥ B2+B3（Router）；A3 依赖真实引擎环境。

### Day 6–7：B1 恢复预算 + 进程树 ✅

| 项 | 内容 |
|----|------|
| 策略 | 本地 `process_group(0)` + SIGTERM→等待→SIGKILL；`try_restart` 对本地 stop+start 重建 handle，Docker 走 `docker restart` |
| 预算 | 每 replica：24h 内 5 次 + 指数退避；超限写 `EndpointStatus::Failed` + `ModelRequestStatus::Failed`，停止空转 |
| 验收 | 预算单测通过；必崩配置停止重启循环 |

### Day 8–9：B2 / B3 Router watch 与 plan_version ✅

| 项 | 内容 |
|----|------|
| B2 | `list_prefix_snapshot` 用 header revision 启 watch；compact/cancel 后全量 resync；endpoints + placements |
| B3 | Router 内 `model_uid → plan_version`；handlers 对所有模型统一过滤 |
| 验收 | 多模型 plan_version 单测；旧 plan endpoint 不可被选 |

### Day 10：A3 取消传播契约测试 ✅

| 项 | 内容 |
|----|------|
| 改动 | `scripts/test_cancel_sse.sh`：客户端中断 SSE → `requests_aborted_total` +1 且 5xx 不增 |
| 验收 | 对运行中 Router：`MODEL=... ./scripts/test_cancel_sse.sh` |

**Sprint 2 出口：** 故障不会无限空转；Router 状态与 plan_version 可信。

---

## Sprint 3 — 收敛与性能（进行中）

### Day 11：B4 placement version 单调 ✅

CAS 写 plan 时 `version = old.version + 1`；`updated_at_ms` 存墙钟；grace/cooldown 改读 `effective_updated_at_ms()`。

### Day 12–13：B5 声明式单路径 ✅

Gateway/BFF load·scale·stop 写 `/deployments/`（+ spec）；Scheduler 只 watch `/deployments/`；reconcile 去掉 model_requests fallback。迁移说明：[`migrate_deployments.md`](./migrate_deployments.md)。

### Day 14：C1 header-driven 流式 ✅

Gateway 轻量 peek `model` → 注入 `x-nebula-model`；Router 优先 header 选路，model 字段字节级改写（无完整 JSON DOM）。

### Day 15：C2 / C3 / C4

| 项 | 状态 | 内容 |
|----|------|------|
| C2 | ✅ | heartbeat：`try_lock` 快照后锁外 health/scrape；reconcile：锁内快照/提交，download/start/stop/drain 在锁外 |
| C3 | ✅ | Node 启动 `grant_lease` + keepalive；status/endpoints 共用 `put_with_lease` |
| C4 | 进行中 | 单测覆盖 B4/C1；文档已同步；workspace 测试跟进 |

**Sprint 3 出口：** 声明式单路径；大 body 不双层缓冲；lease/锁无显著浪费。

---

## 分工建议（2 人时）

| 角色 | Sprint 1 | Sprint 2 | Sprint 3 |
|------|----------|----------|----------|
| Owner-Node | A0、A2 | B1、A3 | C2、C3、相关测试 |
| Owner-Control | A1（Scheduler） | B2、B3 | B4、B5、C1、C4 文档 |

每日以 architecture 表中「验收」列为 Done 定义；不达标不进入下一批次核心项。

## 风险

| 风险 | 缓解 |
|------|------|
| A0 牵连面大，半改半留更危险 | 先改类型与索引，再改 reconcile；同一 PR 内跑通双副本再合 |
| 缩容选错副本打断 affinity 会话 | 首期显式优先「无 affinity / pending=0」；metric 记录被摘 replica |
| 本地进程树与 Docker 行为不一致 | Sprint 2 选定一条生产路径写死，另一条标实验性 |
| B5 迁移打断旧客户端 | 先双读一版（仅读旧 key 告警），再删写路径 |

## 明确不做（本计划内）

EngineShim / capabilities、跨节点 TP、Gateway/Router 水平扩容与 etcd 三节点（见 HA roadmap）、Web Console 大功能、硬件感知调度与模型预热 UI。

---

## 已完成基线（归档摘要）

控制面链路、Prometheus/metrics、结构化日志、统一鉴权、审计、多引擎（vLLM/SGLang）、镜像管理、CLI（chat/logs/metrics/drain/scale/whoami）、Router 策略/熔断/过载、Scheduler reconcile + CAS + election/fencing、Gateway SSE abort 传播——均已完成。旧「Week 1–6 功能补齐」计划已关闭。

工程杂项与更细 P0/P1 清单见 [`../arch/optimization.md`](../arch/optimization.md)。
