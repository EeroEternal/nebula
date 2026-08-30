# Nebula Agent Rules

This document defines rules and conventions for AI agents and developers working on the Nebula project.

## File Organization

- **Documentation:** Keep filenames in the `docs/` directory concise and descriptive. Avoid excessively long names.
  - `docs/manual/` — 产品与运维：能力说明、特性、部署、Gateway/Router、可观测、SLO、BFF、Catalog、HA
  - `docs/versions/` — Release Notes（`v1.3.0.md` 等）
  - `docs/dev/` — 工程细节：开发环境、计划、API 边界、etcd 边界、契约、UniGateway
  - `docs/arch/` — architecture and roadmap
  - Index: `docs/README.md`（唯一索引；子目录不再放 README）
  - Do not commit environment-specific runbooks, internal IPs, or secrets under `docs/`.
  - **Keep docs short:** prefer one screen of facts over essays. State current behavior, boundaries, and links to code; delete completed checklists, speculative plans, and historical analysis once shipped. Prefer renaming `*_integration.md` / `*_plan.md` to the topic name (e.g. `unigateway.md`) when the work is done.
  - **`manual/` 不写进度：** 不含 P0–P6、roadmap 勾选、Phase/O 编号；只写当前版本能力与运维做法。
  - **`manual/` 读者可能是非开发岗**（产品、运维）：先写「是什么、能干什么、出问题怎么办」；术语要解释；环境变量、PromQL 等放到「实施参考」小节。
  - **Short filenames:** use topic names, not suffixes — `plan.md` not `product_plan.md`, `roadmap.md` not `optimization.md`, `slo.md` not `slo_alerts.md`, `versions/v1.3.0.md` not `release_notes_v1.3.0.md`. Ops docs under `manual/`; version notes under `versions/`.
- **Scripts & Tests:**
    - All standalone test scripts, debug scripts, and utility scripts must be placed in the `scripts/` directory.
    - Production-ready binaries and service management scripts belong in `bin/`.
- **Temporary Data:** Do not store temporary data (like `default.etcd`) in the project root. Use `/tmp` or other designated temporary locations.

## etcd 使用约束规范

etcd 是 Nebula **控制面协调权威（排班黑板）**，不是通用数据库、不是对象存储、不是时序库。分类细则与 Keyspace：[`docs/dev/etcd.md`](docs/dev/etcd.md)、[`docs/arch/architecture.md`](docs/arch/architecture.md)。生产三节点运维：[`docs/manual/etcd-ha.md`](docs/manual/etcd-ha.md)。Owner：[`docs/dev/ownership.md`](docs/dev/ownership.md)。

### 定位（必须遵守）

- **唯一权威：** 声明式期望与协调真相以 etcd 为准；组件本地缓存可丢可重建；对账是「etcd vs runtime」清孤儿，禁止内存/DB/文件另立第二权威。
- **不替换：** 不得用 Kubernetes API、Postgres、Redis、对象存储等替代 etcd 作为 Deployment / Placement / Endpoint / 选主等协调真相源。K8s 仅可作 `k8s` runtime **执行面**（见下文）。
- **热路径：** Gateway / Router 选路只认 etcd `/endpoints/`（及既有 Placement 版本约束）；不得把推理热路径改成依赖 kube Service / EndpointSlice 或其它发现系统。

### 准入三问（缺一不可 → 默认不进 etcd）

写入 etcd 前必须同时满足：

1. **多组件共享**，需要 list / watch / CAS 协调；
2. **需要协调语义**（watch、lease、CAS、选主/fencing 之一）；
3. **体量小且只要最新值**（不是历史、不是大对象、不是高基数序列）。

BFF 仅自用的 CRUD → **默认 Postgres**，禁止顺手新开 etcd 前缀。

### 写入口（Single Owner）

- 每种状态 **只有一个写 Owner**（见 ownership 文档）。禁止 Node 与 K8s controller、或 Gateway 与 BFF，对同一副本/同一期望双重 reconcile。
- **Deployment / Spec / 治理配置：** BFF（主写）。**Placement：** 仅 Scheduler（CAS）。**`/endpoints/`、`/stats/`、`/capabilities/`、节点 status：** Node（或 `k8s` 形态下唯一的 in-cluster controller）写 lease。
- **禁止** 新增 Gateway 写 etcd 去重复 BFF 已有更新路径。
- **禁止** 无 Owner 文档更新就新增前缀。

### 生命周期

- **节点附属 / 进程消失应失效的 key** 必须挂 **Node 共享 lease**（或短 TTL）：`/nodes/…/status`、`/endpoints/`、`/stats/`、`/capabilities/`、`/image_status/`、`/model_cache/`、`/node_disk/`、`/alerts/`。
- `/download_progress/`：短 TTL。`/model_gc_requests/`：必须带 TTL，Node 处理后删除。
- **声明类配置**（Deployment、compat、tenants、images 等）无 lease，但须小、可版本化、可 CAS。

### 禁止写入（D 类）

不得写入 etcd：控制台账号 / SSO / 会话（Postgres）；Trace（xtrace）；指标历史 / 告警曲线（Prometheus）；日志（Loki）；模型权重 / 镜像层；审计全文 / token 明文；完整 Pod 清单或高基数集群事件；任何「越写越大」的历史窗口。Prometheus **禁止**高基数 `tenant_id` 等 label（与 etcd 无关也禁止）。

### 借住（C 类：允许暂留，禁止扩张）

`/templates/`、`/pricing/`、`/usage/`、`/benchmarks/`、`/model_profiles/`、遗留 `/model_requests/`（只读、禁止新写）为借住。触碰时 **优先迁 Postgres**（或外置存储）；**禁止** 为迁出而让 Scheduler / Node 读 Postgres；**禁止** 在借住前缀上增加新字段/新子树/新写入方。

