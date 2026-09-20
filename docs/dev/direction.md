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

## 7. 性能与产品重点（纯产品 / 性能视角）

不分竞争，只看「让产品更好用」与「让数字更好看」。分两层，不混。

### 7.1 层次 1：把 Nebula 自己的开销降到 0（确定、可度量）

| 优化 | 现状 / 依据 | 预期 |
|------|-------------|------|
| **嵌入式 Router（单跳）** | 实测两跳让 Nebula 多花 **+19% TTFT P99**（§2） | 直接收回那 ~38ms |
| **请求体零拷贝流式转发** | gateway/router 现在 `to_bytes` 整包缓冲 | 长 prompt TTFT↓、内存峰值↓ |
| **响应流透传** | OpenAI 路径无需逐帧解析 SSE | 省 CPU，同机并发↑ |
| header / trace 不重复序列化 | 两跳各一次 | 小但确定 |
| 上游连接池 / keep-alive | — | 小 |

目标：**经 Nebula ≈ 直连**（让自己从数据路径上消失）。

### 7.2 层次 2：把编排带来的增益做出来（Nebula 存在的理由）

| 优化 | 依据 | 预期 |
|------|------|------|
| **并发准入（自适应，钳在甜点区）** | 实测 C=8→256，P99 TTFT 81ms→**2464ms**，甜点 C≈8–16（§1） | P99 从秒级回落 |
| **多副本 + 跨副本前缀亲和** | 单实例天花板 ~3.4k tok/s | 聚合吞吐线性↑ |
| **响应缓存 / 去重 / 合并** | vLLM 不做（无状态引擎） | 高重复 workload 数量级收益 |
| **吃 vLLM KV events** | 上游已发信号，消费方缺位 | 让前缀亲和有据可依 |
| 优先级 / 公平调度 | 现只有静态租户配额 | 关键租户 TTFT 可保 |

目标：**做单实例做不到的事**（跨副本 / 跨请求 / 跨客户端）。

### 7.3 产品侧要补的能力

KV 全局索引 + 分层 offload 编排、自适应准入 + QoS、SLO 闭环扩缩、Batch / async API、精确·语义缓存、多引擎对齐（SGLang）、睡眠 / scale-to-zero、协议保真（tool_calls / Responses / 多模态）。

### 7.4 性能优先级（impact / effort / 证据）

| 优先 | 动作 | 类型 | 证据 |
|------|------|------|------|
| **P0** | 单跳（嵌入式 Router） | 性能·自研 | 实测 +19% P99 |
| **P0** | 自适应并发准入（甜点区） | 性能+产品 | 实测 C=256 P99 2.5s |
| **P1** | 多副本 + 前缀亲和 + 吃 KV events | 性能+能力 | 单实例天花板 3.4k tok/s |
| **P1** | 请求体零拷贝 + 响应透传 | 性能 | 长 prompt / 流式 |
| **P2** | KV 全局索引 + 分层 offload 编排 | 能力 | 长上下文 / 多轮 |
| **P2** | 响应缓存 / 去重 / 合并 | 性能+产品 | 高重复 workload |
| **P3** | SLO 闭环扩缩、Batch API、QoS、scale-to-zero | 能力 | 运维 / 成本 |

**一句话**：先让自己「变没」（单跳 + 零拷贝 + 透传），再把「多副本 / 多请求 / 多租户」的编排增益做出来（准入 + 前缀亲和 + 缓存去重）。

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
