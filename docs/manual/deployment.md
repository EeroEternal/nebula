# Nebula 部署指南

> **给谁看：** 实施工程师按下面步骤安装；产品/运维只需知道「装好后用控制台 `:5173`、API 走网关 `:8081`」。  
> 各模块说明（网关、监控、多副本等）：[`module.md`](./module.md)。

## 0. 装完以后怎么用

| 入口 | 地址（默认） | 用途 |
|------|--------------|------|
| 控制台 | `http://<主机>:5173` | 部署模型、看监控、管租户 |
| 推理 API | `http://<主机>:8081` | 业务系统调用大模型（OpenAI 兼容） |
| 控制台后台 | `http://<主机>:18090` | 一般无需直接访问，由控制台页面调用 |

推荐用 `deploy/nebula.env` + `./bin/nebula-up.sh` 一键启动，见本文第 10 节。

## 1. 环境要求

| 依赖 | 版本/说明 |
|------|-----------|
| **Rust** | stable（用于编译 nebula 各组件） |
| **etcd** | v3.x，监听 `127.0.0.1:2379` |
| **vLLM** | 支持 `vllm serve` 的版本 |
| **NVIDIA GPU** | 需要足够显存加载目标模型 |
| **CUDA** | 与 vLLM 兼容的版本 |

## 2. 编译

```bash
cargo build --workspace
```

编译产物位于 `target/debug/`（或 `target/release/`），包含：
- `nebula-scheduler`
- `nebula-node`
- `nebula-router`
- `nebula-gateway`
- `nebula-bff`

## 3. 启动 etcd

使用 Docker 快速启动：

```bash
docker run -d --name etcd \
  -p 2379:2379 -p 2380:2380 \
  quay.io/coreos/etcd:v3.5.0 \
  /usr/local/bin/etcd \
  --advertise-client-urls http://0.0.0.0:2379 \
  --listen-client-urls http://0.0.0.0:2379
```

验证健康：

```bash
curl http://127.0.0.1:2379/health
# 预期：{"health":"true","reason":""}
```

## 4. 按顺序启动服务

### 4a. Scheduler — 写入 PlacementPlan

```bash
./target/debug/nebula-scheduler \
  --model-uid qwen2_5_0_5b \
  --model-name Qwen/Qwen2.5-0.5B-Instruct \
  --node-id $(hostname) \
  --engine vllm \
  --port 10814
```

确认输出包含 `placed: key=/placements/qwen2_5_0_5b ...`。

### 4b. Node — 拉起 vLLM 并注册 endpoint

```bash
./target/debug/nebula-node \
  --node-id $(hostname) \
  --etcd-endpoints http://127.0.0.1:2379
```

Node 会：
1. Watch `/placements/` 获取分配给本机的 assignment
2. 启动 vLLM 进程（`vllm serve`）
3. 等待 vLLM ready 后注册 `/endpoints/{model_uid}/{replica_id}`

确认日志包含 `registered endpoint ... base_url=http://127.0.0.1:10814`。

### 4c. 验证 vLLM 就绪

```bash
curl http://127.0.0.1:10814/v1/models
# 预期：200 OK，包含模型名称
```

### 4d. Router — 路由请求到 endpoint

```bash
./target/debug/nebula-router \
  --listen 0.0.0.0:18081 \
  --etcd-endpoints http://127.0.0.1:2379
```

验证健康：

```bash
curl http://127.0.0.1:18081/healthz
# 预期：ok
```

路由指标：

```bash
curl http://127.0.0.1:18081/metrics
```

### 4e. Gateway — 对外提供 OpenAI API

```bash
./target/debug/nebula-gateway \
  --listen 0.0.0.0:8081 \
  --router-url http://127.0.0.1:18081
```

验证健康：

```bash
curl http://127.0.0.1:8081/healthz
# 预期：ok
```

网关指标：

```bash
curl http://127.0.0.1:8081/metrics
```

#### Control API 鉴权（可选）

为 `/platform/v1/*` 与推理 API 开启鉴权时，设置以下环境变量：