### 新增 / 变更 checklist

改 etcd 相关代码或设计前，Agent / 开发者必须：

1. 用「准入三问」自检；不通过则改存 Postgres / Prometheus / 本地盘等。
2. 指定唯一写 Owner，并更新 [`docs/dev/ownership.md`](docs/dev/ownership.md)（若涉及归属）。
3. 更新 [`docs/dev/etcd.md`](docs/dev/etcd.md) 与 architecture Keyspace 表；拟议未实现的前缀须标明「拟议，勿写生产」（如 `/pools/`）。
4. 节点附属 key 选好 lease / TTL；工作队列必须可过期、可清理。
5. 确认 Gateway / Router 热路径无新增 etcd 写、无改发现模型。
6. 生产多节点时客户端使用逗号分隔多 `ETCD_ENDPOINT`；文档不写内网 IP / 密钥。

### 反模式（发现即改）

- 把用量窗口、benchmark 大结果、审计流水当 etcd 主存继续加字段。
- 用 etcd 做查询型业务库（复杂过滤、分页历史、全文检索）。
- 同一 `model_uid` 副本由两个组件同时写 Placement 或 `/endpoints/`。
- 本地缓存当真相、etcd 仅「备份」。
- 为图省事把密钥、大配置 blob、镜像层 metadata 塞进 key。

## K8s / HAMi boundaries

Nebula may add a **k8s** engine runtime for clusters that use Kubernetes GPU virtualization (e.g. HAMi). Plan: [`docs/dev/k8s.md`](docs/dev/k8s.md).

**Split of authority:**
- **etcd** remains the Nebula control-plane coordination authority (desired deployments/placements, `/endpoints/`, etc.). Do not replace it with the Kubernetes API as the source of truth for Nebula metadata.
- **Kubernetes / HAMi** is the **execution plane** for the `k8s` runtime only: schedule Pods, allocate virtual GPUs via resource requests / HAMi, start/stop engine containers.
- **Gateway / Router** keep selecting backends from etcd `/endpoints/`. Do not make the inference hot path depend on kube Service / EndpointSlice discovery.

**Single owner per runtime:**
- `process` / `docker`: `nebula-node` owns engine lifecycle on the host (unchanged).
- `k8s`: an in-cluster **controller / Operator** is the sole owner that apply/deletes Pods (or Workloads) and, when Ready, writes `/endpoints/` (lease). Never let Node and the K8s controller reconcile the same replica.

**GPU placement in `k8s` mode:** do not assign host `gpu_indices` or set `CUDA_VISIBLE_DEVICES` from Nebula Scheduler/Node. Request GPU (and HAMi memory/core annotations as needed) on the Pod spec; let kube-scheduler + HAMi allocate.

**Do not:** store full Pod manifests or high-cardinality cluster events in etcd; scatter ad-hoc `kubectl`/client-go Pod creates from Scheduler or Node on the main path (a dedicated controller is required); dual-reconcile the same model replica across Node and K8s.

## Versioning

- Follow semantic versioning for releases.
- Ensure `CHANGELOG.md` is updated when releasing a new version.

## Cursor Cloud specific instructions

Services and standard commands are documented in `README.md` and `docs/dev/setup.md`; the notes below only capture non-obvious cloud caveats.

- **Rust toolchain:** The workspace is pinned to Rust 1.92 by `rust-toolchain.toml` because UniGateway dependencies declare that MSRV. Install `rustup` and let Cargo select the pinned toolchain. Build binaries with `cargo build --workspace --release` before using `bin/nebula-up.sh` (it runs prebuilt binaries from `target/release/`).
- **etcd:** `etcd`/`etcdctl` live in `~/bin` (not on the default `PATH`). `bin/nebula-up.sh` invokes `~/bin/etcd` directly; call `etcdctl` with the full path.
- **PostgreSQL (only needed for the BFF/console):** start with `sudo pg_ctlcluster 16 main start`. BFF expects a dedicated `nebula` database reachable at `postgresql://postgres:postgres@127.0.0.1:5432/nebula`; it auto-creates tables and seeds the default console admin `admin` / `admin123` on first start.
- **Running the stack:** copy `deploy/nebula.env.example` to `deploy/nebula.env` (gitignored) and set `START_BFF=1`, `OBSERVE_AUTH_MODE=internal` (no xtrace token needed in dev). Then `./bin/nebula-up.sh`. Ports: gateway 8081, router 18081, bff 18090, etcd 2379, frontend 5173.
- **Gateway auth:** the OpenAI-compatible API enforces auth. Set `NEBULA_AUTH_TOKENS=dev-token:admin` (format `token:role` or `token:role:tenant_id`) and call with `Authorization: Bearer dev-token`, or set `NEBULA_AUTH_DISABLED=1` for dev. Optional multi-tenant admission: `NEBULA_MULTI_TENANT=1` plus etcd `/tenants/{id}` quotas.
- **No GPU here:** real inference via `nebula-node` + vLLM cannot run (needs GPU + model files; it is intentionally excluded from `docker-compose.yml`). To exercise the `gateway → router → engine` passthrough, register a mock OpenAI endpoint in etcd at `/endpoints/{model_uid}/{replica_id}` (JSON `EndpointInfo` with `status: "ready"` and a reachable `base_url`); the router picks it up within ~1s.
- **Frontend:** `npm run dev` in `frontend/` proxies `/api` → BFF on `:18090`. The login page is at `/login`; clearing `localStorage` alone does not redirect, so navigate to `/login` directly to reach the login form.
