# 压测证据协议

实现入口：[`run_benchmark.py`](../../scripts/benchmark/run_benchmark.py)；
命令与回放格式见其 [`README`](../../scripts/benchmark/README.md)。
本协议只测客户端可见行为，不涉及引擎 IR、kernel 或请求内 KV 接力。

## 测量定义

| 字段 | 口径 |
|---|---|
| TTFT | 从客户端计划到达时刻，到第一个有生成内容的 SSE delta；不把 role-only、usage 或响应头当首 token |
| TPOT | 从首个到最后一个生成 delta 的时间，除以真实 `completion_tokens - 1`；少于两个 token 时未知 |
| E2E | 从计划到达到收到正常 `[DONE]`；包含客户端调度迟滞与网络时间 |
| throughput | 成功响应的真实输出 token 总数 ÷ 整轮实际 wall time；不是请求耗时之和 |
| goodput | 满足声明 TTFT/TPOT/E2E 门限的请求数或输出 token 数 ÷ 同一 wall time；未声明门限时未知 |
| 每卡吞吐 | throughput ÷ 显式声明的 GPU 数；未知卡数不假设为 1 |
| 相对成本 | wall time × GPU 数 × 1000 ÷ 成功输出 token 数，单位为 GPU 秒/千 token，不是货币 |
| 错误率 | 所有计划请求中的失败比例，含开环发生器的 `client_overloaded` |

引擎可能一次 SSE delta 返回多个 token，所以 TPOT 是客户端可见的平均生成间隔，
不是逐 token 的引擎调度直方图。usage/DONE 尾帧不参与 TPOT；真实 usage 缺失时，
不得用字符数、chunk 数或 `max_tokens` 推算 token 数。任一成功响应缺少 usage，
整轮 throughput 与成本保持未知。

正常结束必须同时观察到 `finish_reason` 和 `[DONE]`。异常 EOF、错误帧、无效 JSON、
超时和 HTTP 错误均是失败，不补造正常完成。多地址连接尝试、TLS 握手和网络读写
共享绝对截止时刻，每次连接或底层读取（包括响应头）重新计算剩余预算，慢速发送
头部不能反复刷新超时。系统 DNS
解析不受 Python socket 超时中断；需要 DNS 硬截止时应使用 IP 或有界系统解析器。
SSE event 有 1 MiB 上限；请求生成器最多接受 100000 条请求，计划 hash 逐条增量
计算，不将重复 prompt 的整份 JSON 数组一次性复制进内存。

## 负载与可比性

- 闭环：真实执行 workload 并发数，空出一个并发槽才发送下一条。
- 开环：按常量速率或固定 seed 的泊松过程到达；并发槽满时记录客户端拒绝，不把
  后续请求排成无界队列。TTFT/E2E 包含实际发送相对计划时刻的调度迟滞。
- 回放：JSONL 显式指定相对到达时刻、prompt 与 token 上限；seed、temperature、
  规范化 trace 的 SHA-256 保存在证据中。原 trace 由操作者安全保存。
- 比较域：profile fingerprint 包含引擎参数、镜像/软件标签、trace、seed、并发、到达方式、
  到达速率、GPU 数、缓存状态与 SLO 门限，避免不同负载被聚成同一画像。
- ABBA：四次运行标记为 A/B/B/A，保存比较组、轮次、位置；端点切换由操作者完成。
  冷/暖状态须分开报告，`--cache-state` 仅记录声明，不自动预热或清空缓存。

## 持久化与选型边界

每轮保存 `samples.jsonl` 与 `manifest.json`，拒绝覆盖既有文件。样本只记录计时、
usage、结束原因和错误分类，不保存生成正文、认证 header 或 token。manifest
包含镜像/软件版本标签、采样设置、负载信息、样本 hash 与可选缓存计数器快照。
指定的计数器按 endpoint 所有标签求和；缺采样或计数器重置时，delta 为未知，
不是零。未传镜像或版本时仍保留未知，不声称具备完整生产复现实验环境。

`BenchmarkRun.evidence.schema_version = 2` 标识新协议。BFF 将摘要与扩展证据字段
原样保存在现有 Postgres JSONB 中，无新增表；原始文件需要另行归档，BFF 不上传。

- dry-run：不联网，返回 `insufficient_data` 与 `synthetic=true`，不生成虚假性能值；
  与 `--ingest` 同时使用时直接拒绝。
- 真实请求任一失败：运行标为 `failed`，保留失败率和可计算的观测数据。
- 无有效输出/usage：标为 `insufficient_data`，不提供乐观选型证据。
- 只有真实成功运行才能进入画像。旧数据缺少 evidence 字段时保留兼容，但明确带
  `dry-run` / `dry_run` 标记的历史数据被排除。
- 推荐时会重算包含已知无效样本的旧画像，防止只过滤成本而仍沿用合成 TTFT/吞吐。

## 验证

协议测试覆盖真实并发、开环拒绝、固定 seed/trace、UTF-8 与 SSE 分片、正常/异常
完成、usage 缺失与零值、wall-time 分母、SLO goodput、GPU 数未知、证据 hash、
不覆盖文件和 dry-run 隔离；已接入 `scripts/ci.sh`。Rust 测试覆盖 JSONB 往返所需的
序列化保真、历史合成数据排除与旧画像重算。

mock 只能证明测量和失败契约，不能证明推理性能收益。GPU 并发扫描也只验证压测器
能观测真实引擎，不等于路由优化 A/B 或 #90 的多副本 `pd_split` 验收。
