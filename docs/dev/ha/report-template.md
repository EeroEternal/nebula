# HA 演练报告 — YYYY-MM-DD

| 字段 | 值 |
|------|-----|
| 环境 | docker-compose.ha.yml / 真机 |
| 执行人 | |
| 开始/结束 | |
| 镜像/commit | |

## 场景结果

| 场景 | 结果 | 成功率 | RTO | 备注 |
|------|------|--------|-----|------|
| 杀 gateway 副本 | PASS/FAIL | | | |
| 杀 bff 副本 | PASS/FAIL | | | |
| 杀 router 副本 | PASS/FAIL | | | |
| 杀 etcd 单节点 | PASS/FAIL | | | |
| 杀 scheduler leader | PASS/FAIL | | | |
| 旧主复活 fencing | PASS/FAIL | | | |
| 下线 GPU 节点 | PASS/FAIL | | | |

## 观察指标

- gateway/router 5xx 与 abort/drain 口径  
- scheduler leader 切换时间  
- etcd 延迟 / 是否出现 CAS 冲突尖峰  

## 问题与跟进

1. …

## 结论

是否满足 [`ha_roadmap.md`](./ha_roadmap.md) Definition of Done：是 / 否
