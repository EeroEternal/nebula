# Nebula 端到端测试记录

## 测试日期

2025-02-08

## 测试环境

- **模型**：Qwen/Qwen2.5-0.5B-Instruct
- **引擎**：vLLM（native HTTP passthrough 模式）
- **元数据存储**：etcd v3，`127.0.0.1:2379`
- **GPU**：NVIDIA（单机单卡）

## 服务拓扑

```
Gateway (8081) → Router (18081) → vLLM (10814)
                                      ▲
                              Node Daemon 管理
                                      ▲
                              Scheduler 写入 PlacementPlan → etcd (2379)
```

---

## 1. 编译

| 步骤 | 结果 |
|------|------|
| `cargo build --workspace` | ✅ 成功（仅有少量 `dead_code` warning，不影响运行） |

## 2. 基础服务检查

| 检查项 | 结果 |
|--------|------|
| etcd 健康 (`/health`) | ✅ `{"health":"true","reason":""}` |
| 端口 8081/18081/10814 空闲 | ✅ 清理后确认无冲突 |
| GPU 无残留进程 | ✅ 清理后确认（见下方说明） |

### 清理过程

- 初始状态：机器上残留一批 nebula/node/router/gateway 以及一个 vLLM 实例。
- `pkill -f` 清理大部分进程后，仍残留一个孤儿 `VLLM::EngineCore`（PID 3015412，占用约 78GB 显存，PPID=1）。
- 原因：`pkill -f 'vllm serve'` 无法匹配 vLLM 的子进程名。
- 解决：手动 `kill 3015412`（TERM），确认 `nvidia-smi` 不再显示占用。

## 3. 按顺序启动

| 步骤 | 组件 | 结果 |
|------|------|------|
| 4a | Scheduler | ✅ 输出 `placed: key=/placements/qwen2_5_0_5b ...` |
| 4b | Node | ✅ 日志确认 `registered endpoint ... base_url=http://127.0.0.1:10814` |
| 4c | vLLM ready | ✅ `GET /v1/models` 返回 200，包含 `Qwen2.5-0.5B-Instruct` |
| 4d | Router | ✅ `18081` 监听，`/healthz` 返回 `ok` |
| 4e | Gateway | ✅ `8081` 监听，`/healthz` 返回 `ok` |

### Router 启动问题

- 首次启动时遇到 `Address already in use (os error 98)`（`18081` 被残留进程占用）。
- 清理占用后再次启动，正常监听。
- **注意**：Router 绑定端口失败时几乎无输出，建议启动时加 `RUST_BACKTRACE=1`。

## 4. 端到端请求验证

### 4a. Gateway `/v1/chat/completions`（非流式）

- **结果**：✅ 返回 JSON 正常
- **响应内容**：`choices[0].message.content` = `"Hello! How can I assist you today?"`

### 4b. Gateway `/v1/chat/completions`（流式）

- **结果**：✅ SSE 流正常
- **格式**：符合 `data: {"choices":[{"delta":{"content":"..."}}]}` 形式
- **示例片段**：

```text
data: {"id":"chatcmpl-...","object":"chat.completion.chunk",...,"choices":[{"delta":{"content":"Hello"}}]}
data: {"id":"chatcmpl-...","object":"chat.completion.chunk",...,"choices":[{"delta":{"content":"!"}}]}
...
data: [DONE]
```

### 4c. Gateway `/v1/responses`（流式）

- **结果**：✅ SSE 流正常
- **事件序列**：`response.created` → `response.output_text.delta`（多次） → `response.completed`
- **示例片段**：

```text
data: {"type":"response.created", ...}
data: {"type":"response.output_text.delta", "delta":"Hello", ...}
data: {"type":"response.output_text.delta", "delta":"!", ...}
...
data: {"type":"response.completed", ...}
```

### 4d. 直接请求 Router `/v1/chat/completions`

- **结果**：✅ 返回 JSON 正常
- **对比**：与通过 Gateway 的结果一致（`"Hello! How can I assist you today?"`）
- **结论**：Gateway 确实经 Router 转发，链路完整