```bash
# token:role 或以 token:role:tenant_id 绑定租户，逗号分隔；role 为 admin/operator/viewer
export NEBULA_AUTH_TOKENS="devtoken:admin,viewtoken:viewer,acme-token:operator:acme"

# 可选：每分钟每 token（或每租户）的请求上限
export NEBULA_AUTH_RATE_LIMIT_PER_MINUTE=120

# 可选：开启多租户配额准入（读 etcd /tenants/）；关闭时保持单 token 兼容
# export NEBULA_MULTI_TENANT=1
```

请求时携带 token：

```bash
curl -H "Authorization: Bearer devtoken" \
  http://127.0.0.1:8081/platform/v1/health/summary
```

查看网关日志（tail 200 行）：

```bash
curl -H "Authorization: Bearer devtoken" \
  http://127.0.0.1:8081/platform/v1/cluster/status
```

控制台前端默认走 BFF（`:18090` / `npm run dev` → `:5173`），治理页可维护租户、兼容矩阵、SLO 与 Benchmark。

## 5. BFF 与 xtrace 鉴权模式（安装部署建议）

BFF 提供 `/api/v2/*`、`/api/audit-logs`、`/api/observe/*` 等管理与观测接口，访问 xtrace 时支持两种模式：

- `OBSERVE_AUTH_MODE=internal`：内网信任模式，不向 xtrace 发送 Bearer token（开发环境推荐）
- `OBSERVE_AUTH_MODE=service`：服务鉴权模式，必须配置 `OBSERVE_TOKEN`（生产环境推荐）

### 开发环境推荐（默认）

```bash
export OBSERVE_URL=http://127.0.0.1:8742
export OBSERVE_AUTH_MODE=internal

./target/debug/nebula-bff \
  --listen-addr 0.0.0.0:18090 \
  --etcd-endpoint http://127.0.0.1:2379 \
  --router-url http://127.0.0.1:18081
```

### 生产环境推荐

```bash
export OBSERVE_URL=http://xtrace:8742
export OBSERVE_AUTH_MODE=service
export OBSERVE_TOKEN=<your-internal-service-token>

./target/debug/nebula-bff \
  --listen-addr 0.0.0.0:18090 \
  --etcd-endpoint http://127.0.0.1:2379 \
  --router-url http://127.0.0.1:18081
```

当 `OBSERVE_AUTH_MODE=service` 且 `OBSERVE_TOKEN` 为空时，BFF 会返回配置错误，避免误以为是权限问题。

### 避免 `{"message":"Unauthorized"}` 的固定检查清单

`/api/audit-logs` 常见 `Unauthorized` 根因是 xtrace token 未配置或配置未生效。建议每次部署都执行以下检查：

```bash
# 1) 确认统一配置文件存在
test -f ~/github/nebula/deploy/nebula.env && echo "nebula.env ok"

# 2) 确认三项关键配置
grep -E '^OBSERVE_URL=|^OBSERVE_AUTH_MODE=|^OBSERVE_TOKEN=' ~/github/nebula/deploy/nebula.env

# 3) 推荐自动对齐 xtrace token（从 xtrace .env 读取）
TOKEN=$(grep -E '^API_BEARER_TOKEN=' ~/github/xtrace/.env | head -n1 | cut -d= -f2-)
sed -i "s|^OBSERVE_TOKEN=.*$|OBSERVE_TOKEN=${TOKEN}|" ~/github/nebula/deploy/nebula.env

# 4) 重启 Nebula 并验证 audit API
cd ~/github/nebula && ./bin/nebula-down.sh || true && START_BFF=1 ./bin/nebula-up.sh
curl -sS -i "http://127.0.0.1:18090/api/audit-logs?limit=1" -H "Authorization: Bearer devtoken" | sed -n '1,20p'
```

期望：返回 `HTTP/1.1 200 OK`。

## 6. 快速验证

### 非流式 Chat

```bash
curl -s http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "Qwen/Qwen2.5-0.5B-Instruct",
    "messages": [{"role": "user", "content": "Hello"}]
  }' | python3 -m json.tool
```

### 流式 Chat

```bash
curl -N http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "Qwen/Qwen2.5-0.5B-Instruct",
    "messages": [{"role": "user", "content": "Hello"}],
    "stream": true
  }'
```

