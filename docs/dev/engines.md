# 多引擎对齐（vLLM / SGLang）

> 日期：2026-09-21。定位：让 **node（裸机）** 与 **k8s-controller（集群）** 两条执行面对多引擎行为一致。

## 1. 目标

Nebula 引擎无关。两条执行面都要能按 `ModelSpec.engine_type` 起 **vLLM / SGLang**，并把能力、指标、健康统一到同一控制面。

## 2. 实现

- **`nebula_common::engine_metrics`**：`parse_vllm_metrics_text` / `parse_sglang_metrics_text` + **`parse_engine_metrics(engine_type, …)`** 分派；node 复用（不再各自一份实现）。
- **`nebula-k8s-controller`**：`create_engine_pod(engine_type)` 选镜像与命令：
  - **vLLM**：`vllm/vllm-openai:latest`（`vllm serve` 入口，`--model /models/weights …`）；
  - **SGLang**：`lmsysorg/sglang:latest`，`python3 -m sglang.launch_server --model-path /models/weights --served-model-name … --host 0.0.0.0 --port 43537 --trust-remote-code --enable-metrics`；
  - `EndpointInfo.engine_type` 取 `spec.engine_type`；`/stats/` 走 `parse_engine_metrics` 分派。

## 3. 真机验证（2026-09-21，8×5090）

以 `engine_type = sglang` 起 Pod：

| 项 | 结果 |
|----|------|
| Pod 镜像 | `lmsysorg/sglang:latest` ✅ |
| endpoint `/endpoints/qwen2.5-7b-nebula/0` | `engine_type: "sglang"`、`status: ready`、**持续刷新** ✅ |
| `/stats/qwen2.5-7b-nebula/0` | 由 **SGLang 解析器**产出（`kv_cache_usage` ← `token_usage`；无 `prefix_cache_hit_rate`）✅ |
| 端到端 | `curl <gateway>/v1/chat/completions` → SGLang 正常回答 ✅ |

## 4. 真机踩到的坑（已修）

1. **SGLang 默认绑 `127.0.0.1`**（vLLM 是 `0.0.0.0`）→ pod IP 不可达 → 健康探测失败 → endpoint 不注册。**必须 `--host 0.0.0.0`**。
2. **SGLang `/health` 响应 >800ms**（`curl -m 0.8` 5/5 超时，`-m 5` 才 200）→ controller 原 **800ms** 就绪探测失败 → endpoint 靠 TTL 过期、永不刷新。**已调到 3s**。
3. SGLang `/metrics` 需 **`--enable-metrics`**，否则 404（`/stats/` 抓不到）。

## 5. 参考

- 能力/契约：[`contracts.md`](contracts.md)（C4 Capability 三态、C6「引擎差在 scrape」）。
- 执行面边界：[`k8s.md`](k8s.md)。

## 6. replica_specs 放置对齐（C1，2026-09-21）

真机（8×5090，vLLM）验证 `ModelDeployment.replica_specs`：

| 项 | 结果 |
|----|------|
| `replica_specs[i].gpu_indices` | ✅ replica0 → GPU **1**、replica1 → GPU **3**（`NVIDIA_VISIBLE_DEVICES`） |
| `replica_specs[i].node_id` | ✅ 指向其它节点的 replica **被跳过**（本节点 agent 不创建/不重建），多节点不会重复建 pod |
| endpoint | ✅ `node_id: "xgateway"`、`engine_type: "vllm"`、`status: ready` |

修复：`f130285` —— controller 原先忽略 `replica_specs[].node_id`、把所有副本都建在自己的 `--gpu-node` 上；多节点各跑一个 agent 时会重复建 pod。现按 `node_id`（缺省 = 本节点）过滤。

## 7. PD 分离传输选型与引擎版本钉死（Batch 3 第一步，2026-10-11）

> 决策约束：**通用性优先**——NVIDIA 与昇腾（及其他后端）都要能跑；NIXL 出局（Dynamo 系方案）；p2p 连接器为 `p2p_nccl`（NVIDIA 专属）同样出局。以下为 rig 真机对两类部署镜像的实证。

