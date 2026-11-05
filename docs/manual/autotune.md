# 有界自动调参

这是默认不运行的服务配置实验器，不是模型训练或无限制自我修改。给定代表性负载、候选参数、备用 GPU 和延迟目标后，它自动运行对照实验、选择候选、应用生产配置，并在验证失败或控制进程失联时恢复原配置。

## 当前范围

| 范围 | 当前行为 |
| --- | --- |
| 引擎并发与批处理 | 仅 vLLM：搜索 `max_num_seqs`、`max_num_batched_tokens` |
| KV 容量 | 已有 LMCache 部署可搜索每副本 `local_cpu_size_gb`，保留 pin 预算 |
| KV pin 时长 | 必须与基线相同，尚无工具暂停负载证据，拒绝自动改变 |
| Gateway 准入 | 不改变并发硬上限、租户配额或队列策略 |
| 工作负载 | 显式提供脱敏负载和有界候选表，不自动采集生产提示词或生成候选 |

模型权重、量化、采样、拓扑、副本数和镜像不在搜索范围。`enable_chunked_prefill` 支持配置投影，但实验期间保持不变。该闭环只证明所给负载下的暖态表现，不代表冷启动或所有用户负载。

## 按场景配置参数

先区分请求参数和部署参数：

| 参数层 | 修改入口 | 生效范围 |
| --- | --- | --- |
| `temperature`、`max_tokens` 等生成参数 | 客户端推理请求 | 单次请求，不重建引擎 |
| `max_num_seqs`、`max_num_batched_tokens`、`enable_chunked_prefill` | 模型或部署配置 | 引擎实例，变更可能触发重建 |
| Gateway 准入、租户配额、队列策略 | 对应的网关与配额配置 | 不由本调参器修改 |

`max_num_seqs` 是引擎同时处理的序列上限，不是客户端压测并发数。`max_num_batched_tokens` 是每轮调度的 token 预算，不是单次请求的输出上限，也不是模型上下文长度上限。

### 手动设置部署参数

手动设置不要求额外的实验 GPU，也不会自动运行对照实验或回滚。先用已认证的 `GET /api/v2/models/{model_uid}` 读取 `spec.config`、`deployment` 和当前副本数，保存原配置作为回退依据。再将合并后的请求体提交到 `POST /api/v2/models/{model_uid}/start`，需要 Operator 或 Admin 权限。

以下仅演示**单副本、没有其他部署覆盖项**的 vLLM 配置，不是通用最优值：

```json
{
  "replicas": 1,
  "config_overrides": {
    "max_num_seqs": 16,
    "max_num_batched_tokens": 4096,
    "enable_chunked_prefill": true
  }
}
```

现有部署必须保留实际副本数和其他部署覆盖项。`config_overrides` 是**整体替换**，不是字段级 PATCH；直接照抄示例可能丢失原有的并行度、KV 或其他设置。配置优先级为模型基础配置 → 部署覆盖 → 单副本覆盖，后者优先。若副本已有同名覆盖，只改部署层不会改变该副本的有效值。

修改模型基础配置可使用 `PUT /api/v2/models/{model_uid}` 的 `config` 字段；该字段同样整体替换，也可能被部署或单副本配置覆盖。不要依赖基础配置写入来完成运行中引擎的重建，应通过部署配置变更应用，并确认新端点就绪。生产变更应安排维护窗口，不承诺无中断切换。

### 为场景选择负载和目标

| 场景 | 代表性负载 | 实验方向 |
| --- | --- | --- |
| 交互式聊天 | 实际长度的短提示词与输出，接近实际并发 | 重点约束首 token 延迟（TTFT）和生成间隔（TPOT），比较较小的并发与批处理候选 |
| 批量生成 | 接近实际输出长度和并发的批处理请求 | 在延迟目标内比较更多并发与更大 token 预算，不能只看总吞吐 |
| 长上下文 / RAG | 脱敏长提示词，长度接近实际检索上下文 | 先手动确认上下文长度、显存和分块预填充配置，再比较批处理候选 |
| 重复上下文 / Agent | 代表性的重复前缀提示词 | 已有 LMCache 时可比较主机 KV 容量；当前不验证工具暂停场景，不调整 pin 时长 |

