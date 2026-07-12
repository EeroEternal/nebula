# HA 文档

| 文档 | 用途 |
|------|------|
| [ha_roadmap.md](./ha_roadmap.md) | 执行清单与 DoD |
| [runbook-phase-c.md](./runbook-phase-c.md) | compose 启停与演练步骤 |
| [report-20260711.md](./report-20260711.md) | **真机 Phase D 报告**（8×5090） |
| [report-template.md](./report-template.md) | 后续报告模板 |
| [optimization.md](../../arch/optimization.md) | 全项目下一步（含 N1 收尾） |
| [architecture.md](../../arch/architecture.md) | 架构原则 |

约定：`report-*.md` 放演练报告；`runbook-*.md` 放应急手册。  
仓库：`docker-compose.ha.yml`、`deploy/ha/Caddyfile.*`、`scripts/phase_d_ha_drill.sh`。

**状态：** 接入/调度 HA 真机 PASS；生产 etcd 三节点**暂缓**（旁路已验证）。