### 7.1 双镜像 connector 实测矩阵

| 能力 | `vllm/vllm-openai:v0.23.0`（CUDA） | `quay.io/ascend/vllm-ascend:v0.23.0rc1-a3`（昇腾，arm64） |
|---|---|---|
| kv-events（zmq，P1c 依赖） | ✅ publisher `null`/`zmq` | ✅ **代码同构**（同上游 v0.23，9 处 zmq 引用） |
| lmcache connector（v1） | ✅ 且 **`lmcache` 包内置** | ✅ 代码在；`lmcache` 包需 pip 安装（纯 Python，arm64 可装） |
| mooncake connector（v1） | ✅ 且 `mooncake` 包内置 | ✅ 代码在；包需安装；TCP backend 可用（无 RDMA 降级跑） |
| offloading / simple_cpu_offload | ✅（分层卸载，非 PD 传输） | ✅ |
| p2p | `p2p_nccl`——NVIDIA 专属 | 同名文件 → **昇腾不可用** |
| nixl / hf3fs / flexkv / moriio | ✅/需基础设施 | ✅/需基础设施（按决策排除或特定环境） |

两镜像的 `kv_connector/v1` 模块集合**逐模块一致**（同一上游 + 后端插件差异）——vLLM 的 connector 抽象本身是硬件无关的，差异只在 pip 依赖与传输后端。

### 7.2 决策

1. **PD 传输主路径 = LMCache**。理由：vLLM 官方 disagg 示例即 lmcache（`examples/disaggregated/lmcache/`，双镜像都带）；传输在 torch tensor 层（D2H → TCP/本地/远端），**不绑传输库与 GPU 厂商**；与 P1c 同源的 block-hash 语义（前缀定位与 KV 对象定位一套键）。
2. **排除**：NIXL（用户决策，Dynamo 系）、Mooncake（用户决策，收敛到 LMCache 单路径）、p2p_nccl（NVIDIA 专属）、hf3fs/flexkv/moriio（特定基础设施）。
3. **引擎版本钉死：vLLM v0.23.x**。CUDA 用 `vllm/vllm-openai:v0.23.0`；昇腾用 `vllm-ascend v0.23.0rc1+`。理由：kv-events 线上格式已在 v0.23 上字节级校准（§5.2 的 map-form/int-hash 学费），connector 集合双硬件同构。升级版本前必须重跑 prefix-affinity §5 的校准流程。

### 7.3 待验证（PD 实现的前置）

- [ ] 昇腾真机（arm64 + NPU）：`pip install lmcache` + LMCacheConnector PD e2e——rig 的昇腾镜像是 arm64 无法在 x86 主机执行，需昇腾机器。
- [x] CUDA 侧最小 PD 验证（2026-10-11，见 §7.4）：角色语义确认（producer/consumer + `lm://` 共享 KV 存储），lmcache 0.5.5 跑通。
- [x] nebula 侧设计：`replica_specs` 角色字段（prefill/decode/混合）+ connector 参数进 ModelDeployment（见 [`../arch/pool.md`](../arch/pool.md) §pd_split）。

### 7.4 CUDA 侧 LMCache TCP PD 实测（2026-10-11，8×5090）

按官方 `disagg_prefill_lmcache_v1` 示例（v0.23.0 镜像，lmcache 0.4.6），GPU0=producer / GPU1=consumer + disagg_proxy：

