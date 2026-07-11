# xtrace 可观测性体系扩展需求

## 概述
为了满足 Nebula 作为高性能分布式推理控制面的可观测性需求，`xtrace` 体系需要从“被动接收方”向“深度上下文集成者”演进。Nebula 不仅仅需要简单的日志追踪，更需要能够与网关层（Gateway）和协议解析层（unigateway）高度协同的信号 Contract。

## 核心扩展需求

### 1. 分布式请求上下文传播协议 (Context Propagation)
Nebula 需要在处理请求时，将 TraceID 和 SpanContext 从网关侧透传至所有后端节点。
*   **需求**：`xtrace` 需定义标准化的 HTTP Header 和 RPC Metadata 契约，确保 Trace 上下文在 `nebula-gateway` -> `nebula-router` -> `nebula-node` 之间无损传播。
*   **实现建议**：在 `xtrace-client` 中提供基于 `tower` 的 middleware，自动注入并提取标准化的 Trace 信号。

### 2. 引擎执行面深度埋点 (Engine Deep Instrumentation)
Nebula 的执行逻辑最终下沉至 `nebula-node` 调用 Python 引擎（如 vLLM/SGLang）。
*   **需求**：`xtrace` 需要支持跨语言的 Trace 拼接。当 Python 引擎执行具体的 Kernel 时，能够将其性能指标（如 Time-to-First-Token, Generation-Throughput）与 Rust 控制面的 Span 挂载在同一个 TraceID 下。
*   **实现建议**：在 `xtrace` 中扩展对 Python Shim 层的 SDK 支持，确保性能采样数据能通过 Unix Domain Socket 或高效队列异步回传至 `xtrace` 后端。

### 3. 高性能异步流式响应追踪 (Stream-Friendly Tracing)
LLM 推理的核心是流式响应（SSE）。目前的链路追踪大多是基于“开始-结束”的闭环，无法反映流式过程中的波动。
*   **需求**：支持“流式 Span”。在流式输出过程中，能够记录每个 Token 的生成耗时，以及流式响应中途出现的异常。
*   **实现建议**：增加“持续性 Span”数据类型，允许在同一个 Span 下多次上报 Event 事件，以可视化展示 TTFT（首字延迟）和总响应延迟。

### 4. 语义化性能诊断契约 (Semantic Diagnosis Contract)
Nebula 的故障往往涉及复杂的放置策略（Placement）和路由（Routing）冲突。
*   **需求**：`xtrace` 需要能够解析并可视化 Nebula 特有的信号，如：
    *   `scheduling.decision`: 决策器为何将请求路由至此节点。
    *   `node.reconcile`: 节点侧配置对齐时的状态变化。
    *   `engine.swap`: 推理引擎进程的加载/卸载状态。

## 目标效果
通过以上扩展，Nebula 的运营人员可以通过 `xtrace` 仪表盘，清晰看到：
1.  **端到端延迟分布**：网关等待时间、队列排队时间、引擎计算时间、网络传输时间。
2.  **根因追踪**：若请求超时，能立即定位是 `nebula-scheduler` 调度慢，还是 `nebula-node` 引擎拉起慢。
3.  **流式性能波动分析**：可视化每个请求在生成过程中的 Token 吞吐波动。
