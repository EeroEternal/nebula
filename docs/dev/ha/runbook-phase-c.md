# HA Phase C Runbook — 接入与 etcd 多副本

> 对齐 [`ha_roadmap.md`](./ha_roadmap.md) Phase C/D（N1 主体已完成；生产 etcd 三节点暂缓）。  
> Compose 拓扑：仓库根目录 `docker-compose.ha.yml`。

## 1. 目标拓扑

| 层 | 组件 | 副本 |
|----|------|------|
| 元数据 | etcd | 3 |
| 接入 | gateway / bff / router | 各 2 + Caddy LB |
| 调度 | scheduler | 2（etcd election，follower `/healthz`=503） |
| 会话 | postgres | 1（HA 可后置） |
| 执行 | node + engine | GPU 主机上部署，不在本 compose |

对外端口：`8081` gateway，`18090` bff，`18081` router，`2379` etcd1。

## 2. 启动

```bash
# 可选：observe / token
export OBSERVE_AUTH_MODE=internal

docker compose -f docker-compose.ha.yml up -d --build
docker compose -f docker-compose.ha.yml ps
```

连接串约定：`ETCD_ENDPOINT=http://etcd1:2379,http://etcd2:2379,http://etcd3:2379`  
（各组件已支持逗号分隔多 endpoint。）

健康探针：

```bash
curl -sS http://127.0.0.1:8081/healthz
curl -sS http://127.0.0.1:18090/api/healthz
curl -sS http://127.0.0.1:18081/healthz
# scheduler：仅 leader 200
docker compose -f docker-compose.ha.yml exec scheduler-a wget -q -O- http://127.0.0.1:18082/healthz || true
docker compose -f docker-compose.ha.yml exec scheduler-b wget -q -O- http://127.0.0.1:18082/healthz || true
```

## 3. 演练清单（Phase D）

记录到 `docs/dev/ha/report-YYYYMMDD.md`（可复制下方模板）。

### 3.1 杀接入副本

```bash
docker compose -f docker-compose.ha.yml stop gateway-a
# 持续 curl LB：应仍 200
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8081/healthz
docker compose -f docker-compose.ha.yml start gateway-a
```

对 `bff-a` / `router-a` 同理。

### 3.2 杀单 etcd 节点

```bash
docker compose -f docker-compose.ha.yml stop etcd3
# 控制面读写仍可用（多数存活）
curl -sS http://127.0.0.1:18090/api/healthz
docker compose -f docker-compose.ha.yml start etcd3
```

### 3.3 杀 scheduler leader

```bash
# 找出 /healthz=200 的实例后 stop
docker compose -f docker-compose.ha.yml stop scheduler-a
# 另一实例应变 leader（< 目标窗口，见 ha_roadmap）
sleep 15
docker compose -f docker-compose.ha.yml exec scheduler-b wget -q -O- http://127.0.0.1:18082/healthz
# 可选：复活旧主，确认 fencing（旧主不得写 placement）
docker compose -f docker-compose.ha.yml start scheduler-a
```

### 3.4 下线 GPU 节点（真机）

在 GPU 主机停 `nebula-node`；仅该节点上的 replica 受影响；scheduler 应重调度（若 desired 允许）。

## 4. 验收（Definition of Done 摘要）

- 任一单接入副本故障，对外 API 仍可用  
- 杀 1/3 etcd，控制面仍可用  
- scheduler leader 切换后旧主 fencing 生效  
- 故障期间成功率 / RTO 可观测  

## 5. 报告模板

见 [`report-template.md`](./report-template.md)。

## 6. 不做

- 为 HA 重写架构  
- 用户态写入 etcd  
- 跳过 election / fencing 单测（仍由 `cargo test --workspace` 覆盖）
