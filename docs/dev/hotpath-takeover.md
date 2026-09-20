# Nebula 网关层接管推理热路径与通用契约架构设计

> **本文档定位：** 详细记录将 Nebula 高性能网关层（UniGateway + Router）前置作为统一推理热路径时，与 PowerLLM 等分布式推理系统的分工架构、收益对比、双模兼容（有/无 Nebula）及代码改动方案。

---

## 1. 核心定位与分工澄清

将 Nebula 前置作为热路径网关，**绝非**绕过 PowerLLM 直接连接底层 vLLM/SGLang 引擎。

- **PowerLLM 定位**：分布式模型运行时与计算调度主体。负责模型实例的生命周期、GPU 拓扑划分（TP/PP）、显存复用、In-Flight 排队背压、会话状态机（StatusGuard）与故障自愈。
- **Nebula 定位**：独立于后端推理实现的通用高性能接口与流量中枢（UniGateway + Router）。负责客户端连接终结、多协议适配、全局流控与跨集群/跨推理框架的零开销选路。

```
                              ┌────────────────────────────────────────┐
                              │            客户端 HTTP 请求             │
                              │ (OpenAI / Claude / Responses 规范)     │
                              └───────────────────┬────────────────────┘
                                                  │
                      ┌───────────────────────────┴───────────────────────────┐
                      │                                                       │
           [形态 A：有 Nebula 前置网关]                             [形态 B：无 Nebula (独立运行)]
                      │                                                       │
                      ▼                                                       │
           ┌──────────────────────┐                                           │
           │    Nebula Gateway    │                                           │
           │  (Rust: 连接/鉴权/   │                                           │
           │   协议转换/C5门控)   │                                           │
           └──────────┬───────────┘                                           │
                      ▼                                                       │
           ┌──────────────────────┐                                           │
           │    Nebula Router     │                                           │
           │ (Rust: 内存选路/转发)│                                           │
           └──────────┬───────────┘                                           │
                      │                                                       │
                      │ (纯净内网请求直达)                                     │
                      ▼                                                       ▼
           ┌──────────────────────────────────────────────────────────────────────────┐
           │                     PowerLLM 推理服务系统 (计算与执行主体)                │
           │                                                                          │
           │  ┌────────────────────────────────────────────────────────────────────┐  │
           │  │ PowerLLM API / Worker 接入层 (双模通用，极致轻量)                  │  │
           │  │  • Phase 2: 本地内存选路缓存 (ReplicaEndpointCache，0 次 Actor RPC)│  │
           │  │  • Phase 3: Raw Body 透传 (不卡 uvloop，跳过大消息体 Pydantic 深拷贝)│  │
           │  │  • Phase 4: 原生流式管道直通与 aiohttp TCP 连接池                  │  │
           │  └─────────────────────────────────┬──────────────────────────────────┘  │
           │                                    │                                     │
           │  ┌─────────────────────────────────┴──────────────────────────────────┐  │
           │  │ PowerLLM 控制面 (Supervisor / StatusGuard / WorkerActor 拓扑)      │  │
           │  │  • Phase 1: 预热/健康检查异步化，消除热路径串行排队                │  │
           │  │  • 管理模型加载/停止、GPU 显存拓扑、副本状态防护与故障自愈        │  │
           │  └─────────────────────────────────┬──────────────────────────────────┘  │
           │                                    │                                     │
           │                                    ▼                                     │
           │                     底层推理引擎 (vLLM / SGLang Engine)                   │
           └──────────────────────────────────────────────────────────────────────────┘
```

---

## 2. 为什么前置 Nebula：收益分析

| 维度 | 无 Nebula（纯 Python 接入） | 前置 Nebula（Rust 网关层） | 核心收益 |
| :--- | :--- | :--- | :--- |
| **高并发连接与 I/O** | 面对弱网客户端、慢连接攻击（Slowloris）及大量 TLS 握手，易占用 Python Socket，拖垮 uvloop 事件循环。 | 由 Rust / Tokio 异步引擎终结外部连接，单机轻松承载数万并发长连接，内存开销仅数十 MB。 | 彻底隔离外部慢网络，Python 仅处理高速内网长连接。 |
| **请求取消（Abort）** | 客户端断开连接时，跨 Actor 流程取消感知滞后，底层 GPU 往往继续无谓推演数十 Token。 | **C2 级联取消契约**：客户端掐断瞬间，Rust 网关毫秒级中断向下游连接并触发 Abort。 | 杜绝无效推演，即时回收宝贵 GPU 算力。 |
| **协议方言兼容** | 若支持 Anthropic Messages 或 OpenAI Responses，需在 Python 端编写复杂解析与转换，加剧 GIL 竞争。 | **UniGateway 统一接口层**：Rust 零开销完成双向协议翻译，对外暴露全套生态方言。 | PowerLLM 只需维护标准 OpenAI 路径，零成本兼容全球主流生态。 |
| **报文防护与安全** | 畸形报文或超规请求（数十 MB）打入 Python 解析，触发内存激增和 GC 卡顿。 | 网关层零拷贝拦截非法报文、超大体积（413）及未授权请求。 | 脏流量不进内网，防止 Python 进程 OOM。 |
| **全局治理与切流** | 仅能在本集群内部 Worker 间轮询，跨集群容灾与蓝绿发布困难。 | **Nebula Router 选路**：支持跨集群分流、金丝雀灰度、加权最小连接调度。 | 具备全局多集群流量治理能力与故障秒级切流。 |

