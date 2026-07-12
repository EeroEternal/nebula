# 历史深度分析（归档）

> **归档说明（2026-07-11）：** 本文保留早期 crate 拓扑示意。过时结论请以 [`architecture.md`](./architecture.md) 与 [`optimization.md`](./optimization.md) 为准。  
> 已删除并并入 arch 的文档：`docs/dev/gpt56-analysis.md`、`absorb_powerllm_engineering_guidance.md`、`engineering_maturity_checklist.md`。

## 组件拓扑（仍有效）

```mermaid
graph TB
    Client["Client / Frontend"]
    GW["Gateway :8081"]
    BFF["BFF :18090"]
    Router["Router :18081"]
    Sched["Scheduler"]
    Node["Node Agent"]
    ETCD["etcd"]
    PG["PostgreSQL"]
    XT["xtrace"]
    Engine["vLLM / SGLang"]

    Client -->|"inference"| GW
    Client -->|"dashboard"| BFF
    GW -->|"proxy"| Router
    Router -->|"proxy"| Engine
    BFF -->|"auth"| PG
    BFF -.->|"meta"| ETCD
    BFF -->|"observe"| XT
    Router -.->|"meta"| ETCD
    Sched -.->|"reconcile"| ETCD
    Node -.->|"watch"| ETCD
    Node -->|"manage"| Engine
```

## Crate 依赖（仍有效）

```mermaid
graph BT
    common["nebula-common"]
    meta["nebula-meta"]
    router["nebula-router"]
    scheduler["nebula-scheduler"]
    node["nebula-node"]
    gateway["nebula-gateway"]
    bff["nebula-bff"]
    cli["nebula-cli"]

    meta --> common
    router --> common
    router --> meta
    scheduler --> common
    scheduler --> meta
    node --> common
    node --> meta
    gateway --> common
    gateway --> meta
    bff --> common
    bff --> meta
    cli --> common
```

无循环依赖。更细的旧问题清单（BFF 重复、HTTP client 分散等）已迁入 [`optimization.md`](./optimization.md) 的 **N2**，不再在此维护。
