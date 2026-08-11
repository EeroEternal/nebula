# Phase 0 真机验证 — pro6000

> 日期：2026-08-11。主机 `pro6000`（8× NVIDIA RTX PRO 6000 Blackwell）。代码 `2abf1ad`（main）。  
> 原始日志在真机 `~/nebula/logs/phase0/`（不入库）。

## 拓扑

控制面：etcd（单节点）+ gateway / router / scheduler / node / bff。  
引擎（已有容器复用）：vLLM `qwen15_moe_vllm`（GPU 0,1 :10826）+ SGLang `qwen15_moe_sglang`（GPU 6,7 :10824），模型均为 Qwen1.5-MoE-A2.7B-Chat。

## 结果

| 场景 | 结果 |
|------|------|
| 拉最新 main + release 重建 + 控制面滚动重启 | PASS（引擎容器 `reusing existing`，未重建） |
| Gateway / Router / Scheduler healthz | PASS |
| 双引擎 chat e2e（Gateway→Router→引擎） | PASS |
| 双引擎 SSE stream | PASS |
| Gateway / Router `/metrics` | PASS |
| Benchmark live + BFF ingest（需 console token） | PASS（各 1 run，201） |
| Recommend（`model_name`=deployment uid） | PASS（confidence=low，样本=1） |
| `phase0_slo_burn` 离线可用性评估 | PASS（双引擎 `compliant`，5xx=0） |
| `phase0_tenant_isolation` | PASS（A 限流 429 / B 隔离 / ACL 403） |
| abort 契约（`test_cancel_sse` / 主动断流） | FAIL（`nebula_router_requests_aborted_total` 未 +1） |
| BFF `slos/*/evaluate` 的 ttft/latency | 部分（availability 有；ttft_p95/latency_p95 缺，尽管 Router 已暴露 histogram） |
| 生产 etcd 三节点切入 | ⏸ |
| Prometheus 告警实触发 | ⏸ |

## 结论

pro6000 上 **Gateway→Router→双引擎** 主路径与 **多租户隔离** 已通过；控制面可滚动升级且复用引擎。遗留：abort 计数未涨、BFF evaluate 未吃到 TTFT histogram、生产 etcd HA 与告警实触发仍待维护窗。
