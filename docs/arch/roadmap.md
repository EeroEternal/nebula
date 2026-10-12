# Nebula 下一步优化计划

> 更新：2026-08-08（对齐 [`vision.md`](./vision.md) Phase）。已完成项摘要见 [`architecture.md`](./architecture.md)；产品总纲与能力分层见 vision；批次勾选见本文与 [`../dev/plan.md`](../dev/plan.md)；Release Notes 见 [`../versions/v1.3.0.md`](../versions/v1.3.0.md)。

## 当前进展（一句话）

M1 / N1 HA 主体 / N2 / O1–O8 / **产品对齐 P0–P6 Batch 1** 已随 **v1.3.0** 发布（L0 底座 + L1/L3/L4 雏形）。**Phase 0 真机通路径 + 多租户隔离压测 + 轻量 SLO burn + etcd HA runbook 已落地**（Gateway→Router→引擎；双引擎；声明式拉起；A 限流/B 隔离；abort 不计 5xx；[`manual/etcd-ha.md`](../manual/etcd-ha.md)）；生产 etcd 三节点切入仍按维护窗。

## 优先级总表

| 批次 | ID | 主题 | 状态 | 说明 |
|------|----|------|------|------|
| **N2** | Q1–Q4 | 工程质量 | ✅ | BFF 去重、HTTP client、observe、CI |
| **N1** | D3 | 接入/调度 HA | ✅ 主体 / ⏸ 生产 etcd 切入 | 真机报告；runbook [`../manual/etcd-ha.md`](../manual/etcd-ha.md) |
| **N4-Obs** | O1–O10 | 可观测 | ✅ O1–O8 / ⏸ O9–O10 | SLO runbook + 告警样例已落；O9 按需 |
| **Product P0** | P0 | 契约与观测可信度 | ✅ Batch 1–2 | 见 [`../dev/plan.md`](../dev/plan.md) |
| **Product P1** | P1 | Engine Capability / Adapter | ✅ Batch 1–3 / ✅ 真机通路径 | 含能力 etcd 持久化、版本支持表；5090 上 SGLang Node 声明式已通；契约硬化 → Phase 1 |
| **Product P2** | P2 | Serving Cell 只读接入 | ❌ 已移除 | CellIngress / `/cells/` 已下线，不再宣传 |
| **Product P3** | P3 | 镜像平台 / 加速器台账 | ✅ Batch 1–2 | 兼容矩阵 + platforms 放置 + 库存 API/控制台；历史画像 → Phase 3 |
| **Product P4** | P4 | 统一 SLI / SLO | ✅ Batch 1 / ✅ 轻量 burn / ✅ 建议骨架 | ModelSlo CRUD/评估 + 诊断时间线；真机 abort 口径 + scrape evaluate；evaluate 按违约原因出建议；告警实触发 / Selection 半自动落地 → Phase 3 后续 |
| **Product P5** | P5 | Benchmark / 推荐 / Canary | ✅ Batch 1 / ✅ 双引擎通路径 | 证据面已落；5090 上 Qwen3.5-4B × SGLang+vLLM 经 Gateway 均 200；Selection 成层 → Phase 1 |
| **Product P6** | P6 | 多租户 / 配额 / 成本 | ✅ Batch 1 / ✅ 隔离压测 | Tenant + Gateway 准入 + 用量成本；真机双租户 RPS/ACL 隔离见 `scripts/phase0_tenant_isolation.sh`；成本联动 → Phase 3 |
| **N3** | D4/D5 | 按需能力 | ⏸ | 跨节点 TP / EngineShim → Phase 2（有需求再开） |
| **N4 其余** | UX | 产品化 | ⏸ | 硬件镜像、Console 大功能；选型向导属 Phase 1 |
| **K8s** | K0–K3 | K8s / HAMi 执行面 | ⏸ | 与 Phase 正交；有集群虚拟化客户再开；见 [`../dev/k8s.md`](../dev/k8s.md) |
| **Integration** | I0–I4 | Control API / 对外暴露 | ✅ | 见 [`../dev/integration-plan.md`](../dev/integration-plan.md) |

原则：不阻塞日常推理；不破坏 L0 主轴；智能/重治理不进推理热路径。

---

## 下一阶段顺序（对齐 vision Phase）

总纲路线见 [`vision.md`](./vision.md) §8。本文把既有 ⏸ 项映射到 Phase；Batch 勾选仍以本节下方历史表为准。

