# 生产 etcd 三节点（运维手册）

> **给谁看：** 要把控制面从单节点 etcd 切到 3 节点多数派的实施/运维。旁路演练结论见 [`../dev/ha/report.md`](../dev/ha/report.md)。  
> **不做：** 本文不替你在生产机上直接改集群；不写内网 IP / 密钥。开发默认仍单节点即可。

## 是什么、能干什么

Nebula 控制面把声明式状态放在 etcd。单节点挂了，Scheduler/Router/Gateway/Node/BFF 的 list/watch 都会断。三节点多数派：任挂一台，读写与选主仍可用。

客户端已支持逗号分隔多地址：`ETCD_ENDPOINT=http://etcd1:2379,http://etcd2:2379,http://etcd3:2379`（见 `deploy/nebula.env.example`）。

## 何时该切

- 生产控制面要扛单机维护 / 单节点宕机；或 SLA 要求元数据面不断。
- 旁路已验证「杀 1/3 成员后仍可读写」不够：必须把 **生产 keyspace** 迁到 3 节点，且所有 Nebula 进程用同一串多 endpoint。

开发机、实验室单机：继续单节点，不必跟本文。

## 拓扑约定

三台主机（或三容器）各跑一个 etcd v3.5+：

| 角色 | 端口（惯例） |
|------|----------------|
| client | 2379 |
| peer | 2380 |

成员名与 `initial-cluster` 必须稳定；生产用固定 DNS 或静态主机名，勿用会漂移的容器临时名。客户端 URL 对 Nebula 暴露，peer URL 仅集群内部。

## 新建空集群（推荐先旁路）

三节点同时带同一 token 与 `initial-cluster` 启动（示意，主机名自行替换）：

```bash
etcd \
  --name etcd1 \
  --data-dir /var/lib/etcd \
  --listen-client-urls http://0.0.0.0:2379 \
  --advertise-client-urls http://etcd1:2379 \
  --listen-peer-urls http://0.0.0.0:2380 \
  --initial-advertise-peer-urls http://etcd1:2380 \
  --initial-cluster etcd1=http://etcd1:2380,etcd2=http://etcd2:2380,etcd3=http://etcd3:2380 \
  --initial-cluster-token nebula-etcd \
  --initial-cluster-state new
```

`etcd2` / `etcd3` 仅改 `--name` 与对应 advertise URL。健康：

```bash
etcdctl endpoint health --endpoints=http://etcd1:2379,http://etcd2:2379,http://etcd3:2379
etcdctl member list --write-out=table
```

预期：三成员都是 started；至少两节点 healthy。

## 把 Nebula 指到多 endpoint

所有控制面进程（gateway / router / scheduler / node / bff）使用同一配置：

```bash
ETCD_ENDPOINT=http://etcd1:2379,http://etcd2:2379,http://etcd3:2379
```

或 CLI：`--etcd-endpoint` 传入同一逗号串。改完滚动重启；确认无进程仍指向旧单节点。

## 从生产单节点迁到三节点（要点）

1. **旁路演练**：空 3 节点上验证 member list / 杀一台仍读写（不要动生产数据）。
2. **停写窗口**：短暂停 Scheduler / Node 写路径（或整体维护窗），避免迁移中途写丢。
3. **备份**：对单节点 `etcdctl snapshot save`；保留快照文件与校验哈希。
4. **恢复到新集群**：按 etcd 官方流程用快照初始化 **新** 三节点（或先单成员 restore 再 `member add`）；确认关键 key 前缀可读（`/deployments/`、`/placements/`、`/endpoints/`、`/tenants/` 等）。
5. **切客户端**：所有 Nebula 进程改为多 `ETCD_ENDPOINT` 并重启；旧单节点只读观察一段时间后下线。
6. **冒烟**：Gateway `/healthz`、Scheduler 唯一 leader、推理一条、BFF 读模型列表。

迁移细节以 [etcd disaster recovery](https://etcd.io/docs/latest/op-guide/recovery/) 为准；Nebula 不另造备份工具。

## 故障演练（切完后必做）

| 动作 | 预期 |
|------|------|
| 停 1 个 etcd 成员 | `endpoint health` 仍多数 healthy；Nebula list/watch 不中断 |
| 恢复该成员 | 自动追数据；member list 仍为 3 |
| 同时停 2 个成员 | 集群不可用（预期）；只验证监控告警，勿当验收通过条件 |

接入层（Gateway/Router 多副本、Scheduler 选主）演练仍按 [`module.md`](./module.md)「高可用」与 [`../dev/ha/report.md`](../dev/ha/report.md)。

## 出问题怎么办

| 现象 | 处理 |
|------|------|
| Nebula 连不上 etcd | 检查 `ETCD_ENDPOINT` 是否含全部成员、防火墙、证书/明文是否与集群一致 |
| 只有一个 endpoint healthy | 看 peer 网络与磁盘；`member list` 是否有 learner/未 started |
| 切流后 Placement 异常 | 确认维护窗内无双写；必要时以 etcd 为准对账，清孤儿见 [`../dev/etcd.md`](../dev/etcd.md) |
| 误以为「旁路 3 节点」= 生产 HA | 生产 keyspace 未迁、客户端仍单地址时，**不算**完成 |

## 与文档边界

- 键该不该进 etcd：[`../dev/etcd.md`](../dev/etcd.md)
- HA 真机报告（旁路 PASS、生产未切）：[`../dev/ha/report.md`](../dev/ha/report.md)
- 排期：[`../arch/roadmap.md`](../arch/roadmap.md) — runbook ✅；生产切入仍按维护窗执行
