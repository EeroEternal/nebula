# Nebula 下一步优化计划

> 更新：2026-07-12。已完成项摘要见 [`architecture.md`](./architecture.md)；可观测设计见 [`../dev/observability.md`](../dev/observability.md)。

## 当前进展（一句话）

M1 / N1 HA 主体 / N2 已完成。可观测双写与 Loki 路径（O1–O7）已落地；**剩余工程勾选：O8 SLO/告警草案**，以及按需 N3/N4。生产 etcd 三节点暂缓。

## 优先级总表

| 批次 | ID | 主题 | 状态 | 说明 |
|------|----|------|------|------|
| **N2** | Q1–Q4 | 工程质量 | ✅ | BFF 去重、HTTP client、observe、CI |
| **N1** | D3 | 接入/调度 HA | ✅ 主体 / ⏸ etcd | 真机报告；生产 etcd 暂缓 |
| **N4-Obs** | O1–O10 | 可观测 | ✅ O1–O7 / ⏳ O8 | 双写+Loki 已通；SLO runbook 待补 |
| **N3** | D4/D5 | 按需能力 | ⏸ | 跨节点 TP；EngineShim |
| **N4 其余** | UX | 产品化 | ⏸ | 硬件镜像、Console 大功能 |

原则：不阻塞日常推理；生产 etcd 三节点仍暂缓。

---

## 下一阶段顺序

```
① O8（收尾）           SLO / 告警阈值写入 runbook
② N3 / 产品 N4（有需求再开）
③ 生产 etcd 三节点（暂缓）
```

---

## N4-Obs — 全链路可观测

设计源：[`../dev/observability.md`](../dev/observability.md)、[`../dev/xtrace_requirements.md`](../dev/xtrace_requirements.md)、[`../dev/loki.md`](../dev/loki.md)。

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
| **O7** | Loki 采集文档 + 示例 | ✅ | [`../dev/loki.md`](../dev/loki.md)、`deploy/observe/` |
| **O8** | SLO / 告警草案 | ⏳ **当前收尾** | observability §5.3 → runbook |
| **O9** | 双写缺口审计 | ⏳ 持续 | Scheduler 等按需补；低基数 |
| **O10** | xtrace 深度需求 | ⏸ 上游 | 见 `xtrace_requirements.md` |

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
| 真机报告 | ✅ [`../dev/ha/report-20260711.md`](../dev/ha/report-20260711.md) |
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
| 本文 | **排期真源**（剩余 O8 / 按需 N3–N4） |
| [`../dev/observability.md`](../dev/observability.md) | 可观测设计权威 |
| [`../dev/xtrace_requirements.md`](../dev/xtrace_requirements.md) | xtrace 上游扩展需求 |
| [`../dev/loki.md`](../dev/loki.md) | Loki 采集路径 |
| [`../dev/ha/`](../dev/ha/) | HA 报告与 runbook |
| [`../dev/details/stats.md`](../dev/details/stats.md) | etcd `/stats/` 控制面契约 |