| 阶段 | 对应层 | 重点 | 状态 | 承接的既有项 |
|------|--------|------|------|----------------|
| **已交付** | L0 + 雏形 | P0–P6 Batch 1、N1 主体、O1–O8 | ✅ | v1.3.0 / v1.4.0 |
| **Phase 0** | L0/L4 地基 | 真机 e2e、多租户压测、观测 burn、etcd HA runbook | ✅ 通路径+压测+轻量 burn+runbook / ⏸ 生产切入 | 已：Gateway→Router→引擎；双引擎；Deployment→Placement→Node；`phase0_tenant_isolation.sh`；`phase0_slo_burn.sh`；[`../manual/etcd-ha.md`](../manual/etcd-ha.md)。待：维护窗迁生产 keyspace |
| **Phase 1** | L1 + L3 骨架 | 一致性契约套件；ModelProfile；Selection 推荐+草稿；控制台选型向导 | ✅ 骨架 + L1 契约硬化（含流式 tool_calls）+ Selection 可解释/向导加深 | [`selection.md`](./selection.md)（score/breakdown、`current` 自动填充）；[`../dev/contracts.md`](../dev/contracts.md)（C1–C6 ✅）；`/api/v2/selection/*` + 治理页 preference/platform |
| **Phase 2** | L2 | HardwarePool、池约束、PD/TP（按需）、故障隔离 | ⏸ 设计 ✅ / 代码有需求再开 | [`pool.md`](./pool.md)；N3 D4/D5；与 K8s 执行面正交 |
| **Phase 3** | L4 + L3 闭环 | SLO→建议→半自动；容量规划；成本与选择联动；企业 SSO/RBAC | ⏸ 建议✅ / 容量骨架✅ / Selection 联动暂缓 | SLO 可解释建议；`GET /api/v2/capacity` 只读缺口；半自动落地与成本反哺未做 |

旁路（不插入 Phase 主轴）：**K8s/HAMi K0–K1** — 有客户再开；边界见 [`AGENTS.md`](../../AGENTS.md)、[`../dev/k8s.md`](../dev/k8s.md)。
### Product P6 Batch 1 勾选

| 项 | 状态 |
|----|------|
| Tenant / Quota / Usage / Pricing 契约 + etcd | ✅ |
| Auth `token:role[:tenant_id]`；关闭多租户兼容单 token | ✅ |
| Gateway 准入 + ExecutionContext 传播；稳定拒绝码 | ✅ |
| 低基数 `tenant_denied_total{reason=}`；审计含 tenant/deny | ✅ |
| BFF + 控制台租户用量/成本视图 | ✅ |
| 真机多租户隔离压测 | ✅ `scripts/phase0_tenant_isolation.sh`（A RPS/ACL 拒、B 不受影响） |

### Product P5 Batch 1 勾选

| 项 | 状态 |
|----|------|
| `BenchmarkRun` / `PerformanceProfile` / recommend / canary 契约 | ✅ |
| `scripts/benchmark` workload + runner（dry-run / ingest） | ✅ |
| BFF `/benchmarks/*` + `/canaries/*`；不足数据不默引擎 | ✅ |
| 控制台 governance：runs / recommend / canary | ✅ |
| 真机双模型 × vLLM/SGLang 通路径（Qwen3.5-4B @ 5090） | ✅ 通路径（benchmark 重复跑 ⏸） |

### Product P4 Batch 1 勾选

| 项 | 状态 |
|----|------|
| `ModelSlo` 契约 + etcd `/slos/{model_uid}` CRUD | ✅ |
| `GET .../evaluate`：Router scrape 对比目标；低流量 `insufficient_data`（不假绿） | ✅ |
| abort/Drain 排除错误预算语义写入评估结果 | ✅ |
| `DiagnosticEvent` 聚合 deployment/placement/slo | ✅ |
| 控制台 `/governance`：矩阵 / 库存 / SLO / 时间线 | ✅ |
| 真机流量 burn / Prometheus 告警实触发 | ✅ 轻量（`phase0_slo_burn.sh` + abort 口径）/ ⏸ Alertmanager 实触发 |

### Product P3 Batch 2 勾选

| 项 | 状态 |
|----|------|
| `CompatibilityRule` + etcd `/compat/` + 默认种子（含 Ascend deny） | ✅ |
| `ModelDeployment.image_id` / `image_override_reason` / `compat_rule_ids` | ✅ |
| 启动前兼容校验；Scheduler 结构化 `platform_incompatible` 拒绝 | ✅ |
| `GET /inventory/hardware`；节点页展示 platform/name/driver/cuda | ✅ |
| GPU 历史利用率画像库 | ⏸ |

### Product P1 Batch 3 / P3 Batch 1 勾选

