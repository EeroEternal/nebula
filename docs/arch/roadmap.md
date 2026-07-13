# Nebula 下一步优化计划

> 更新：2026-07-13（**v1.3.0**）。已完成项摘要见 [`architecture.md`](./architecture.md)；产品阶段见 [`../dev/plan.md`](../dev/plan.md)；Release Notes 见 [`../versions/v1.3.0.md`](../versions/v1.3.0.md)。

## 当前进展（一句话）

M1 / N1 HA 主体 / N2 / O1–O8 / **产品对齐 P0–P6 Batch 1** 已随 **v1.3.0** 发布。真机 Gateway e2e、多引擎 benchmark、多租户压测暂缓。剩余：按需 N3–N4 / P7。生产 etcd 三节点暂缓。

## 优先级总表

| 批次 | ID | 主题 | 状态 | 说明 |
|------|----|------|------|------|
| **N2** | Q1–Q4 | 工程质量 | ✅ | BFF 去重、HTTP client、observe、CI |
| **N1** | D3 | 接入/调度 HA | ✅ 主体 / ⏸ etcd | 真机报告；生产 etcd 暂缓 |
| **N4-Obs** | O1–O10 | 可观测 | ✅ O1–O8 / ⏸ O9–O10 | SLO runbook + 告警样例已落；O9 按需 |
| **Product P0** | P0 | 契约与观测可信度 | ✅ Batch 1–2 | 见 [`../dev/plan.md`](../dev/plan.md) |
| **Product P1** | P1 | Engine Capability / Adapter | ✅ Batch 1–3 | 含能力 etcd 持久化、版本支持表、Cell 重试边界 |
| **Product P2** | P2 | Serving Cell 只读接入 | ✅ Batch 1–2 / ⏸ e2e | 真机 Gateway e2e 暂缓 |
| **Product P3** | P3 | 镜像平台 / 加速器台账 | ✅ Batch 1–2 | 兼容矩阵 + platforms 放置 + 库存 API/控制台；历史画像 ⏸ |
| **Product P4** | P4 | 统一 SLI / SLO | ✅ Batch 1 | ModelSlo CRUD/评估 + 诊断时间线；真机 burn ⏸ |
| **Product P5** | P5 | Benchmark / 推荐 / Canary | ✅ Batch 1 / ⏸ e2e | schema + scripts + BFF + 控制台；真机双引擎 ⏸ |
| **Product P6** | P6 | 多租户 / 配额 / 成本 | ✅ Batch 1 / ⏸ 压测 | Tenant + Gateway 准入 + 用量成本；真机压测 ⏸ |
| **N3** | D4/D5 | 按需能力 | ⏸ | 跨节点 TP；EngineShim |
| **N4 其余** | UX | 产品化 | ⏸ | 硬件镜像、Console 大功能 |

原则：不阻塞日常推理；生产 etcd 三节点仍暂缓。

---

## 下一阶段顺序

```
①–⑦ Product P0–P6 Batch 1          ✅ 已随 v1.3.0 发布
⑧ N3–N4 / P7（有需求再开）
⑨ 生产 etcd 三节点（暂缓）
⑩ 真机 e2e / 多引擎 benchmark / 多租户压测（环境就绪后）
```

### Product P6 Batch 1 勾选

| 项 | 状态 |
|----|------|
| Tenant / Quota / Usage / Pricing 契约 + etcd | ✅ |
| Auth `token:role[:tenant_id]`；关闭多租户兼容单 token | ✅ |
| Gateway 准入 + ExecutionContext 传播；稳定拒绝码 | ✅ |
| 低基数 `tenant_denied_total{reason=}`；审计含 tenant/deny | ✅ |
| BFF + 控制台租户用量/成本视图 | ✅ |
| 真机多租户隔离压测 | ⏸ |

### Product P5 Batch 1 勾选

| 项 | 状态 |
|----|------|
| `BenchmarkRun` / `PerformanceProfile` / recommend / canary 契约 | ✅ |
| `scripts/benchmark` workload + runner（dry-run / ingest） | ✅ |
| BFF `/benchmarks/*` + `/canaries/*`；不足数据不默引擎 | ✅ |
| 控制台 governance：runs / recommend / canary | ✅ |
| 真机双模型 × vLLM/SGLang 重复执行 | ⏸ |

### Product P4 Batch 1 勾选

