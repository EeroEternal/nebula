# Nebula 可观测性

> 状态：统一设计与实施规范（2026-07-12）  
> **执行勾选：** [`../arch/optimization.md`](../arch/optimization.md) N4-Obs（O1–O7 ✅；**O8** SLO/告警待补）。  
> Loki 采集：[`loki.md`](./loki.md)。


本文合并原 `observability_external.md`、`observability_contract.md`、`memory/observability.md`、`gateway/gateway_observability.md`。

---

## 0. 结论先行

**xtrace 足够作为产品内 LLM 可观测权威，不足以单独作为对外运维监控全家桶。**

| 平面 | 权威系统 | 谁用 |
|------|----------|------|
| LLM 语义（trace / generation / 用量 / 真分位） | **xtrace**（`nebula-observe` / BFF） | Nebula 控制台；合同约定时可只读 API |
| 运维时序 | **Prometheus `/metrics` 双写** | 客户 VictoriaMetrics / Prometheus / Grafana |
| 日志 | **JSON stdout** → 采集器 | Loki / ELK；**应用不直连 Loki** |
| 关联 | `request_id` / `trace_id` | 三平面互跳 |
| 产品 UI | Nebula 前端 + BFF | xtrace 不是最终产品 UI |

**禁止**：xtrace → VM/Loki 官方桥；trace/generation 灌进时序库；把 xtrace UI 当唯一运维入口；P2.5 前端绕过 BFF 直连 xtrace。

---

## 1. 目标架构

```text
  Gateway / Router / Scheduler / Node
           │
           ├─ OTLP traces / 兼容 batch ──► xtrace (nebula-observe)
           ├─ LLM 业务 raw points ──► xtrace
           ├─ Prometheus Histogram/Counter ──► /metrics ◄── 客户 VM scrape
           └─ JSON logs (stdout) ──► Promtail/Vector ──► Loki / ELK

  Nebula 前端 ◄── BFF ◄── xtrace 查询 + 控制面状态
  客户 Grafana ◄── VM / Prometheus（不经 xtrace）
```

客户常见三类诉求与边界：

1. **产品内排障** → xtrace + Nebula UI
2. **已有 Grafana/VM** → Prometheus 面双写，不是 xtrace 转发
3. **集中日志** → JSON stdout + 外部采集，不是 xtrace 职责

---

## 2. 责任边界

| 角色 | 职责 |
|------|------|
| **xtrace** | LLM 语义权威：Trace/Event、业务 raw points、真分位、审计/generation |
| **Prometheus `/metrics`** | 运维导出面；与 xtrace **同采集点双写**，分存储 |
| **日志管道** | 应用只出带关联字段的 JSON stdout；Loki 由采集器接入 |
| **Nebula 策略** | Context 透传、TTFT/聚合策略、abort/drain 是否计入错误预算、`nebula.*` 命名、产品内诊断 UI（经 BFF） |

原则：领域逻辑归 Nebula；不把引擎特性注入 xtrace 核心；写入评估对推理延迟的影响；无非法桥接。

---

## 3. 现状与已集成能力

> 纠正旧表述：并非「已从 Prometheus 完全转向 xtrace」，而是混合架构。

已落地：

1. **nebula-observe** — 内嵌 xtrace
2. **nebula-node** — xtrace-client 上报 GPU / KV cache / pending 等
3. **OTLP tracing** — gateway/node/router 经 `nebula-common::telemetry`；**W3C TraceContext** 全局 propagator
4. **`DualWriteEmitter`** — Gateway/Router 热路径同点写 Prometheus 原子 + xtrace `push_metrics`
5. 组件 `--xtrace-url` / `--xtrace-token`（`OBSERVE_URL` / `OBSERVE_TOKEN`）
6. Prometheus `/metrics` 导出面（Router 含 latency/TTFT histogram）
7. **JSON 日志** — `NEBULA_LOG_FORMAT=json` → stdout → Promtail/Vector → Loki（见 [`loki.md`](./loki.md)）

xtrace：Langfuse 兼容 + OTLP、`/v1/metrics/batch`、查询 API。  
etcd 只存控制面 stats；历史观测不进 etcd。

---

## 4. 实施规范

### 4.1 领域信号（`nebula.*`）

- 调度：`nebula.scheduling.decision`, `nebula.scheduling.placement_version`
- 节点：`nebula.node.reconcile_status`, `nebula.node.engine_swap`
- 引擎：`nebula.engine.kv_cache_utilization`, `nebula.engine.gpu_memory_usage`

Prometheus 导出可用 `nebula_*`；与 xtrace 点名并存时交付文档须对照单位与分位算法（真分位 vs `histogram_quantile`）。

### 4.2 推理性能

- xtrace：TTFT、端到端时延、token、吞吐、`request.count(outcome)`
- 同一 emit 点双写 Prometheus Histogram/Counter（需对外 scrape 的信号）
- abort / 主动 drain **默认不计入**错误预算
- 低基数 label：`model` / `node` / `engine`；禁止 `user_id` / `request_id` 进 scrape labels

### 4.3 上下文传播