## 5. 测试结论

| 验证项 | 状态 |
|--------|------|
| 编译 | ✅ |
| etcd 健康 | ✅ |
| vLLM 就绪 | ✅ |
| Router `/healthz` | ✅ |
| Gateway `/healthz` | ✅ |
| Gateway 非流式 chat | ✅ |
| Gateway 流式 chat (SSE) | ✅ |
| Gateway `/v1/responses` 流式 | ✅ |
| 直接请求 Router 对比 | ✅ |

**整条链路 `Gateway(8081) → Router(18081) → vLLM(10814)` 已完全跑通，非流式和流式均正常。**

## 6. 已知问题与注意事项

1. **vLLM 孤儿进程**：`pkill -f 'vllm serve'` 无法匹配 vLLM 子进程（如 `VLLM::EngineCore`），需手动 `kill` PID。
2. **Router 端口冲突静默退出**：绑定失败时几乎无输出，需加 `RUST_BACKTRACE=1` 排查。
3. **8080 端口**：测试机器上被其他服务占用，不影响本次使用 8081 的 Gateway。
4. **Embeddings**：`/v1/embeddings` 当前返回 501 Not Implemented，待后续接入 embedding 模型。

---

## 7. Control API 鉴权基线验证

测试日期：2026-02-09

### 7.1 测试环境

- **网关端口**：`8090`
- **鉴权配置**：`NEBULA_AUTH_TOKENS="devtoken:admin,viewtoken:viewer"`
- **限流配置**：`NEBULA_AUTH_RATE_LIMIT_PER_MINUTE=5`
- **测试机**：heyi（单卡 RTX 5090）

### 7.2 测试过程记录

1) **同步代码**（本地 -> heyi）

```bash
rsync -av --delete --exclude '.git' --exclude 'target' \
        -e "ssh -J ubuntu@mynotes.fit -p 9001" \
        /Users/xinference/github/nebula/ \
        lipeng@120.136.162.13:~/nebula-auth-test
```

2) **编译组件**（heyi）

```bash
source ~/.cargo/env
cd ~/nebula-auth-test
cargo build -p nebula-gateway -p nebula-router -p nebula-scheduler -p nebula-node -p nebula-cli
```

3) **确认环境**（heyi）

```bash
nvidia-smi --query-gpu=name,memory.total,memory.used,temperature.gpu --format=csv,noheader
ss -tlnp | grep 2379
```

4) **准备请求体**（heyi）

```bash
python3 - <<'PY' > /tmp/load.json
import json
print(json.dumps({'model_name':'qwen_test','model_uid':'qwen_test','replicas':1}))
PY
```

5) **启动 Gateway 并执行鉴权/限流测试**（heyi）

```bash
cd ~/nebula-auth-test
NEBULA_AUTH_TOKENS='devtoken:admin,viewtoken:viewer' \
NEBULA_AUTH_RATE_LIMIT_PER_MINUTE=5 \
NEBULA_GATEWAY_ADDR=0.0.0.0:8090 \
NEBULA_ROUTER_URL=http://127.0.0.1:18090 \
ETCD_ENDPOINT=http://127.0.0.1:2379 \
nohup target/debug/nebula-gateway > /tmp/nebula-gateway-auth-8090.log 2>&1 &

curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8090/v1/admin/cluster/status
curl -s -o /dev/null -w '%{http_code}' -H 'Authorization: Bearer bad' \
        http://127.0.0.1:8090/v1/admin/cluster/status
curl -s -o /dev/null -w '%{http_code}' -H 'Authorization: Bearer viewtoken' \
        http://127.0.0.1:8090/v1/admin/cluster/status
curl -s -o /dev/null -w '%{http_code}' -H 'Authorization: Bearer devtoken' \
        -H 'Content-Type: application/json' -d @/tmp/load.json \
        http://127.0.0.1:8090/v1/admin/models/load

for i in 1 2 3 4 5 6; do
        curl -s -o /dev/null -w "%{http_code}" -H 'Authorization: Bearer viewtoken' \
                http://127.0.0.1:8090/v1/admin/cluster/status
        echo
done
```

