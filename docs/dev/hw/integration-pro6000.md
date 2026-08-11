# Integration I0–I6 真机验证 — pro6000

> 日期：2026-08-12。主机 `pro6000`（8× NVIDIA RTX PRO 6000 Blackwell）。  
> 代码 `e37a751`（main，含 I6 `/platform/v1` 与 Legacy Admin 移除）。  
> 原始日志：`~/nebula/logs/integration-verify/`（不入库）。

## 拓扑

控制面：etcd（单节点）+ gateway / router / scheduler / node / bff。  
推理引擎：vLLM `qwen15_moe_vllm`（GPU 0,1 :10826）+ SGLang `qwen15_moe_sglang`（GPU 6,7 :10825），模型 Qwen1.5-MoE-A2.7B-Chat。

Gateway：`NEBULA_AUTH_DISABLED=0`，Postgres API Key（`dev-token`）；Platform 读写在 `/platform/v1`。

## 验证方式

```bash
cd ~/nebula
LOG_DIR=~/nebula/logs/integration-verify \
TOKEN=dev-token GATEWAY=http://127.0.0.1:8081 \
CHAT_MODEL=qwen15_moe_vllm \
./scripts/integration_platform_verify.sh
```

脚本覆盖：Platform 读 API、I6 Legacy 404、scale+Operation+幂等、canaries、chat+`x-nebula-request-id`、replicas/drain。

## 结果

| 场景 | 结果 |
|------|------|
| `GET /platform/v1/health/summary` | PASS |
| `GET /platform/v1/whoami` | PASS |
| `GET /platform/v1/cluster/status` | PASS |
| `GET /platform/v1/nodes` | PASS |
| `GET /platform/v1/models` | PASS |
| `GET /platform/v1/models/{uid}/replicas` | PASS |
| `GET /v1/admin/whoami` → 404（I6） | PASS |
| `POST …/deployment/scale` → 202 + Operation succeeded | PASS |
| Idempotency-Key 重放 → 202 同一 operation | PASS |
| `GET /platform/v1/canaries` | PASS |
| `POST /v1/chat/completions`（vLLM） | PASS |
| 响应头回显 `x-nebula-request-id` | PASS |
| `POST /platform/v1/replicas/drain` | PASS |

**汇总：14 PASS / 0 FAIL**（2026-08-12 重跑）。

## 备注

首次跑失败 2 项（chat + request-id）：scale 触发 SGLang 副本 reconcile，chat 仍用 `qwen15_moe_sglang` 时尚未重新注册 `/endpoints/`。脚本已改为 **chat 默认 `CHAT_MODEL=qwen15_moe_vllm`**，**drain 置于末尾**，并在 chat 前等待 ready replica。

SGLang 在 scale 后需数十秒启动；单独 curl `qwen15_moe_vllm` 可稳定走通 Gateway→Router→引擎热路径。

## 结论

pro6000 上 **Integration I0–I6**（Platform API、Operation、Legacy 移除、治理读、drain）与 **推理热路径** 均已通过；可作为 v1.6 真机签收依据。