| 项 | 状态 |
|----|------|
| `ModelSlo` 契约 + etcd `/slos/{model_uid}` CRUD | ✅ |
| `GET .../evaluate`：Router scrape 对比目标；低流量 `insufficient_data`（不假绿） | ✅ |
| abort/Drain 排除错误预算语义写入评估结果 | ✅ |
| Cell 违约建议仅 capacity/config，无 worker 写操作 | ✅ |
| `DiagnosticEvent` 聚合 deployment/placement/cell/slo | ✅ |
| 控制台 `/governance`：矩阵 / 库存 / SLO / 时间线 | ✅ |
| 真机流量 burn / Prometheus 告警实触发 | ⏸ |

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
| Cell Ingress：Router 不重试、不记 endpoint circuit（避免二次放大） | ✅ |
| `NodeStatus.platform` + `GpuStatus` name/driver/cuda；`NEBULA_NODE_PLATFORM` | ✅ |
| Scheduler：`EngineImage.platforms` 参与候选过滤与可解释拒绝 | ✅ |
| 真实 SGLang Gateway / vLLM Router e2e | ⏸ 等真机环境 |

### Product P2 Batch 2 勾选

| 项 | 状态 |
|----|------|
| BFF `GET/POST .../cells/.../observe`：健康探针 + Ingress `/metrics` allowlist 解析 | ✅ |
| 缺字段显示为 null/`n/a`；`data_source: cell_ingress`；不写 `/stats/` | ✅ |
| 刷新 `status`/`last_checked_ms` 到 etcd（Router 可感知 Unhealthy） | ✅ |
| 控制台 `/cells`：列表、注册、取消注册、观测面板；内部拓扑不可见 | ✅ |
| 真实 SGLang Gateway / vLLM Router e2e | ⏸ 等真机 |

### Product P2 Batch 1 勾选

| 项 | 状态 |
|----|------|
| etcd `/cells/{model_uid}/{cell_id}` + `CellIngress` 契约（无 worker 写字段） | ✅ |
| BFF `POST/GET/DELETE /api/v2/cells`；OpenAI `/v1/models` 探针；与 Nebula Running 互斥 | ✅ |
| DELETE 只删 etcd 声明，不停外部 Cell；响应 `internal_topology: not_visible` | ✅ |
| Router watch `/cells/`；Ready Cell 整入口选路，不选 Cell 内 worker | ✅ |
| Node 不 watch `/cells/`（外部生命周期由引擎 serving 栈管理） | ✅（按设计忽略） |
| Ingress `/metrics` 采集与控制台 Cell 视图 | ✅ Batch 2 |

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
| `EngineCapability` / `ServingTopology` / `CellIngress` 契约草案 | ✅ |
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

设计源：[`../manual/observability.md`](../manual/observability.md)、[`../manual/loki.md`](../manual/loki.md)。

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
| **O7** | Loki 采集文档 + 示例 | ✅ | [`../manual/loki.md`](../manual/loki.md)、`deploy/observe/` |
| **O8** | SLO / 告警草案 | ✅ | [`../manual/slo.md`](../manual/slo.md)、`deploy/observe/prometheus-alerts.yml` |
| **O9** | 双写缺口审计 | ⏳ 持续 | Scheduler 等按需补；低基数 |
| **O10** | xtrace 深度需求 | ⏸ 上游 | 见 observability §9 |

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
| 生产 etcd 三节点 | ⏸ **暂缓** |

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

## 明确不做

- 改架构主轴；Actor 回潮  
- Gateway 绕过 Router / 双写资源  
- xtrace 当 Prometheus 替代或 xtrace→Loki 桥  
- 应用直连 Loki  
- 只有 metrics、没有 abort/drain SLO 口径  

---

## 文档地图

| 文档 | 用途 |
|------|------|
| [`architecture.md`](./architecture.md) | 架构现状 |
| 本文 | **排期真源**（按需 O9 / N3–N4；Product P1） |
| [`../manual/observability.md`](../manual/observability.md) | 可观测设计权威（含上游 O10） |
| [`../manual/slo.md`](../manual/slo.md) | SLO / 告警 runbook（O8） |
| [`../manual/loki.md`](../manual/loki.md) | Loki 采集路径 |
| [`../manual/ha.md`](../manual/ha.md) / [`report.md`](../dev/ha/report.md) | HA 演练与真机报告 |
| [`../dev/stats.md`](../dev/stats.md) | etcd `/stats/` 控制面契约 |