这些只是实验方向，不能据此断言某组数值更快。为每个场景准备自己的 `workload.json`，调整 `tuning.json` 中的 `candidates` 和三个 p95 延迟上限。`min_improvement` 指满足延迟目标的有效请求率提升，不是单纯吞吐提升。保持所有候选的负载、并发和采样设置一致；需要比较不同客户端并发时，分别运行独立轮次。

若多个场景需要长期使用不同引擎参数，应创建不同生产模型 UID 的部署，由客户端在推理请求的 `model` 字段中明确选择对应 UID。它们可以引用同一模型来源，但需要各自的运行资源；不要依赖相同模型名称的公共别名来区分部署。目前不会自动识别请求场景或逐请求切换引擎配置。

## 启动条件

- 生产部署运行中，固定副本数，并在 `replica_specs` 显式固定每个副本的节点及 GPU；显式设置两个批处理参数，副本无单独配置或镜像覆盖。
- 使用不可变的原始 `image@sha256:…` 镜像引用；镜像注册 ID 暂不接受。镜像必须确实提供声明的 vLLM 版本。
- 提供同型号、同显存容量、同平台的独占备用 GPU，每副本 GPU 数与生产相同；节点心跳新鲜，GPU 已用显存必须为零。
- LMCache 实验需要运维确认**实际独立**的远端存储，不能只是同一后端的另一个 DNS 名；不支持本地磁盘层实验。为隔离主机内存，实验节点不能与本模型生产节点或其他活动实验共用。候选必须通过 pin 预算和新鲜的主机可用内存检查。
- BFF 凭据和 Gateway 推理凭据都需要 Operator 或 Admin 权限。`at-` 加 32 位十六进制字符是保留实验 UID，即使 placement 先于 spec 同步也不注册公共别名；实验不进入公开模型列表，也不接受普通用户流量。
- 配置好现有 Scheduler、Node 和执行环境。调参器不启动或升级引擎基础设施，也不提供真实 GPU 性能保证。

## 使用

准备 `tuning.json`，其中的节点名是部署者实际提供的备用资源：

```json
{
  "engine_version": "0.23.0",
  "candidates": [
    {"max_num_seqs": 32, "max_num_batched_tokens": 4096},
    {"max_num_seqs": 64, "max_num_batched_tokens": 8192}
  ],
  "replica_specs": [{"node_id": "spare-node", "gpu_indices": [0]}],
  "objective": {
    "ttft_p95_ms_max": 500,
    "tpot_p95_ms_max": 50,
    "e2e_p95_ms_max": 10000,
    "min_improvement": 0.05,
    "max_regression": 0.05,
    "min_requests": 32
  },
  "observation_seconds": 120,
  "max_experiment_seconds": 7200
}
```

LMCache 候选还必须带基线 `pin_timeout_sec` 和候选 `local_cpu_size_gb`，并在配置顶层提供 `isolated_remote_url`。角色从生产放置继承，不改变 Prefill/Decode 分工。

准备 `workload.json`，以下是用于演示格式的脱敏短问答负载；正式实验应替换为自己的代表性提示词和输出上限：

```json
{
  "id": "scenario-short-chat-v1",
  "class": "custom",
  "prompt": "用两句话解释什么是 KV cache。",
  "max_tokens": 128,
  "concurrency": 16,
  "request_count": 32
}
```

自动调参命令中的 `--concurrency` 和 `--requests` 会覆盖文件中的对应值；`--requests` 不得小于目标中的 `min_requests`。更多负载格式见[压测器说明](../../scripts/benchmark/README.md)和[压测协议](../dev/benchmark.md)。此调参命令目前使用负载文件的闭环请求，不暴露压测器的 JSONL trace 或开环到达参数。