- ✅ **引擎可启动**：`enable_nixl: False` 时 LMCache StorageManager 强制要求 `LocalCPUBackend` 分配器（官方示例默认 nixl 才关了 local_cpu）——**配置修正**：`local_cpu: True` + `max_local_cpu_size: 4`，两个角色均正常起服。LMCache 的 RPC/lookup 层本身是 **zmq**（`lmcache/v1/rpc_utils.py`），纯 TCP。
- ❌ **首请求 consumer EngineCore 崩溃**：`request_finished → assert self.lmcache_engine is not None`（`lmcache/integration/vllm/vllm_v1_adapter.py:1783`）。init 无报错、但请求期引擎句柄为空——疑似 0.4.6 实验性 v1 disagg（需 `LMCACHE_USE_EXPERIMENTAL=True`）在无 NIXL 路径的 lifecycle bug，或缺少未在示例中说明的 lookup 辅助服务。
- **升级 lmcache 0.5.5 后反转**（同配置仅 `pip install -U lmcache`）：
  - ✅ **disagg 编排全链路走通**：proxy + producer(GPU0) + consumer(GPU1)，HTTP 200、输出正确、暖态 **0.68s** 完整响应（prefill TTFT 46.8ms）；0.4.6 的崩溃修复。
  - ❌ **KV 实际未跨传**：consumer 侧 `LMCache hit tokens: 0, need to load: 0` + producer 侧 `Controller message sender is not initialized`——decoder 在重算前缀（正确性无恙，传输未发生）。根因（源码级）：`lmcache_rpc_port` 构造的是 **IPC（unix socket）路径组件**（`{base}/engine_{id}_service_lookup_lmcache_rpc_port_{p}`），producer 的 **lookup server** 未被拉起/接线——属 LMCache disagg v1 的文档化配置项（其仓库 examples），非 nebula 或代码缺陷。
- **版本钉死更新**：vLLM v0.23.x + **lmcache ≥ 0.5.5**（0.4.6 无 NIXL 路径崩溃）。
- **✅ 线已接通（同日，`lm://` 远端 KV 存储形态）**：完整拓扑 = `lmcache_server 0.0.0.0 6800 cpu`（LMCache 自带远端 KV 存储服务，纯 TCP）+ 两个引擎 `remote_url: lm://localhost:6800`。实测（681-token prompt）：
  - prefiller(GPU0) `Stored 681/681, 0.0364 GB, 0.93 GB/s` → 远端；
  - decoder(GPU1) `hit 512, Retrieved 512/512, 0.0273 GB, 139ms(0.197 GB/s)` ← 远端，只重算尾部；
  - **纯 TCP、零 NIXL、跨引擎**。此前的 hit=0 根因：短 prompt（26 tok）不满一个 chunk（chunk_size=256），decoder 标准路径不计命中——测试须用 ≥chunk 的 prompt。
- **形态结论**：无 NIXL 时 LMCache 的正确 PD 形态是"**共享 KV 存储**"（prefill 池写 `lm://`、decode 池读），而非官方 proxy+kv_consumer 流程（那条路的直传通道在 0.5.5 只有 nixl）。这恰好更贴 nebula：路由与角色编排由 nebula 自己做（P1c 索引 + 角色化放置），不依赖上游 proxy。
- **升级门禁**：`scripts/check_lmcache_compat.sh`（quick 无 GPU ~1min / `--full` 双 GPU 端到端；真机两模式均 8/8 通过）。改 `LMCACHE_PIN`/`VLLM_IMAGE` 前必须先跑它。容器内到 pypi.org 很慢的机器（国内网络实测默认 bridge ~80 KB/s，`pip install lmcache` 会卡在 resolve）用 `PIP_INDEX_URL=https://mirrors.huaweicloud.com/repository/pypi/simple ALLOW_RUNTIME_PIP=1 ./scripts/check_lmcache_compat.sh`，镜像地址会透传进探测容器（2026-10-23 实测 8/8）。
- **Nebula 编排参数（Batch 3 实现输入）**：prefill 副本 = `kv_role: kv_producer` + store 远端；decode 副本 = `kv_role: kv_both` + `retrieve_locations: [RemoteBackend]`；共享 `PYTHONHASHSEED`（chunk key 一致）；`lmcache_server` 为部署新增组件（TCP，硬件无关，昇腾可用）。
- **Nebula 侧无需 Rust 重写**：LMCache 是引擎进程内的 KV 面，nebula 的边界仍是"控制面 + 路由"（启动参数/角色/放置 + P1c 索引），KV 字节永远不经过 nebula。重写 LMCache 于 Rust = 重写引擎侧组件，违反引擎零侵入原则，无路由收益。
