# Nebula Lite

单机测试用的最小推理入口：一个二进制、一个进程对外服务。不跑 etcd / BFF / Router / Scheduler / Node，也不做多机调度。

与完整栈的关系：完整控制面仍走 `nebula-up.sh` + 声明式 Deployment；Lite 是独立产品切片，只服务「本机起一个引擎、用 OpenAI API 试一把」。

## 形态

```
Client  --OpenAI HTTP :8081-->  nebula-lite  --spawn+proxy-->  vLLM 或 SGLang 子进程
```

同一时刻只跑一个引擎实例。Ctrl+C 时先停子进程再退出。

## 范围

| 有 | 无 |
|----|----|
| 拉起本机 vLLM 或 SGLang | etcd / MetaStore / Placement |
| OpenAI 兼容反向代理（含 SSE） | BFF、Postgres、控制台 |
| `--gpus` → `CUDA_VISIBLE_DEVICES` + TP | 多 Node、Scheduler、独立 Router |
| 前台单进程、清晰启动失败报错 | 多模型并发编排、鉴权（测试默认关） |

不把现有 Gateway/Router/Scheduler/Node 抽 `run` 拼装；Lite 单独实现，只参考 Node 引擎启动参数与 Gateway 流式转发思路。

## CLI（目标用法）

```bash
cargo build -p nebula-lite --release

# vLLM（默认）
./target/release/nebula-lite \
  --model Qwen/Qwen2.5-0.5B-Instruct \
  --engine vllm \
  --gpus 0,1,2,3 \
  --port 8081

# SGLang
./target/release/nebula-lite \
  --model Qwen/Qwen2.5-0.5B-Instruct \
  --engine sglang \
  --gpus 0,1,2,3 \
  --port 8081
```

关键参数：`--model`（必填）、`--engine vllm|sglang`（默认 `vllm`）、`--gpus`（默认 `0`；卡数作为 TP）、`--port` / `--host`、`--vllm-bin`（默认 `vllm`）、`--sglang-bin`（默认 `python3 -m sglang.launch_server`）、`--max-model-len` / `--gpu-memory-utilization`（vLLM）、`--mem-fraction-static`（SGLang）、`--ready-timeout-secs`。

对外路由：`POST /v1/chat/completions`、`/v1/completions`、`/v1/embeddings`，`GET /v1/models`、`GET /healthz` → 转发到本机引擎内部端口。

## 实现

crate：`crates/nebula-lite`，二进制 `nebula-lite`。模块：`engine`（spawn / 就绪 / 进程组清理）、`proxy`（Axum 转发含 SSE）、`args` + `main`。

```bash
cargo build -p nebula-lite --release
./target/release/nebula-lite --help
```
