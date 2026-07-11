# nebula-node 多引擎支持

已完成完整的多引擎支持，编译通过，单元测试通过。

## 架构变更

- **去掉全局 engine 单例** — main.rs 不再创建全局 Arc<dyn Engine>
- **per-assignment 引擎选择** — reconcile_model 根据 PlacementAssignment.engine_type 动态创建引擎实例
- **RunningModel 持有引擎** — 每个运行中的模型持有自己的 Arc<dyn Engine>
- **heartbeat 从 RunningModel 取引擎** — 不再接收全局 engine 参数，健康检查/指标采集/重启都通过 rm.engine

## 文件结构

- `engine/mod.rs` — Engine trait + 共享工具（stop_docker_container_by_name 提升为共享函数）+ create_engine 工厂（支持 "vllm" 和 "sglang"）
- `engine/vllm.rs` — VllmEngine 实现
- `engine/sglang.rs` — SglangEngine 实现（Docker/本地二进制两种模式、SGLang Prometheus metrics 解析）
- `args.rs` — 新增 sglang_* 系列 CLI 参数（sglang_bin/cwd/host/docker_image/model_dir/tensor_parallel_size/data_parallel_size/mem_fraction/max_running_requests）
- `reconcile.rs` — reconcile_model 签名去掉了 engine 参数，内部按 assignment.engine_type 创建
- `heartbeat.rs` — heartbeat_loop 签名去掉了 engine 参数
- `main.rs` — 去掉全局 engine，reconcile_model 调用不再传 engine

## SGLang 引擎特点

- 启动参数：--model-path, --host, --port, --tp, --dp, --mem-fraction-static, --max-running-requests, --served-model-name
- sglang_bin 支持多词命令如 "python3 -m sglang.launch_server"
- Docker 模式加 --ipc=host（SGLang 需要共享内存）
- 指标解析：sglang:num_requests_waiting, sglang:num_requests_running, sglang:token_usage (KV cache)

## 扩展方式

新增引擎只需在 engine/ 下创建新文件实现 Engine trait，在 create_engine 工厂注册，在 args.rs 加对应参数。
