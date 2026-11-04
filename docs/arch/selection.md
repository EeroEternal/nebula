# L3 智能选择层

> Phase 1 分册。总纲见 [`vision.md`](./vision.md)；证据面仍用 P5 Benchmark/Canary；排期见 [`roadmap.md`](./roadmap.md)。

## 定位

在约束下给出可解释的后端方案（引擎/镜像/拓扑草稿），**默认半自动**：控制台确认后再写 `/deployments/`。不进 Gateway/Router 热路径；不直接改 `/endpoints/`。

## 与 P5 关系

| 能力 | 归属 |
|------|------|
| Benchmark runs / profiles、Canary | P5 证据与发布 |
| ModelProfile、打分、切换成本、Deployment 草稿 | **本层（Selection）** |
| `POST /benchmarks/recommend` | 保留兼容；新集成走 `/selection/*` |

## 数据模型（v0）

- **ModelProfile**：架构（dense/moe）、参数规模、权重量化、上下文、可选 `model_uid`；另含 `sliding_window_tokens`、`linear_attention_layer_ratio`、`kv_lora_rank`。后三项仍驱动经验 KV 估算，尚非实测标定（#88 未完成部分）。
- **WorkloadHint**：并发、QPS、长短上下文偏好、workload_id
- **SelectionConstraints**：SLO、预算、允许引擎/平台、偏好（latency|throughput|cost）
- **BackendCandidate**：引擎、版本、镜像、平台、预估 SLI、`confidence`、`switching_cost`、`score`、`score_breakdown`、`reasons`、证据 run ids
- **DeploymentDraft**：候选 → 未落盘的 `ModelSpec` 补丁 + `ModelDeployment`（确认后由 BFF 写入）

证据不足 → `insufficient_data`，禁止静默默认引擎。

## 打分与可解释

排序分数由偏好决定，并对 `switching_cost` 施加惩罚：

- **latency**：`switch_penalty = switching_cost * 10`，分数为 `-(ttft_p95 + switch_penalty)`（越高越好）。
- **throughput**：`switch_penalty = switching_cost * 10`，分数为 `throughput_tps - switch_penalty`。
- **cost**：`switch_penalty = cost_per_1k * switching_cost * 0.1`，分数为 `-(cost_per_1k + switch_penalty)`。`switching_cost` 为 0–1 的启发式值，因此切换惩罚不超过候选成本的 10%，与主指标量纲一致；该比例是排序策略，不是实测迁移费用。缺失成本仍按最差分数处理，不以零成本替代。

每个候选写回 `score` 与 `score_breakdown`（如 `preference=latency`、`ttft_p95=…`、`switch_penalty=…`、`score=…`）；`reasons` 含 preference 与 switching 说明。成本、切换惩罚与最终分数的明细保留浮点精度，不将小额成本或惩罚舍入成零。

### Roofline 声明门禁

只消费候选证据 run 中的 `evidence.engine_capability`，复用现有 `EngineCapability`：
`quantization` 声明该次运行的活动格式（`w4a4/w4a16/fp8/w8a8/int8`），
`native_low_precision_paths` 按格式声明 `supported/unsupported/unknown`，缺项为未知。
通用引擎静态表不声明硬件低精度能力；当前无自动探针，不把手工声明称为实测能力。

候选的所有 run 必须真实成功、完整、声明一致，且模型、完整 profile key、引擎版本、
平台和镜像作用域一致。召回阶段逐条对照所属画像的完整 key；覆盖同一 `run_id`
导致 workload/GPU/fingerprint 变化时先重建旧画像，移除跨配置引用，再计算性能与声明。
缺少版本/镜像/平台、声明缺失或冲突时，修正为零。
不从芯片名、镜像名、`ModelProfile.quantization` 或理由文案推断原生能力。
采集入口见 [`压测证据协议`](../dev/benchmark.md) 和 runner 的 `--capability-file`。

以下保留为**未经真机校准的排序启发式**，不是百分比收益或容量准入依据：

- 活动格式为 W4A4 且对应原生通路明确 supported：+15。
- 活动 W4A16 原生通路 supported，并发 1–4：+15。
- 活动 FP8/W8A8/INT8 原生通路 supported，并发 ≥16：+15。
- 活动 W4A16 原生通路 supported，W4A4 **明确 unsupported**，并发 ≥16：−5；
  仅缺少 W4A4 声明不能作为惩罚依据。
- Cost 偏好始终不叠加这些常数，仅保留有声明依据的解释。

## `current` 自动填充

`POST /selection/recommend`（及内部先 recommend 的 draft）：若请求 `current` 为空且 `model.model_uid` 有值，BFF 从 `/models/{uid}/spec` 与 `/deployments/{uid}` 填 `engine_type`、`image_id`（部署 `image_id` 优先，否则 spec `docker_image`），以便计算切换成本。客户端显式传入的 `current` 不覆盖。

## API（BFF）

| 方法 | 路径 | 作用 |
|------|------|------|
| `PUT/GET` | `/api/v2/model-profiles/{id}` | 可选持久化画像（etcd 最新值） |
| `POST` | `/api/v2/selection/recommend` | Top-K 候选（含切换成本与打分明细） |
| `POST` | `/api/v2/selection/draft` | 由候选生成 Deployment 草稿（不写 etcd） |
| `POST` | `/api/v2/selection/apply` | Operator 确认后写 Deployment（半自动） |

流水线：硬过滤（兼容/Capability）→ 召回 PerformanceProfile → 打分 → 切换成本惩罚 → Top-K。

## 原则

无证据不自动选型；切换必声明式、可 Canary、可回滚；全自动仅开关+冷却+审计（Phase 3）。