| 项 | 状态 |
|----|------|
| 运行时 capability 写入 etcd `/capabilities/`，BFF/模型详情展示 | ✅ |
| Adapter `EngineVersionSupport` 静态表 + `validate_engine_version` | ✅ |
| `NodeStatus.platform` + `GpuStatus` name/driver/cuda；`NEBULA_NODE_PLATFORM` | ✅ |
| Scheduler：`EngineImage.platforms` 参与候选过滤与可解释拒绝 | ✅ |
| 真实 SGLang Gateway / vLLM Router e2e | ✅ 通路径（Gateway→Router→SGLang/vLLM） |

### Product P2 — Serving Cell（已移除）

Serving Cell（`CellIngress` / etcd `/cells/` / BFF `/api/v2/cells` / Router 整入口选路）已从产品与代码路径下线，下列历史勾选不再作为当前能力宣传。

| 项 | 状态 |
|----|------|
| 历史 Batch 1–2 工程交付 | ❌ 已移除 |
| 真机 SGLang Gateway / vLLM Router e2e | ❌ 不再跟进 |

### Product P1 Batch 1 勾选

| 项 | 状态 |
|----|------|
| 未知 `engine_type` 显式报错（Node `create_engine` 不再回退 vLLM） | ✅ |
| `Engine` trait：`capabilities` / `validate_config` | ✅ |
| vLLM / SGLang 静态 `EngineCapability` 表（`SupportLevel`） | ✅ |
| BFF create/update/start/template 部署前校验 engine+config | ✅ |

### Product P1 Batch 2 勾选

| 项 | 状态 |
|----|------|
| Scheduler / common：按引擎方言生成 Placement `extra_args` | ✅ |
| 运行时 capability 探测（`/v1/models` + `/metrics`，失败不阻塞 Ready） | ✅ |
| 能力结果持久化到 etcd / 控制台展示 | ✅ |
| Adapter 版本支持范围矩阵与 e2e fixture | ✅（静态表 + 单测；真机 e2e ⏸） |

### Product P0 Batch 1 勾选

| 项 | 状态 |
|----|------|
| `EndpointStats.kv_cache_usage` 替换伪字节字段 | ✅ |
| scrape success/fail/timeout/parse 指标 | ✅ |
| vLLM / SGLang metrics fixture + 解析测试 | ✅ |
| `EngineCapability` / `ServingTopologyKind` 契约草案 | ✅ |
| BFF latency `data_source: router` 标明 | ✅ |
| UI 缺失 KV 显示 n/a（不用 0） | ✅ |

### Product P0 Batch 2 勾选

