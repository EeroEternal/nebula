# Nebula 发展方向备忘

> 日期：2026-09-20。性质：**战略备忘**（结论性判断，非实现计划——批次以 [`../arch/roadmap.md`](../arch/roadmap.md) 为准）。
> 依据：[`landscape.md`](landscape.md)（Dynamo / llm-d 对照）、[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md)（真机测量 + A/B/C 分区）、[`../arch/control-plane-adapters.md`](../arch/control-plane-adapters.md)（适配器边界）。

## 1. 定位

> **Nebula = 不依赖 K8s 的多引擎推理编排 + 多租户治理控制面**（引擎之上、K8s 之外）。

「引擎之上编排」这一层（[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §10 的 Zone C）已被 Dynamo（NVIDIA，Rust）与 llm-d（CNCF，K8s）占据，**正面拼广度必输**。Nebula 唯一未被占据的位置是**轻量、无需 K8s、引擎无关、带企业治理**；llm-d 的 `non-kubernetes-mode` 提案说明该需求真实，但窗口有限。

## 2. 三条主线

### 主线 1（守）：把「不依赖 K8s」做成产品级护城河

Nebula 是**单二进制 + etcd 权威 + 快速起栈**；Dynamo / llm-d 都要求 K8s。

- 落地：一键起栈、etcd HA / fencing、生产 etcd 切入、无 K8s 运维 runbook。
- 衡量：**起栈门槛**（依赖数、时间）、无 K8s 环境的可运维性。
- 对应现有 roadmap：Phase 0 收尾 + Phase 2/3 的运维面。

### 主线 2（攻）：补齐「引擎之上」的核心能力

这是**能形成壁垒**的一块，现有 roadmap **缺失**（Phase 2 是 HardwarePool，Phase 3 是 SLO 闭环，都没有 KV 感知路由）。

| 能力 | 依据 | 价值 |
|------|------|------|
| 吃 **vLLM KV events** | vLLM 已能发（`EventPublisher.register_publisher`） | 信号源就绪，消费方缺位 |
| **KV 全局索引 + 分层 offload 编排** | Dynamo `kvbm-*`、llm-d tiered cache | 多轮 / 长 system prompt 的头号能力；**最大缺口** |
| **跨副本前缀亲和** | 实测单实例天花板 ~3.4k tok/s（[`hw/vllm-serving-perf.md`](hw/vllm-serving-perf.md) §1） | **唯一能真超过单实例直连的系统级手段** |
| 选路升级 **Filter→Score→Pick + 插件** | llm-d EPP、Dynamo `router-plugins` | 单阶段 `select()` → 可插拔管线 |

### 主线 3（拉大）：深化企业治理

Dynamo / llm-d 偏基础设施；**多租户 / SLO / 成本 / 审计 / canary** 是 Nebula 已建成（P4/P5/P6 Batch 1）、对手相对弱的企业平台能力。

- 补：**SLO 闭环自动扩缩**（现只到建议）、**成本与选型联动**、**Batch / async API**（对手都有）。

## 3. 优先级（结合真机数据）

| 优先 | 动作 | 依据 | 类型 |
|------|------|------|------|
| **P0** | **嵌入式 Router（单跳）** | 实测 Nebula 多花 **+19% TTFT P99**（§2） | 纯自研，明确回收 |
| **P0** | **并发准入钳在 C≈8–16** | 实测 C=256 时 P99 2.5s（§1） | 配置即可 |
| **P1** | **KV events 消费 + 跨副本前缀亲和** | 唯一能超过单实例直连的系统级手段 | 新能力（主线 2） |
| **P2** | **KV 全局索引 + 分层 offload 编排** | 对 Dynamo KVBM 的最大缺口 | 新能力 |
| **P2** | 选路 Filter→Score→Pick + 插件 | 对齐 EPP 语义 | Router 重构 |
| **P3** | SLO 闭环扩缩、Batch API、GAIE 互操作 | 对手已有 | 产品补齐 |

## 4. 明确不做（已排除）

| 不做 | 原因 |
|------|------|
| 引擎内核 / 前端 | `vllm-rs` 已做，且上游免费维护（§4） |
| KV 传输实现 | 边界正确——用 NIXL 的信号，不自造 |
| fork vLLM 插件碰内核 | Zone A/B：耦合 + 随模型升级变（§11） |
| 单机 PD 分离 | 无 NVLink，KV 传输是纯增项（§6） |
| ZMQ 调参 / CPU 绑核 | 实测无收益（§3/§6） |

## 5. 风险与时间窗

1. **窗口有限**：llm-d `non-kubernetes-mode` 一旦落地，「无 K8s」护城河被正面侵入。
2. **资源不对等**：Dynamo 是 NVIDIA + Rust，正面拼广度必输。
3. **对策：窄而深**——把「无 K8s + 治理 + KV 感知」三个点做透，而不是「vLLM 有什么我都要有」。

## 6. 衡量指标（换 KPI）

不要再用**单请求 TTFT** 作主 KPI（由引擎决定，Nebula 最多打平）。改看：

1. **多副本 aggregate goodput / 缓存命中率**（前缀亲和收益）；
2. **P99 尾延迟**（单跳 + 准入）；
3. **部署门槛**（起栈时间、依赖数、无 K8s 可行性）；
4. **多租户隔离与可问责**（治理）。