### 流式 Responses

```bash
curl -N http://127.0.0.1:8081/v1/responses \
  -H "Content-Type: application/json" \
  -d '{
    "model": "Qwen/Qwen2.5-0.5B-Instruct",
    "input": "Hello",
    "stream": true
  }'
```

### 奖励模型打分（Score API，SGLang 后端）

```bash
curl -s http://127.0.0.1:8081/v1/score \
  -H "Content-Type: application/json" \
  -d '{
    "model": "reward-model-uid",
    "query": "What is 2+2?",
    "items": ["4", "5"]
  }' | python3 -m json.tool
```

> 说明：`POST /v1/score` 受引擎能力矩阵 `score_api` 门控（`sglang` 原生支持；`vllm` 返回 `400 score_api_unsupported`，重排场景请使用 `POST /v1/rerank`）。

## 7. 端口汇总

| 服务 | 默认端口 | 说明 |
|------|----------|------|
| etcd | 2379 | 元数据存储 |
| vLLM | 10814 | 模型推理（由 node 自动启动） |
| Router | 18081 | 请求路由 |
| BFF | 18090 | 管理与 v2 API（审计/观测/模型管理） |
| Gateway | 8081 | 对外 API 入口 |
| xtrace | 8742 | 审计与观测后端 |

## 7.5 版本固定与升级门禁（交付收敛，2026-10-12）

**版本清单（单一事实源）**——升级前先过对应门禁：

| 组件 | 钉死版本 | 升级门禁 |
|------|----------|----------|
| nebula 控制面 | workspace 版本（当前 1.10.0） | `./scripts/ci.sh`（本地全量） |
| vLLM 引擎镜像 | `vllm/vllm-openai:v0.23.0` | `./scripts/check_lmcache_compat.sh`（quick） |
| lmcache（镜像内 pip 升级） | ≥ 0.5.5（0.4.6 无 NIXL 路径崩溃） | 同上 + `--full`（改 pin/镜像时必跑） |
| etcd | `quay.io/coreos/etcd:v3.5.17` | `etcdctl endpoint health` |
| kv-events 线上格式 | v0.23 字节级校准（map-form/int-hash） | 换引擎版本 = 重跑 prefix-affinity §5 校准 |

> **升级纪律**：先在测试环境过门禁 → 再改生产 pin。禁止跳门禁直接升。

## 7.6 P1c 前缀亲和与 `pd_split` / 阶段感知部署矩阵

按角色查表；除 `pd_split` 拓扑下的 `NEBULA_APPROVED_PD_IMAGES` 外全部可选（不设 = 现状行为）：

