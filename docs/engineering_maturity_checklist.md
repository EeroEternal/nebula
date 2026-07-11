# Nebula 工程成熟度优化清单

> 状态：执行清单（2026-07-10）
> 目标：达到与 PowerLLM 同级的生产工程完备度（HA / 生命周期 / CI / 运维），不复制 Actor 实现
> 原则文档：`absorb_powerllm_engineering_guidance.md`
> 相关：`ha/ha_roadmap.md`、`arch_optimization.md`、`unigateway_integration.md`、`development_plan.md`

用法：按 Wave 顺序做；每项完成后把 `[ ]` 改成 `[x]`，并补上 PR / 验收备注。对齐的是**行为与验收**，不是 PowerLLM 类名。

---

## 总原则（每次开工前过一遍）

- 短推理路径不变：`Gateway → Router → Engine HTTP`。
- etcd 是唯一权威；禁止内存权威 + 事后 rectify 双源。
- 不引入 xoscar / 上帝 Scheduler / 引擎内嵌 / Scheduler 直连 Postgres。
- UniGateway 只嵌 Gateway 协议层，不替代 Router（见 `unigateway_integration.md`）。
- 每项必须有可重复测试或演练记录；进 P0 的行为必须能进 CI。

---

## Wave 0（当前动手点）：选主 + fencing + CI + drain 语义

目标：单机可演示「双 scheduler 杀主 + 旧主拒写 + drain 不硬切」；PR 无测试不能合。

| ID | 项 | 组件 | 要做的事 | 验收 | 状态 |
|----|----|------|----------|------|------|
| W0.1 | Scheduler 选主 | `nebula-meta`, `nebula-scheduler` | etcd election/lease；仅 leader 写控制面；暴露 `leader_epoch` | 双实例恰好 1 leader；kill 后 &lt;10s 接管 | [x] |
| W0.2 | follower healthz | `nebula-scheduler` | leader `/healthz`→200；follower→503（readiness） | LB/探针可摘备；单测覆盖 200/503 | [x] |
| W0.3 | Placement fencing | `nebula-common`, `scheduler`, `node` | 写入带 epoch/version；CAS；Node 拒落后 plan | 旧 leader 写被拒；脑裂演练通过 | [x] |
| W0.4 | 主路径 CAS | `nebula-scheduler` | 演示路径上 placement 禁止裸 `put`（与 P0.5 衔接） | CAS 冲突单测；失败重读或跳过本轮 | [x] |
| W0.5 | HA in-process 测试 | `scheduler` / `meta` tests | 2 scheduler + 测试 etcd 或可注入 meta | 选主 / failover / fencing / healthz 四条主线 | [x] |
| W0.6 | PR CI gate | `.github/workflows` | PR 必跑 `cargo test --workspace`（或等价最小集含 W0.5） | 红则不能合；文档写清如何跑 | [x] |
| W0.7 | Drain 语义修正 | `router`, `node`, `scheduler` | Draining 不接新流量；in-flight 跑完再 stop；再删 endpoint / assignment | 流式请求可完成；无孤儿 endpoint | [x] |

**Wave 0 完成标志**：本地 compose/脚本能跑「杀 leader + 缩容 drain」；上述测试在 CI 绿。

建议实现顺序：`W0.1 → W0.2 → W0.3/W0.4 → W0.5 → W0.6`，`W0.7` 可并行但不要晚于 Wave 0 收尾。

---

## Wave 1（P0 收尾）：生命周期与边界

在 Wave 0 之后做，仍属可靠性基线。

| ID | 项 | 组件 | 要做的事 | 验收 | 状态 |
|----|----|------|----------|------|------|
| P0.4 | Request abort | `gateway`, `router` | disconnect/abort 取消下游；断开引擎连接 | 取消后停生成；abort 指标单独标签 | [x] |
| P0.5 | Placement CAS 清零 | `nebula-scheduler` | `arch_optimization.md` 中直接 `put` placement 路径清零 | grep/审计无裸 put；冲突可恢复 | [x] |
| P0.6+ | 最小契约集扩面 | `router`, `gateway`, CI | `plan_version` 过滤、CAS 冲突、SSE 序列、auth 拒绝 | 契约测试进同一 CI gate | [x] |
| P0.7 | API 边界收敛 | `gateway`, `bff`, `router` | 每个 API 唯一 owner；禁止 Gateway 吞 Router | 文档表 + 代码路径一致 | [x] |
| P0.UG | Gateway 编译/UG 边界 | `nebula-gateway` | 恢复可编译；UG 仅协议层（见 `unigateway_integration.md`） | workspace check 通过；无绕过 Router 默认路径 | [x] |