凭据分别从 `NEBULA_BFF_TOKEN` 和 `NEBULA_GATEWAY_TOKEN` 环境变量读取，勿写进配置或命令参数：

```bash
bin/nebula-autotune \
  --model production-model-uid --config tuning.json \
  --workload-file workload.json \
  --bff-url http://127.0.0.1:18090 \
  --gateway-url http://127.0.0.1:18080/v1 \
  --concurrency 16 --requests 32 --artifacts-dir ./logs/autotune
```

默认只运行一轮。`--cycles 0 --interval 600` 持续重复，每轮重新读取生产基线，冷却至少 300 秒。单轮最长默认 2 小时，可配置 300–86400 秒；最多 16 个候选、128 次测量，全局最多 16 个活动实验。

## 如何决定与回退

1. 创建独立实验 UID，不抢占生产名称别名。
2. 对每个候选按 **基线、候选、候选、基线（ABBA）** 测量。每次先完整回放一次负载预热，再记录同一请求计划的流式 TTFT、TPOT、端到端延迟和有效请求率。
3. 只接受同负载、种子、并发、请求数、版本、镜像和硬件声明的真实证据；任何失败、样本不足或超过延迟目标的测量都不能胜出。候选两次最差有效请求率必须超过基线两次最好值乘以 `1 + min_improvement`。
4. 胜出后才更新生产部署。**应用会触发现有引擎配置重建机制，未承诺无中断滚动升级。** 随后会在生产 UID 上额外回放相同负载，至少两次并持续至观察时间结束。请把这部分合成流量纳入生产容量预算。`max_regression` 是观察期有效请求率相对实验基线的最大允许下降比例，不是延迟变化比例；延迟仍由三个明确的 SLO 上限约束。
5. 验证失败或 120 秒租约过期时恢复原部署。写入使用更短的服务器侧许可，先原子撤销许可，再判定旧 CAS 是否已提交，避免迟到请求重新应用候选。恢复后等待生产端点就绪；停止实验部署后，还要等待放置和端点消失，并看到停止写入之后的新鲜节点清单确认实验 GPU 已用显存为零，才释放预留。停机失败或节点失联时保留预留，`cleanup_wait_reason` 显示等待原因。

候选预热失败会被跳过；基线预热失败会终止本轮。原始计时样本与 manifest 留在指定目录，不保存响应文本。**manifest 中仍包含显式提供的 workload 提示词**，请只使用脱敏数据，并控制目录及 BFF 历史的访问权限。

## 故障与状态

程序输出实验 ID，可用已认证的 `GET /api/v2/autotune/{id}` 查询阶段、原因、租约、测量及 `last_reconciled_at_ms`。写操作为 Operator 级，提交到同一路径：

```json
{"owner":"创建时的 UUID","epoch":1,"operation":{"action":"rollback"}}
```

实验历史、控制租约和待应用意图在 PostgreSQL；短期写许可位于 etcd `/autotune/permits/` 并带服务器 TTL，不存历史。部署仍走 etcd CAS，由原 Scheduler 生成放置。多个 BFF 通过行锁和可重放意图恢复中途失败，不引入第二个放置写入者。人工修改使旧实验失去写权限，不会被覆盖；若人工接管了实验部署，其新所有者需自行停止它。

若卡在 `restoring` 或 `cleaning`，检查 Scheduler、Node、引擎启动与端点租约，不要直接删数据库记录来解除预留。数据库或 etcd 无法访问时，自动恢复只能等服务恢复，不能保证在网络隔离期间完成。结束后保留停止的实验 spec/deployment 供检查，不自动删除历史对象。

实现已验证数据库恢复、重复应用、并发控制器和配置冲突契约；模拟流不证明真实引擎收益。生产启用前仍需在自己的 GPU、镜像和负载上实测。