`Gateway`（注入）→ `Router`（透传）→ `Node` → Engine；`request_id` 与 `trace_id` 同时进日志与 trace metadata。

### 4.4 日志（Loki 等）

- 最小字段：`ts`, `level`, `service`, `message`, `request_id`, `trace_id`, `model_uid?`, `node_id?`
- K8s 推荐 JSON + stdout；可选 OTel Logs，不替代 stdout→采集器主路径
- 交付：采集示例 + 按 `trace_id` 的 LogQL 示例

### 4.5 对外话术

| 场景 | 打开什么 | 说法 |
|------|----------|------|
| 只要控制台 | xtrace + BFF + 前端 | 监控与审计在控制台完成 |
| 只有 VM/Grafana | `/metrics` + scrape 文档 | 曲线来自 Prometheus 导出，与 xtrace 同采集、分存储 |
| 要 Loki | JSON stdout + 采集手册 | 日志进贵方 Loki，可用 trace_id 互跳 |
| 完整包 | 三者都开 | 语义在 xtrace；时序在贵方 TSDB；日志在贵方 Loki |

---

## 5. Gateway / Router 产品内面板

数据经 BFF 聚合；前端负责卡片/趋势/告警态。目标：识别 5xx/过载/熔断、防护是否有效、调参前后对比。

### 5.1 最小指标（MVP）

**Router**：`nebula_router_requests_total`、`responses_{2xx,4xx,5xx}`、`retry_total` / `retry_success_total`、`upstream_error_total{kind}`、`request_too_large_total`、`route_circuit_skipped_total`、`circuit_open_total`、`nebula_route_latency_seconds`、`nebula_route_ttft_seconds`。

**Gateway**：`nebula_gateway_requests_total`、`responses_{2xx,4xx,5xx}`、`request_too_large_total`、`upstream_error_total{kind}`、abort 相关计数（与可靠性口径一致）。

### 5.2 布局

- 顶部卡片：RPS、5xx 比例、重试成功率、熔断开启次数
- 中部趋势：状态码、重试、上游错误分类、延迟/TTFT 分位
- 底部诊断：熔断/候选缩减、413、高错误时间窗

衍生：`5xx/requests`、`retry_success/retry`、`circuit_skipped/requests` 等由前端或 BFF 计算。

### 5.3 初始告警阈值（需按流量校准）

- 5xx 比例 > 2% 持续 5 分钟
- 重试成功率 < 20% 持续 10 分钟
- `circuit_open_total` 5 分钟突增（相对 1h 均值 3 倍）
- timeout 类错误占比 > 40% 持续 10 分钟

面板验收：支持 5m/15m/1h；可下钻标签；故障演练能回答错误起点、重试是否生效、熔断是否降低坏副本命中。

后续：按 `model_uid` 维度；参数变更事件叠加趋势图。

---

## 6. 与 P1.5 / P2.5

| ID | 焦点 | 落点 |
|----|------|------|
| **P1.5** | SLI/SLO、abort/drain 口径、告警 | xtrace 查询 + PromQL 草案分述 |
| **P2.5** | 前端运维流 | 总览 / 告警中心 / 观测面板 / 审计 / 薄多集群；客户 VM/Loki 只出文档与出口 |

P2.5 不做：自建第二套时序 UI、把客户 Grafana/Loki 搬进控制台。

---

## 7. 落地顺序

1. 本文为设计源；执行勾选见 **optimization N4-Obs**。
2. ~~双写脚手架 + Gateway/Router 热路径~~ ✅（`DualWriteEmitter`）
3. ~~JSON 日志 + Loki 采集文档~~ ✅（[`loki.md`](./loki.md)）
4. **下一步 O8**：SLO + 告警阈值写入 runbook（§5.3 初值）。
5. O9：Scheduler 等业务点双写缺口按需审计。
6. 前端运维面板（P2.5）：只绑 BFF；客户 VM/Loki 走文档出口。

---

## 8. 反模式清单

- 承诺 xtrace 兼容 PromQL，或用 xtrace 替代 scrape 生态
- xtrace → VM/Loki 默认同步器
- generation/prompt 写入客户 TSDB
- 应用直连 Loki HTTP
- 不声明真分位与 `histogram_quantile` 的口径差
- 前端绕过 BFF 调 xtrace

---

## 9. xtrace 上游扩展（O10，不阻塞）

以下依赖 xtrace 产品侧能力，Nebula 侧不为此阻塞 O1–O8：

1. **流式 Span / Event**：SSE 过程中多次上报 TTFT/吞吐事件。  
2. **跨语言拼接**：Python 引擎侧 Span 挂到同一 TraceID。  
3. **语义字段可视化**：如 `scheduling.decision`、`node.reconcile`。

控制面已具备：W3C `traceparent`、OTLP export、热路径 MetricPoint 双写。

---

## 10. 一句话

Nebula 可观测 = **xtrace（LLM 语义）+ Prometheus 面（客户运维）+ 结构化日志（Loki 等）**，用关联 ID 串起来；产品 UI 做运维体验，客户栈走标准出口。
