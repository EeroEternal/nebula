# Nebula 部署指南

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

路由指标：

```bash
curl http://127.0.0.1:18081/metrics
```
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

网关指标：

```bash
curl http://127.0.0.1:8081/metrics
```

#### Control API 鉴权（可选）

为 `/v1/admin/*` 开启鉴权时，设置以下环境变量：

```bash
# token:role 以逗号分隔，role 为 admin/operator/viewer
export NEBULA_AUTH_TOKENS="devtoken:admin,viewtoken:viewer"

# 可选：每分钟每 token 的请求上限
export NEBULA_AUTH_RATE_LIMIT_PER_MINUTE=120
```

请求时携带 token：

```bash
curl -H "Authorization: Bearer devtoken" \
  http://127.0.0.1:8081/v1/admin/cluster/status

查看网关日志（tail 200 行）：

```bash
curl -H "Authorization: Bearer devtoken" \
  "http://127.0.0.1:8081/v1/admin/logs?lines=200"
```

访问 Web Console（MVP）：

```bash
open http://127.0.0.1:8081/v1/admin/ui
```
```
```

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

## 7. 端口汇总

| 服务 | 默认端口 | 说明 |
|------|----------|------|
| etcd | 2379 | 元数据存储 |
| vLLM | 10814 | 模型推理（由 node 自动启动） |
| Router | 18081 | 请求路由 |
| BFF | 18090 | 管理与 v2 API（审计/观测/模型管理） |
| Gateway | 8081 | 对外 API 入口 |
| xtrace | 8742 | 审计与观测后端 |

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

远端实操 SOP（登录、rsync 部署、重启、测试）见：`docs/memory/remote_login_deploy_test.md`。
