# Nebula 极速数据面优化方案：嵌入式路由、零拷贝流式代理与引擎感知分流

> 归属：`docs/arch/` · 阶段：数据面性能强化 · 对照基准：vLLM 直连吞吐与延迟  
> 适用组件：`crates/nebula-gateway` · `crates/nebula-router`  
> **正交文档：** 控制面可插拔（发现 / 鉴权 / 粗策略）见 [`control-plane-adapters.md`](./control-plane-adapters.md)。本文管热路径怎么快；那篇管控制面从哪来。不要把 KV 打分或平台 `if` 写进 Gateway 热路径。

---

## 1. 背景与问题定义

Nebula 采用 Rust + etcd 构建，架构上天然实现了「控制面与数据面解耦」：
- **控制面**：通过 etcd watch、PlacementPlan CAS 与 NodeActor 进行声明式生命周期管理；
- **数据面**：以无锁 DashMap 内存选路结合 HTTP 反向代理（`Engine-Passthrough`）直达后端推理引擎（vLLM / SGLang）。

在常规请求下，Nebula 已经规避了传统 Python Actor 控制面的跨进程调度阻塞。但在对比**直连 vLLM 裸引擎**的极限压测场景（如 8K/200K 长上下文、30~70 高并发压测）下，当前代码实现仍存在进一步榨干性能、追求与裸引擎零差距（<1%）的优化空间。

---

## 2. 核心性能瓶颈剖析

### 2.1 瓶颈 1：Gateway → Router 的内部双跳 HTTP 转发
- **现状**：
  在 `crates/nebula-gateway/src/handlers.rs` 中，外部请求先进入 Gateway（8081），校验通过后由 `st.http.post(&url).send()` 转发至 `nebula-router`（18081），Router 选路后再发送给后端 vLLM 容器。
- **代价**：
  - 数据链路多了一次本地 TCP / Loopback 网络栈流转；
  - 带来了双重连接池维护与请求头部、TraceContext 的二次序列化；
  - 在高并发长输入（如 200K Prompt 请求体约 1.5MB）场景下，同一个 Payload 在内存中被收发、复制了两次。

### 2.2 瓶颈 2：超大 Payload 的内存完全缓冲（`to_bytes`）
- **现状**：
  - Gateway 在接收请求时执行 `axum::body::to_bytes(req.into_body(), max_size)` 完整等待整包到达并分配连续内存；
  - Router 接收请求后再次调用 `axum::body::to_bytes`；
  - 默认上限 `NEBULA_ROUTER_MAX_REQUEST_BODY_BYTES` 为 4MB。
- **代价**：
  - 70 并发 × 2MB Payload 瞬间要求几十上百兆的堆内存瞬时分配，并伴随系统调用内存拷贝；
  - 人为增加了「等待完整 Prompt 传输完成」的时延，无法让底层引擎第一时间收到字节流进行并行预解析。

### 2.3 瓶颈 3：非 OpenAI 兼容协议流式转换中的逐帧解析
- **现状**：
  - 在 `create_responses` / `create_anthropic_messages` 处理流式模式时，网关通过 `buf.push_str(...)` 逐行扫描 `\n`，调用 `parse_openai_sse_chunk` 反序列化 JSON，再重新组装为 Anthropic/Responses 格式的 SSE。
- **代价**：
  - 高频 Token 吐出时，网关侧密集的字符串分配与 JSON 解析拉高了 CPU 利用率，恶化了 ITL（Inter-Token Latency）。

---

## 3. 具体优化建议与技术架构

### 优化 1：架构扁平化 —— 嵌入式共享内存路由器（In-Process Router）

#### 方案设计
将 `nebula-router` 的核心路由选路能力（`Router`）作为库直接链接到 `nebula-gateway`：

```text
【当前双跳架构】
Client ------> [ Gateway ] (HTTP) ------> [ Router ] (HTTP) ------> [ vLLM Engine ]

【优化后单跳架构】
Client ------> [ Gateway (内嵌 In-Process Router) ] (HTTP) --------> [ vLLM Engine ]
```

- **实施细节**：
  1. `nebula-gateway` 直接在进程内初始化 `Arc<Router>`，并在后台启动与 etcd 同步的 `endpoints_sync_loop` / `stats_sync_loop`；
  2. 请求到达 Gateway 完成鉴权与门控后，直接调用 `router.route(&ctx, &model_uid)` 进行内存原子选路（微秒级），直接获得选中的 `EndpointInfo`；
  3. Gateway 复用连接池直接 `reqwest` 打向目标引擎 `endpoint.url`。
- **收益**：直接砍掉 1 次内部 HTTP RTT，消除全部内部网络转发损耗。

---

### 优化 2：流式 Body 透传与零拷贝（True Streaming Proxy）

#### 方案设计
对于标准 OpenAI 兼容推理接口（`/v1/chat/completions`、`/v1/completions`）：

1. **轻量模型嗅探（Model Peeking）**：
   - 优先从 Header 读取模型名（如 Gateway 已支持的 `X-Nebula-Model-Uid`）；
   - 若客户端未传 Header，仅流式读取请求体的首个 Chunk（前 512 字节~1KB）快速匹配 `"model": "..."` 字段，提取后立即组装回流中，**不等待后续完整的巨型 messages 文本**；
2. **Body 直传 Engine**：
   - 将 Axum 底层的 `req.into_body().into_data_stream()` 直接作为 Reqwest 的 `reqwest::Body::wrap_stream()`，以 Chunked 方式流式推送至 vLLM。
- **收益**：在 200K 上下文下，内存占用由 `O(payload_size * concurrency)` 降为 `O(chunk_size * concurrency)`，首包抵达引擎时间提前数毫秒至数十毫秒。

---

### 优化 3：基于引擎真实负载感知的智能分流（Engine-Aware Balancing）

#### 方案设计
目前 Nebula 采用 `LeastPending`（结合 `kv_cache_usage > 0.95` 熔断）。在多副本、混合输入长短的场景下，演进为两阶段感知选路：

1. **动态加权评分算法**：
   $$Score = \alpha \cdot \text{ActiveRequests} + \beta \cdot \text{KVCacheUsage} + \gamma \cdot \text{WaitingReqs}$$
2. **结合 Prefix Cache 亲和性**：
   - 当请求包含较长相同 System Prompt 时，通过哈希指纹（Prompt Hash Affinity）优先将请求调度至已有对应 KV Cache 的副本，大幅降低 Prefill 计算量。

---

## 4. 实施规划（Roadmap）

| 阶段 | 目标 | 涉及模块 | 预期成果 |
| :--- | :--- | :--- | :--- |
| **Phase 1** | **嵌入式 Router 合并（单跳直通）** | `nebula-gateway`<br>`nebula-router` | 消除网关与路由间的内部 HTTP，单请求基础延迟降低 2~5ms |
| **Phase 2** | **零拷贝流式 Body 转发** | `nebula-gateway/src/handlers.rs` | 200K 超长输入内存峰值降低 90%，消除大包排队等待 |
| **Phase 3** | **KV 亲和与加权分流增强** | `nebula-router/src/strategy/` | 多副本混合高并发场景下 P99 尾延迟降低 30% |

---

## 5. 总结

Nebula 凭借 Rust 的高性能异步运行时与 etcd 声明式模型，已经在控制面和数据面奠定了远优于传统 Python Actor 的技术优势。通过本次**架构折叠（嵌入式 Router）**与**零拷贝流式转发**，Nebula 将能够彻底抹平与裸 vLLM 引擎的性能差距，达到极近裸金属的吞吐与延迟。
