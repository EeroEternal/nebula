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

- **ModelProfile**：架构（dense/moe）、参数规模、量化、上下文、可选 `model_uid`
- **WorkloadHint**：并发、QPS、长短上下文偏好、workload_id
- **SelectionConstraints**：SLO、预算、允许引擎/平台、偏好（latency|throughput|cost）
- **BackendCandidate**：引擎、版本、镜像、平台、预估 SLI、`confidence`、`switching_cost`、`reasons`、证据 run ids
- **DeploymentDraft**：候选 → 未落盘的 `ModelSpec` 补丁 + `ModelDeployment`（确认后由 BFF 写入）

证据不足 → `insufficient_data`，禁止静默默认引擎。

## API（BFF）

| 方法 | 路径 | 作用 |
|------|------|------|
| `PUT/GET` | `/api/v2/model-profiles/{id}` | 可选持久化画像（etcd 最新值） |
| `POST` | `/api/v2/selection/recommend` | Top-K 候选（含切换成本） |
| `POST` | `/api/v2/selection/draft` | 由候选生成 Deployment 草稿（不写 etcd） |
| `POST` | `/api/v2/selection/apply` | Operator 确认后写 Deployment（半自动） |

流水线：硬过滤（兼容/Capability）→ 召回 PerformanceProfile → 打分 → 切换成本惩罚 → Top-K。

## 原则

无证据不自动选型；切换必声明式、可 Canary、可回滚；全自动仅开关+冷却+审计（Phase 3）。