| 变量 | 设在谁 | 作用 | 默认值 / 真机验证值 |
|------|--------|------|------------|
| `NEBULA_ROUTER_STRATEGY` 或 `--routing-strategy` | Router | `prefix_affinity` | ✅ +74% rps 实测 |
| `NEBULA_ROUTER_KV_EVENTS_BIND` | Router / indexd | 引擎 PUB 连过来的 SUB 端点 | `tcp://0.0.0.0:5557` |
| `NEBULA_ROUTER_TOKENIZER_DIR` | Router | tokenizer 目录（`<dir>/<model_name>/tokenizer.json` + `tokenizer_config.json`） | ✅ chat 哈希必需 |
| `NEBULA_KV_HASH_SEED` | Router **和** 执行面（同值！） | block-hash 种子，进引擎 `PYTHONHASHSEED` | `42`（不一致=永不命中，可见回落） |
| `NEBULA_KV_EVENTS_ENDPOINT` | k8s-controller / node | 引擎侧注入 `--kv-events-config` 等 | Router 的 SUB 地址 |
| `NEBULA_ROUTER_PREFIX_TTL_MS` | Router / indexd | 索引 TTL | `300000` |
| `NEBULA_INDEXD_BIND` | indexd | 引擎唯一稳定端点（Batch 2） | 同 5557 语义 |
| `NEBULA_INDEXD_SINKS` | indexd | 逗号分隔 Router sink 列表 | `tcp://host:5583,...` |
| `NEBULA_INDEXD_SNAPSHOT_SECS` | indexd | 快照周期（Router 重启自愈） | `60` |
| `NEBULA_ROUTER_KV_EVENTS_SINK_BIND` | 副本 Router | 收 indexd/对端推送 | `tcp://0.0.0.0:5583` |
| `NEBULA_ROUTER_KV_EVENTS_PUSH_TO` | 采集 Router | HA 扇出（不用 indexd 时） | 对端 sink 列表 |
| `NEBULA_APPROVED_PD_IMAGES` | BFF / CLI / Scheduler / k8s-controller | `topology=pd_split` 允许运行的精确镜像字符串白名单（逗号分隔精确匹配，建议仅填入经过门禁的固定版本/digest 镜像而非浮动 tag） | `pd_split` 必填（如 `nebula/vllm:0.23.0-lmcache0.5.5`） |
| `NEBULA_PD_BYPASS_TOKENS` | Router | `pd_split` 短请求直通 Decode 池的 prompt token 阈值（需 Decode 副本有新鲜 `/stats/` 且 `pending_requests == 0` 或 `<= min_prefill_pending`） | `256`（设为 `0` 关闭） |
| `NEBULA_PD_PREFILL_SPILLOVER_PENDING` | Router | `pd_split` 冷请求在 Prefill 池积压达阈值（`min_prefill_pending >= 阈值`）且某 Decode 副本有新鲜 `/stats/` 且 `pending_requests < min_prefill_pending` 时溢出直跑 Decode 的阈值 | `0`（默认关闭；显式设 `>0` 开启） |
| `NEBULA_TOOL_PAUSE_TTL_MS` | Router | Agentic tool-call 暂停窗口内租户级 `(tenant_id, x-session-id)` 单次续接租约 TTL（毫秒） | `60000` |

**`pd_split` 的启动顺序**：`lmcache_controller` / `lmcache_server` 必须先于引擎就绪 —— 设了 `NEBULA_LMCACHE_CONTROLLER_URL` 时，`nebula-node`（引擎启动前）与 `k8s-controller`（建 Pod 前）会**等 controller 的 TCP 端口可连**，超时（`NEBULA_LMCACHE_CONTROLLER_WAIT_MS`，默认 node 60s / controller 30s）则本次不启动、下一轮重试。引擎先起时 worker 能注册但**不会上报 KV 键**，`/lookup` 恒返回空、`pd_lookup_warm_total` 恒 0，`pd_split` 静默退化为单 Prefill 副本，**且只能靠重启引擎恢复**。路由器在连续 50 次全冷后打一条警告并计入 `stage_stats_missing_total{decision="warm_signal_cold"}`。

**先看有没有 stats**：`pd_split` 的短请求旁路、Prefill 溢流、以及 `least_pending` 在副本间分摊，全部依赖 `/stats/{model_uid}/{replica_id}`（由 `nebula-node` 或 `k8s-controller` 抓引擎 `/metrics` 写入）。指标 `nebula_router_stage_stats_missing_total` 非零或 `nebula_route_stale_stats_dropped_total` 持续增长，就说明这套管道没通 —— 此时任何 `pd_split` 的收益数字都不成立。

**运维三件套（缺一则亲和永不命中，但安全回落可见）**：引擎 `PYTHONHASHSEED` 固定 + `--prefix-caching-hash-algo sha256_cbor`（执行面设 `NEBULA_KV_EVENTS_ENDPOINT` 后自动注入）+ Router tokenizer 目录。