---

## 3. Nebula 侧架构：通用契约体系

Nebula 对下游推理服务保持**零业务侵入与通用契约假定**：

1. **协议规范契约（L1 契约 C1–C6，详见 [`contracts.md`](./contracts.md)）**：
   - **C1（OpenAI 路径）**：统一出站到下游服务遵循 `/v1/chat/completions` 标准。
   - **C2（Abort 级联取消）**：客户端中断时向下游发中断信号，不误计 5xx 错误预算。
   - **C3（标准错误码）**：502/504/429/413 等统一 Envelope，下游返回标准 OpenAI JSON 时原样透传。
   - **C4/C5（能力门控）**：根据下游注册的 `capabilities` 对 `tool_calling` 等高级能力进行网关前置门控拦截（拒绝静默丢参）。
2. **端点注册契约（Discovery Contract）**：
   - Nebula Router 仅通过 etcd 监听 `/endpoints/{model_uid}/{replica_id}`：
     ```json
     {
       "base_url": "http://<powerllm_ip>:<port>",
       "status": "ready",
       "weight": 100,
       "capabilities": {
         "tool_calling": "supported"
       }
     }
     ```
   - **通用性保证**：Nebula 不关心下游是 PowerLLM、vLLM 原生集群还是 Triton，任何服务只需按此契约维护带租约（Lease）的心跳即可纳入热路径。

---

## 4. PowerLLM 侧改动：双模兼容与自身优化

为了不破坏 PowerLLM 原有的单机部署与既有使用习惯，所有接入 Nebula 的特性均采用**外置插件与可选环境变量**。

### 4.1 自身的四大核心热路径优化（无论有无 Nebula 均生效）
1. **Phase 1（剪枝冗余 RPC）**：
   - 清除热路径上多余的 `status_guard_ref` 状态拉取；
   - 模型预热检查（`get_instance_warmup`）移出主请求链路，改为基于内存状态异步同步。
2. **Phase 2（本地端点选路缓存）**：
   - 实现 `ReplicaEndpointCache`，通过内存读取最小 In-Flight 副本，将选路 Actor RPC 从 3~4 次降为 0 次。
3. **Phase 3（长上下文 Raw Body 透传）**：
   - 在 HTTP 代理路径跳过 Pydantic 针对超长 messages 的深拷贝和二次序列化，直接透传原始 bytes。
4. **Phase 4（流式管道直通与连接池）**：
   - 保持 SSE chunk 原始字节传输，复用持久化 `TCPConnector`。

### 4.2 适配 Nebula 的增量改动（松耦合插件化）

```
backend/powerllm/v2/
├── api/
│   ├── restful_api.py          # 增加网关信任中间件 (继承 X-Request-Id / 跳过重复鉴权)
│   ├── infer_http_proxy.py     # 捕获 CancelledError 触发底座 Engine Abort
├── plugins/
│   └── nebula/
│       ├── __init__.py
│       └── registrar.py        # 可选组件：向 etcd /endpoints/ 注册带 Lease 的心跳
```

1. **端点注册插件 (`NebulaRegistrar`)**：
   - **配置驱动**：读取 `NEBULA_ETCD_ENDPOINTS`（未配置时不导入任何 etcd 依赖，零侵入）；
   - **生命周期挂钩**：当 Supervisor/Worker 确认 Model Replica 状态变为 `READY` 时，写入 `/endpoints/{model_uid}/{replica_id}` 并挂载 TTL 租约（5~10s）；当模型卸载或进程退出时主动撤销。
2. **网关信任与上下文透传 (`restful_api.py`)**：
   - 当检测到受信任网关请求头（如 `X-Nebula-Internal-Auth`）时，跳过重复的客户 Token 解析校验；
   - 提取并透传 `X-Request-Id`、`traceparent`，保证全链路调用链完整闭环。
3. **级联取消响应 (`infer_http_proxy.py`)**：
   - 响应客户端或网关断开产生的 `asyncio.CancelledError`，第一时间向底层 Engine 发出 Abort 指令，迅速释放显存算力。

---

## 5. 实施与验证路径

1. **第一阶段（PowerLLM 自身底座优化）**：
   - 落地 Phase 1~4，确保 PowerLLM 在独立无网关状态下单机短文本/长文本性能达到极致。
2. **第二阶段（Nebula 端点契约验证）**：
   - 在测试集群启动 Nebula Gateway 与 Router，手动向 etcd `/endpoints/` 注入 PowerLLM 服务的端点，验证 OpenAI / Anthropic 格式互转及 Abort 级联表现。
3. **第三阶段（自动注册插件接入）**：
   - 在 PowerLLM 中激活 `NebulaRegistrar`，实现集群实例上线、扩缩容、故障下线时在 Nebula 路由池的毫秒级自动化感知。