**Wave 1 完成标志**：P0 表（指导意见 §5）全部勾完；「杀主 + drain + abort」可演示且 CI 挡住回退。

---

## Wave 2（P1）：生命周期生产化与交付

| ID | 项 | 要做的事 | 验收 | 状态 |
|----|----|----------|------|------|
| P1.1 | Async launch + LaunchProgress | 状态机分阶段；progress 写 etcd；BFF/CLI 可读 | API 不阻塞；UI/CLI 可见进度 | [ ] |
| P1.2 | Autoscale 生产化 | cooldown；scale-down **必须**走 Wave0 drain | 无硬切缩容；有 runbook | [ ] |
| P1.3 | Metadata reconcile | `POST /admin/reconcile` + 周期全量校正 | 孤儿 endpoint 可清理 | [ ] |
| P1.4 | 多副本拓扑 | etcd 3 节点；gateway/bff/router 多副本 + LB | 对齐 `ha/ha_roadmap.md` 执行段 | [ ] |
| P1.5 | SLO 落地 | 成功率 / TTFT P95 / 流式中断；abort/drain 不算错；xtrace 与 PromQL 口径分述（见 `observability.md`） | dashboard + 告警规则 | [ ] |
| P1.6 | 一键部署 | compose profile 或 Helm 初版 | 新环境约 30 分钟 E2E | [ ] |

**Wave 2 完成标志**：小集群可日常用；扩缩容与发布有 runbook。

---

## Wave 3（P2）：深度能力（按产品取舍，非工程基线必做）

| ID | 项 | 约束 | 状态 |
|----|----|------|------|
| P2.1 | PD 分离 | Placement role + Router 策略；不引入 PDModelActor | [ ] |
| P2.2 | 多模态 | 容器 / VirtualEngine；禁止嵌入 Rust | [ ] |
| P2.3 | License | 仅 BFF/Gateway | [ ] |
| P2.4 | Batch API | 可选；独立 job 空间 | [ ] |
| P2.5 | 前端运维流 | 监控 / 审计 / 告警 / 多集群；产品内走 BFF+xtrace；客户 VM/Loki 仅标准出口（见 `observability.md`） | [ ] |
| P2.6 | 硬件感知调度 | `(硬件, 引擎, 模型) → 镜像` | [ ] |
| P2.UG | 协议增强 | Anthropic / Responses 等经 UG protocol 嵌 Gateway | [x] |

---

## 明确不做（吸收时自检）

- 把 Scheduler 做成新的上帝服务。
- 引擎嵌回控制面；用 Python venv 替代镜像隔离。
- Gateway 复制 enterprise 单体或吞掉 Router。
- 上 HA 却不把 election/fencing 测进 CI。
- 开 autoscale scale-down 却未修 drain。
- License/用户体系让 Scheduler 直连 Postgres。
- 同时维护旧 binlog HA 与新 lease HA。
- 只有 metrics 没有 SLO 口径（abort/drain 是否算错）。

---

## 与现有文档的关系

| 文档 | 角色 |
|------|------|
| `observability.md` | 可观测统一设计（xtrace / Prometheus / Loki、面板、P1.5·P2.5） |
| `absorb_powerllm_engineering_guidance.md` | 为什么、学什么、不学什么 |
| **本文件** | 按什么顺序做、做到什么算完 |
| `ha/ha_roadmap.md` | HA 拓扑与多机交付细节（执行态，服务 Wave 0–2） |
| `arch_optimization.md` | CAS / 边界 / Gateway 编译等债项，由本清单消化 |
| `unigateway_integration.md` | UG 引入边界；不阻塞 Wave 0 |

---

## 进度约定

1. 同一时间只推进一个 Wave 的「进行中」主线；旁路债项可修但不抢优先级。
2. 合并 PR 时更新本文件对应 `[x]`，并在 PR 描述引用 ID（如 `W0.3`）。
3. Wave 0 未完成前，不把 P2 企业/多模态当主线。
4. 真机多节点演练记录放到 `docs/ha/report-*.md`；CI 单测不过不能用「真机过了」代替。