**排障核心指标**：
- `nebula_router_strategy_fallback_total{reason="no_signal"}`：持续增长 = 亲和没生效（查三件套）；
- `nebula_router_strategy_fallback_total{reason="owner_congested"}`：前缀命中的 owner 副本过载或排队过深，已触发多 owner 次优回退或负载感知回落；
- `nebula_router_prefix_index_entries`：当前前缀索引规模；
- `nebula_router_pd_decode_local_hit_total`：`pd_split` 命中 Decode 池本地显存（Tier-1 跳过 Prefill 与远端 KV 拉取）计数；
- `nebula_router_pd_short_bypass_total`：`pd_split` 短请求直通空闲 Decode 副本计数；
- `nebula_router_saved_prefill_tokens_total{model_uid}`：**缓存命中的收益口径**（Batch 6.4）—— 被服务副本实际已持有的 prefill token 数（块数 × 16）。冷请求与非所选副本的命中都不计入，可当作节省量的保守下界。
- `nebula_router_stage_prefill_spillover_total`：`pd_split` Prefill 池饱和时溢出至空闲 Decode 副本计数。
- `nebula_router_stage_stats_missing_total{decision="short_bypass"|"prefill_spillover"|"warm_signal_cold"}`：**因缺少新鲜 `/stats/` 而无法评估**的阶段决策计数。非零意味着 `/stats/` 管道没通（`nebula-node` / `k8s-controller` 的 scrape 没跑或副本不上报），此时短请求旁路与 Prefill 溢流**静默失效**，`pd_split` 会退化成"全部走 Prefill"——排查顺序见下方「数值先看它」。

## 7.7 etcd 备份与恢复

```bash
# 备份（cron 建议每小时）
docker exec nebula-etcd etcdctl --endpoints=127.0.0.1:12379 \
  snapshot save /backup/etcd-$(date +%Y%m%d%H%M).db

# 验证快照
etcdctl snapshot status /backup/etcd-xxx.db

# 恢复（新数据目录起空集群）
etcdctl snapshot restore /backup/etcd-xxx.db --data-dir /var/lib/etcd-restore
# 然后用该 data-dir 重启 etcd 容器
```

> 模型/部署期望态全在 etcd；Postgres（BFF 业务数据）用 `pg_dump` 常规备份。**KV 前缀索引不在备份范围**（可丢可重建缓存）。

## 7.8 升级与回滚

**升级（控制面二进制）**：
1. 测试环境过 `./scripts/ci.sh`；
2. 目标机 `git pull && cargo build --release`（注意 GLIBC：在目标机或同基镜像内构建）；
3. `./scripts/nebula-stack.sh restart-control-plane`（仅滚动重启 Router / Gateway / BFF / k8s-controller，保留 etcd 中的 `/deployments/{model_uid}` 期望与运行中的引擎 Pod；注意不要使用 `stop`，`stop` 为全栈下线，会删除 deployment 期望与模型 Pod）；
4. 验证：`probe_stack.sh` + 发一条推理请求 + Router `/metrics` 无 error 计数增长。

**升级（引擎镜像/lmcache）**：
1. 过 `check_lmcache_compat.sh`（改 pin 必跑 `--full`）；
2. 更新 ModelSpec 的 `docker_image` → controller 感知 → pod 重建（声明式，无需手动删 pod）。

**回滚** = 反向重跑上述步骤（版本回退 + 重启）；etcd 数据无需回滚（结构体 serde-default 向后兼容）。引擎回滚后**必须**抽查 `no_signal` 计数（线上格式校准风险点）。

## 7.9 多 gateway 前置接入：xLB（TLS 卸载 + 整机故障切换）