| 项 | 状态 |
|----|------|
| Trace inject/extract 往返单测（同 trace_id 跨 Gateway→Router→Engine hop） | ✅ |
| `normalize_otlp_endpoint`：`OBSERVE_URL` 基址自动补 `/api/public/otel` | ✅ |
| BFF reliability `kind=upstream_5xx` 对齐 Router 导出 | ✅ |
| BFF gateway/* 响应统一 `data_source: "router"` | ✅ |
| 删除 Router/Scheduler 死指标 `*_xtrace_*` | ✅ |
| Inference 页优先 Router 指标；auth 缺失显示 n/a | ✅ |
| `remote-degradation-check.sh` 改为校验活指标 / 死指标缺席 | ✅ |

---

## N4-Obs — 全链路可观测

设计源：[`../manual/module.md`](../manual/module.md)「监控与日志」。

### 目标架构（已落地）

```text
  Gateway / Router / Node / Scheduler
           │
           ├─ OTLP traces ──► xtrace (nebula-observe)
           ├─ MetricPoint batch ──► xtrace
           ├─ Prometheus Counter/Histogram ──► /metrics
           └─ JSON logs (stdout) ──► Promtail/Vector ──► Loki

  关联：request_id + W3C traceparent（Gateway → Router）
```

**禁止**：xtrace→VM/Loki 桥；应用直连 Loki；高基数 label（user_id/request_id）进 Prometheus。

### 执行清单

| ID | 项 | 状态 | 落点 |
|----|----|------|------|
| **O1** | 公共双写发射器 | ✅ | `nebula_common::DualWriteEmitter` |
| **O2** | Gateway 热路径双写 | ✅ | `/metrics` + xtrace `nebula_gateway_*` |
| **O3** | Router 热路径双写 | ✅ | latency/TTFT/outcome 双写 |
| **O4** | Node GPU/引擎 → xtrace | ✅ | heartbeat `push_metrics`；etcd `/stats/` 仍为控制面 |
| **O5** | OTLP + W3C 传播 | ✅ | `TraceContextPropagator`；Router inject |
| **O6** | JSON 日志（Loki 路径） | ✅ | `NEBULA_LOG_FORMAT=json` |
| **O7** | Loki 采集文档 + 示例 | ✅ | [`../manual/module.md`](../manual/module.md)、`deploy/observe/` |
| **O8** | SLO / 告警草案 | ✅ | [`../manual/module.md`](../manual/module.md)、`deploy/observe/prometheus-alerts.yml` |
| **O9** | 双写缺口审计 | ⏳ 持续 | Scheduler 等按需补；低基数 |
| **O10** | xtrace 深度需求 | ⏸ 上游 | 见 module 监控与日志 |

### 环境变量（全链路打开）

| 变量 | 作用 |
|------|------|
| `OBSERVE_URL` | xtrace 基址（metrics batch + 可选 OTLP） |
| `OBSERVE_TOKEN` | xtrace Bearer |
| `NEBULA_LOG_FORMAT=json` | stdout JSON → Loki 采集 |
| 组件 `/metrics` | 默认暴露；Prometheus/VM scrape |

### 验收

1. 开 `OBSERVE_URL` 后，推理请求在 xtrace 可见 `nebula_router_*` / `nebula_gateway_*` 点（或 traces）。  
2. 同请求在 `GET /metrics` 上计数/直方图增加。  
3. `NEBULA_LOG_FORMAT=json` 时日志可被 Promtail 解析，并按 `request_id` 检索。  
4. abort/drain 仍**不计入** 5xx 错误预算（Prometheus 独立 `*_aborted_total`）。

### 与旧 P1.5 / P2.5

| 旧 ID | 并入 |
|-------|------|
| P1.5 SLI/SLO | O8 + observability §5 |
| P2.5 前端运维 | 仍按需；面板只绑 BFF，不直连 xtrace |

---

## N1 — 接入面 HA（主体完成）

| 项 | 状态 |
|----|------|
| Scheduler 选主 + fencing 真机 | ✅ |
| Gateway / Router ×2 + LB 真机 | ✅ |
| 真机报告 | ✅ [`../dev/ha/report.md`](../dev/ha/report.md) |
| 生产 etcd 三节点 | ⏸ 切入暂缓；runbook ✅ [`../manual/etcd-ha.md`](../manual/etcd-ha.md) |

---

## N2 — 工程质量（已落地）

Q1–Q4 全部 ✅。可选：拆 `nebula-common`、前端拆包。

---

## N3 — 按需能力

| ID | 项 | 触发 |
|----|-----|------|
| D4 | 跨节点 TP | 单机 GPU 不够 |
| D5 | EngineShim | Passthrough 不够 |

---

## N4 — 产品化（观测以外）

- 硬件感知镜像映射、Console 大功能、模型预热 UI：单独产品排期。  
- 观测面板 UI 在 O1–O8 稳定后做 BFF 聚合增强。

---

## P1c 后续（前缀亲和生产化，2026-10-11 真机验收后规划）

> 依据与验收数据见 [`../dev/prefix-affinity.md`](../dev/prefix-affinity.md) §5–§6。排期单位为开发会话。

### Batch 1：P1c 生产化加固（无外部依赖）

| # | 任务 | 状态 | 验收标准 |
|---|------|------|----------|
| 1.1 | Chat 模板哈希（Router 侧渲染 chat_template + 复用哈希链，覆盖 `/v1/chat/completions`） | ✅ 2026-10-11 | 真机：6 个共享前缀 chat 请求全集中持有副本；A/B：p99 −60%（暖引擎） |
| 1.2 | 索引自愈 | ✅ 2026-10-11 | 实测 v0.23 命中（reuse）也发事件：重启后 3 请求内索引恢复、no_signal 仅 +1；`replay_endpoint` 转可选优化 |
| 1.3 | 亲和可观测（`prefix_index_hit_ratio` + 前缀热度 top-N：BFF API + 控制台卡片） | ⏳ | 控制台可回答亲和收益与前缀热度 |
| 1.4 | HA 扇出（多 Router 共享事件流；索引元数据不进 etcd 热路径） | ⏳ | 双 Router 实例一致命中 |

### Batch 2：全局 KV 索引服务（P1，依赖 1.4）✅ 2026-10-11

- `nebula-indexd`（nebula-router crate 内独立二进制）：引擎唯一稳定端点 → 权威索引 → 原始帧 + 周期快照推送到所有 Router sink；热路径零依赖，故障降级可见。本地验收 4/4（转发/自愈/降级/双 Router 一致）。细节见 prefix-affinity.md §5.8。

### Batch 3：引擎版本策略 + PD 编排（S2 前置）

1. ✅ 2026-10-11 兼容矩阵与选型（[`engines.md`](../dev/engines.md) §7）：**PD 传输主路径 LMCache**（双硬件代码原生、torch 层传输不绑 GPU 厂商）、备选 Mooncake(TCP)；排除 NIXL（Dynamo 系）与 p2p_nccl（NVIDIA 专属）；**引擎版本钉死 vLLM v0.23.x**（kv-events 已字节级校准）。待验证：昇腾真机 lmcache PD e2e、CUDA 侧最小 PD、角色化放置设计。
2. ✅ 2026-10-12 `pool.md` §pd_split 设计稿 **v1.1**（独立评审 REWORK 后修订）：v1 重新定位为"角色化完整请求路由"（单请求 PD 为 v2 开放设计）；补 Placement 配置传播生命周期、扩缩容 v1 禁自动、全局单 seed 契约、镜像 digest 产物绑定 + 门禁正数断言、多 owner 索引（v1.1）与四场景故障注入计划。
3. ⏳ 实现：按 pool.md 触点清单（common 结构体 → controller/node 注入 → Router stage 过滤 → BFF/UI），估 2-3 会话。

### 可信性收敛批次（assessment REWORK 后插队，2026-10-12；详见 [`../dev/assessment.md`](../dev/assessment.md)）

> 评估结论：架构主轴保留，优先把已有能力变可信。pd_split 第 3 步实现顺延至本批次之后。

| # | 任务 | 状态 |
|---|------|------|
| A1 | **嵌入式路由接线修复**：KV-event 运行时（ingest/sink/TTL）抽成 `start_kv_runtime` 共享函数，独立 Router 与 Gateway 嵌入式模式共用；端到端一致性测试（合成事件流 → 索引进索引） | ✅ 2026-10-12 |
| A2 | 控制台真实化：Endpoints 页固定值（协议数/健康度）改按实际状态计算；前缀亲和卡定时刷新；区分健康/降级/未知/过期 | ✅ 2026-10-12 |
| A3 | 配额语义边界：queue-admission.md 修正失实表述（轮询出队→等待者份额）+ 明确"进程级保护 ≠ 集群配额"交付边界（N 网关 = N× 上限）；新增租户份额语义单测（热租户占满份额被拒、他租户不受阻） | ✅ 2026-10-12 |
| A4 | 文档收敛：README→1.10.0、positioning→1.10.0、etcd.md 池条目改已实现、根 DESIGN.md 归档并指向 docs/design.md 唯一入口 | ✅ 2026-10-12 |

之后顺序（对齐评估建议 2/3）：交付收敛（默认部署清单/版本固定/升级回滚/HA 验收）→ 收益验证（亲和+共享 KV 多副本 A/B，采纳米"客户端 P99/有效吞吐/拒绝率/部署与恢复时间"口径）→ 再决定严格 PD。

### Batch 4：空闲容量吃批负载（独立，可插队）

- benchmark/canary 调度到空闲副本（`/stats/` 忙闲信号 + 准入队列低优先级）；Phase 3 Selection 联动前置。

### 顺手卫生项（夹带做）

- 数据面 retry 语义收敛（首字节前可重试，单一实现）；review skill 增加引擎 adapter 仅声明 capability 规则。

---

## 明确不做

- 改架构主轴；Actor 回潮  
- Gateway 绕过 Router / 双写资源  
- xtrace 当 Prometheus 替代或 xtrace→Loki 桥  
- 应用直连 Loki  
- 只有 metrics、没有 abort/drain SLO 口径  
- 引擎内 Stage 重构（S1，vLLM 上游的活）；自造 KV 传输/shuffle；Decode 中途迁 KV（等 S3 层落地再评估）  

---

## 文档地图

| 文档 | 用途 |
|------|------|
| [`vision.md`](./vision.md) | 产品总纲：定位、L0–L4、原则、Phase 路线 |
| [`architecture.md`](./architecture.md) | L0 架构现状 |
| 本文 | **排期真源**：Phase 映射 + 批次勾选 |
| [`selection.md`](./selection.md) | L3 分册 |
| [`pool.md`](./pool.md) | L2 池化设计（Phase 2） |
| [`../dev/contracts.md`](../dev/contracts.md) | L1 契约验收条目 |
| [`../manual/module.md`](../manual/module.md) | 功能模块（可观测 / SLO / Loki / HA） |
| [`../dev/ha/report.md`](../dev/ha/report.md) | HA 真机报告 |
| [`../dev/stats.md`](../dev/stats.md) | etcd `/stats/` 控制面契约 |