6) **清理进程**（heyi）

```bash
pkill -f 'nebula-gateway.*8090' || true
```

### 7.3 验证结果

| 用例 | 预期 | 实际 |
|------|------|------|
| 无 token 访问 `/v1/admin/cluster/status` | 401 | ✅ 401 |
| 无效 token | 403 | ✅ 403 |
| viewer token 访问 status | 200 | ✅ 200 |
| admin token 执行 model load | 200 | ✅ 200 |
| viewer token 限流（第 5-6 次） | 429 | ✅ 429 |

### 7.4 备注

- 端口 8081/8082/8083 已被其他 gateway 实例占用，故使用 8090 进行验证。
- 该验证仅覆盖 Control API 鉴权与限流，不依赖模型加载成功或 vLLM 运行状态。

---

## 8. 可观测性与新接口验证
### 8.2 heyi 端到端验证（2026-02-10）

#### 验证环境
- 机器：heyi（RTX 5090）
- 代码：本地 rsync 最新
- 组件：nebula-router, nebula-gateway, nebula-cli (debug 构建)
- etcd: 127.0.0.1:2379
- 端口：router 18090, gateway 8090

#### 验证过程
1. **清理端口**：确认 18090/8090 无残留进程，必要时 kill 占用进程。
2. **启动 router**：
        - 参数：`--listen-addr 0.0.0.0:18090 --etcd-endpoint http://127.0.0.1:2379 --model-uid qwen_test`
        - 首次遇到参数拼写错误（应为 `--listen-addr`、`--etcd-endpoint`），修正后正常启动。
        - 若端口被占用，报错 `Address already in use (os error 98)`，kill 后重启成功。
        - `/metrics` 返回 200，内容符合 Prometheus 格式。
3. **启动 gateway**：
        - 环境变量：`NEBULA_AUTH_TOKENS='devtoken:admin,viewtoken:viewer'` 等
        - 启动后 `/metrics`、`/v1/admin/whoami`、`/v1/admin/metrics`、`/v1/admin/logs`、`/v1/models`、`/v1/admin/ui` 均返回 200。
4. **CLI 验证**：
        - `nebula-cli whoami` 返回 principal/role 正确
        - `nebula-cli metrics` 返回 Prometheus 文本
        - `nebula-cli logs` 返回日志 tail
5. **清理**：测试结束后 kill 所有 nebula 相关进程，环境恢复。

#### 结果总结
| 验证项 | 预期 | 实际 |
|--------|------|------|
| Router `/metrics` | 200 | ✅ 200 |
| Gateway `/metrics` | 200 | ✅ 200 |
| Admin `/v1/admin/whoami` | 200 | ✅ 200 |
| Admin `/v1/admin/metrics` | 200 | ✅ 200 |
| Admin `/v1/admin/logs` | 200 | ✅ 200 |
| `/v1/models` | 200 | ✅ 200 |
| `/v1/admin/ui` | 200 | ✅ 200 |
| CLI whoami | principal/role | ✅ 正确 |
| CLI metrics | Prometheus 文本 | ✅ 正确 |
| CLI logs | 日志 tail | ✅ 正确 |
| 端口冲突处理 | 可恢复 | ✅ kill 后正常 |
| 参数拼写修正 | 启动成功 | ✅ |
| 进程清理 | 无残留 | ✅ |

**结论：所有 observability、admin、CLI 相关新特性在 heyi 端到端验证通过。**

测试日期：2026-02-10

### 8.1 验证项

| 用例 | 预期 | 实际 |
|------|------|------|
| Gateway `/metrics` | 返回 Prometheus 文本 | ✅ | 
| Router `/metrics` | 返回 Prometheus 文本 | ✅ |
| Admin `/v1/admin/metrics` | 受鉴权保护 | ✅ |
| Admin `/v1/admin/whoami` | 返回 principal/role | ✅ |
| `/v1/models` | 返回模型列表 | ✅ |
| `/v1/admin/ui` | 页面可加载 | ✅ |
| Admin `/v1/admin/logs` | 返回日志 tail | ✅ |