> ⚠️ **外部组件声明**：xLB（[`EeroEternal/xlb`](https://github.com/EeroEternal/xlb)，MIT，独立发布）**不在本仓**，不受 `./scripts/ci.sh` 覆盖，本仓也不为其配置/版本做回归。本章只声明接入点、约束与代价；参数以其官方 `config.toml` 为准，且**未经本仓真机验证**。

### 什么时候值得加

同时满足以下三条才值得（缺一条就先别加，见文末「什么时候不要用」）：

1. **≥2 个 gateway 实例**（多机或同机多副本），需要**整机级**故障切换——即某个 gateway 进程/主机挂了，活跃流式请求能落到别的实例；
2. 需要**在一处终止 TLS**（客户端 → Nebula 走 HTTPS），而不是让每个 gateway 各自管证书；
3. 需要把 **POST 连接池与超时语义从 gateway 进程里剥离**，让 gateway 可以自由滚动重启而不掐断已有流。

### 它不做什么（边界）

- xLB **只搬字节，不懂 token**：不知道 replica、`/endpoints/`、KV 冷暖、`prefill/decode` 角色。**它不能替代 `nebula-router`**，两者不在同一层。
- xLB 在 **gateway 外面**：`Client → xLB(TLS) → nebula-gateway ×N → nebula-router → 引擎`。Router 侧的 `prefix_affinity` / `pd_split` / `Tool-Pause` 全部照旧在 gateway/router 内部生效。
- 它**不做**鉴权、配额、路由决策、SLO 归因 —— 这些仍是 Gateway/BFF 的职责。

### 关键约束（顺序错了会丢错误归因）

```text
Client ──HTTPS──▶ xLB(thin-l7, TLS 终止) ──HTTP──▶ gateway-1 / gateway-2 / … ──▶ router ──▶ 引擎
                       └─ 上游池 = gateway 地址（默认 8081）        └─ 内部端口见 §7 端口汇总
```

1. **xLB 的超时必须比 Nebula 自己更宽松**。Nebula 的代理分档是 `connect 3s / first_byte 60s（NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS）/ idle 120s（NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS）/ 无总超时`。若 xLB 先掐断，Nebula 就看不到这次失败，指标记在 xLB 上、`nebula_route_*` 直方图失真。建议 xLB 侧：

   ```toml
   [timeouts]
   connect    = "5s"
   first_byte = "75s"    # > NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS (60s)
   idle       = "180s"   # > NEBULA_PROXY_STREAM_IDLE_TIMEOUT_MS (120s)
   total      = "3600s"  # 只在极端兜底；不要设成会截断长生成的量级
   ```

2. **只在首字节之前重路由**。流式一旦开始吐字就不能换后端（客户端已经收到增量）。xLB `thin-l7` 的语义正是如此，不要为了"更高可用"去开首字节后重试。
3. **重路由状态码要收窄**。在**gateway 副本之间**重试 `429` 是惊群放大器（429 通常意味着整个集群饱和，不是这个副本坏了）——这与 provider 型网关（各上游配额独立）不同。建议 `retry_statuses = [500, 502, 503, 504]` 并按需加 `404`（副本配置错时换一个有意义）。
4. **健康检查指向 gateway 的 `/healthz`**，不要指 router 的 `/healthz`（否则 xLB 会认为 gateway 不可用）。要反映「这个 gateway 能不能接请求」。
5. **容量按"并发流数"配**，不是按 QPS：每条长 SSE 都占住一条上游连接。`pool.max_idle_per_backend` / `max_conns_per_backend` 要 ≥ 目标并发流数 / gateway 副本数。

### 代价（必须一起声明）

| 代价 | 说明 |
|------|------|
| 串联两个代理 | TTFT 多一份 delta。用 xLB 自带 `xlb_ttft_delta_ms` 量化；若这段延迟不可接受，说明瓶颈本来不在代理层 |
| 排障面翻倍 | 一个 5xx 可能来自 xLB、gateway 或引擎，需要 `x-request-id` 贯穿（Nebula 已注入，确认 xLB 透传） |
| 新增一个故障域 | xLB 挂掉时即使 gateway 全健康也不可用 → **要么 ≥2 个 xLB + VIP/K8s Service，要么别上** |
| 不在本仓 CI | 版本升级、配置漂移都要你自己回归；xLB 的发布节奏与 Nebula 无关 |
| 双份 metrics | `nebula_route_*`（gateway/router）与 `xlb_*`（xLB）要分开看，别混成一套 SLO |

### 最小配置片段（多机 + TLS 卸载）

```toml
[server]
bind = "0.0.0.0:443"
mode = "thin-l7"

[server.tls]
enabled = true
cert = "/etc/xlb/tls/fullchain.pem"
key  = "/etc/xlb/tls/privkey.pem"

[backends]
algorithm = "leastconn"
servers = [
  { addr = "10.0.0.11:8081", weight = 1 },   # gateway 副本 1
  { addr = "10.0.0.12:8081", weight = 1 },   # gateway 副本 2
]

[retry]
max_retries = -1
retry_statuses = [404, 500, 502, 503, 504]   # 故意不含 429，见「关键约束 3」
```

### 客户端可见时序：状态码在首块到达之后才提交

`nebula-router` 的流式路径会**先读到上游首块、再向客户端提交 `200`**（[`first_byte.rs`](../../crates/nebula-router/src/first_byte.rs)）。因此：

- 上游副本"接了请求但不出字"时，router 会换副本重试；所有副本都在预算内没出字才返回 `503 + Retry-After`，而不是先回 `200` 再吐一条截断的流。
- **响应头（含 `x-nebula-*` 回显头）会晚到首个 token**。依赖"尽早拿到 header"的客户端/代理需要知道这一点；对 LLM 流式来说 header 本身没有信息量。
- 预算两个旋钮：单次尝试 `NEBULA_PROXY_FIRST_BYTE_TIMEOUT_MS`（默认 60s），跨尝试总计 `NEBULA_PROXY_FIRST_BYTE_TOTAL_BUDGET_MS`（默认 90s）。**不要只调大前者**，否则坏集群会把延迟按尝试次数放大。
- 两条路径都已实现：`nebula-router` 与 `NEBULA_ROUTER_MODE=embedded`（gateway 进程内选 endpoint）都在首块到达后才提交状态；embedded 模式的重试计数是 `nebula_gateway_first_byte_retry_total`，router 模式是 `nebula_router_first_byte_retry_total`。

### 缓存生命周期 API（Batch 6.3）

给运维/Agent 用的 KV 缓存操作面，全部走 BFF。需要 **`NEBULA_LMCACHE_CONTROLLER_BASE_URL`**（controller 的 HTTP API，通常 `http://<host>:9000`；未配置时返回 400 并说明）。注意它与执行面用的 `NEBULA_LMCACHE_CONTROLLER_URL` **不是同一个东西** —— 后者是引擎注册用的 ZMQ monitor 端点（`LMCACHE_CONTROLLER_PULL_URL`），拿它当 HTTP 地址会把 API 调用打到 ZMQ 端口：

| 端点 | 权限 | 作用 |
|------|------|------|
| `POST /api/v2/models/{uid}/cache/lookup` | Viewer | `{tokens:[...]}` → `matched_tokens`（0 = 冷） |
| `POST /api/v2/models/{uid}/cache/pin` | Operator | `{tokens:[...], location?}` → 锁住热门前缀，防驱逐 |
| `POST /api/v2/models/{uid}/cache/clear` | Operator | `{location?}` → 清空该实例的缓存 |
| `POST /api/v2/models/{uid}/cache/warmup` | Operator | `{prompt}` → 走 Router 真跑一次推理，把 KV 算出来写进共享存储 |

- `tokens` 必须由调用方提供：控制面没有分词器；`warmup` 之所以收 prompt，是因为**只有引擎能产出 KV**，controller 只能 pin 已存在的。
- `location` 默认 `LocalCPUBackend`（L1 DRAM 层）。
- `pin` / `clear` 按 **`{model_uid}-{replica_id}`** 逐个实例下发（与引擎注册时 `LMCACHE_LMCACHE_INSTANCE_ID` 同一个 helper `lmcache_instance_id` 生成，两边不会漂）；返回体带 `instances` 与逐实例 `failures`，部分失败不会被报成成功。
- `lookup` 的命中数按 `{model_uid}-` 前缀过滤，多模型共用一个 controller 时不会把别的模型的缓存算成本模型的命中。
- 没有 worker 注册时 `pin`/`clear` 会返回上游 500（`The number of tokens … should be the same across all workers.`），说明 shape 正确但没有可操作对象 —— 先确认引擎已注册（`query_worker_info`）。

### 什么时候不要用

- **只有一个 gateway 实例**：多一跳代理，没有任何故障切换收益。
- **已经跑 `NEBULA_ROUTER_MODE=embedded`（单跳数据面）**：再加一层就变成三段跳，TTFT 收益被抵消。
- **前面已经有 L4 LB + VIP 或 K8s Service**：先确认那层是否已经满足「整机切换 + TLS 卸载」，多数环境已经够用。
- **需要按 token/KV 做决策**：那是 `nebula-router` 的职责，xLB 永远做不了。

## 8. 常见问题

### 8.1 `{"message":"Unauthorized"}`（Audit Logs）

按顺序排查：

1. `deploy/nebula.env` 是否存在；
2. `OBSERVE_AUTH_MODE` 是否为 `service` 且 `OBSERVE_TOKEN` 非空；
3. `OBSERVE_TOKEN` 是否与 `~/github/xtrace/.env` 的 `API_BEARER_TOKEN` 一致；
4. 重启后确认进程参数中包含 `--xtrace-token` 非空：

```bash
pgrep -af 'nebula-bff|nebula-gateway'
```

5. 直接验证 xtrace：

```bash
TOKEN=$(grep -E '^API_BEARER_TOKEN=' ~/github/xtrace/.env | head -n1 | cut -d= -f2-)
curl -sS -i "http://127.0.0.1:8742/api/public/traces?tags%5B%5D=audit&limit=1" \
  -H "Authorization: Bearer ${TOKEN}" | sed -n '1,20p'
```

### 端口被占用

启动前检查端口是否空闲：

```bash
ss -tlnp | grep -E '(8081|18090|18081|10814|8742)'
```

如有残留进程，先清理：

```bash
pkill -f 'nebula-router'
pkill -f 'nebula-gateway'
pkill -f 'nebula-node'
pkill -f 'vllm serve'
```

### vLLM 孤儿进程

`pkill -f 'vllm serve'` 可能无法杀掉 vLLM 的子进程（如 `VLLM::EngineCore`）。需要：

```bash
# 查找残留的 vLLM 相关进程
nvidia-smi  # 查看 GPU 占用
ps aux | grep -i vllm

# 手动 kill 孤儿进程
kill <pid>
# 如果 TERM 无效
kill -9 <pid>
```

### Router 启动无输出

Router 如果绑定端口失败会静默退出。启动时建议加 `RUST_BACKTRACE=1`：

```bash
RUST_BACKTRACE=1 ./target/debug/nebula-router --listen 0.0.0.0:18081 --etcd-endpoints http://127.0.0.1:2379
```

---

## 9. Docker Compose（控制面）

提供基础控制面编排（含 etcd/gateway/router/scheduler/bff，不含 GPU 的 nebula-node / vLLM）。

```bash
docker compose up -d --build
```

开发环境可直接：

```bash
OBSERVE_AUTH_MODE=internal docker compose up -d --build
```

生产建议：

```bash
OBSERVE_AUTH_MODE=service \
OBSERVE_TOKEN=<internal-service-token> \
OBSERVE_URL=<xtrace-endpoint> \
docker compose up -d --build
```

如需在 compose 内同时启动 xtrace（可选）：

```bash
OBSERVE_URL=http://xtrace:8742 \
OBSERVE_AUTH_MODE=service \
OBSERVE_TOKEN=<internal-service-token> \
docker compose --profile observe up -d --build
```

可选环境变量：
- `OBSERVE_IMAGE`：observe（xtrace）镜像地址（默认 `ghcr.io/lipish/xtrace:latest`）
- `OBSERVE_DATABASE_URL`：observe（xtrace）数据库连接串（建议数据库名为 `observe`）

默认使用 `NEBULA_AUTH_TOKENS="devtoken:admin,viewtoken:viewer"`，可在 `docker-compose.yml` 中调整。

## 10. 统一启动配置（nebula.env）

推荐使用单一配置源 `deploy/nebula.env` 管理启动参数，避免脚本参数和手工命令漂移。

```bash
cp deploy/nebula.env.example deploy/nebula.env
```

至少设置以下字段：

```bash
START_BFF=1
BFF_DATABASE_URL=postgresql://<nebula_user>:<nebula_pass>@127.0.0.1:5432/nebula
OBSERVE_URL=http://127.0.0.1:8742
OBSERVE_AUTH_MODE=service
OBSERVE_TOKEN=<API_BEARER_TOKEN>
```

注意：`BFF_DATABASE_URL` 必须使用 Nebula 独立数据库（推荐 `nebula`），不要与 observe（xtrace）数据库（推荐 `observe`）共用。

随后直接使用：

```bash
./bin/nebula-up.sh
./bin/nebula-down.sh
```

可通过环境变量覆盖配置文件路径：

```bash
NEBULA_ENV_FILE=/path/to/nebula.env ./bin/nebula-up.sh
```

远端部署可参考 [`module.md`](./module.md)「高可用」与 [`../dev/setup.md`](../dev/setup.md)。
