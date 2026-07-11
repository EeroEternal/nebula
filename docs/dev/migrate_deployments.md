# 迁移：`/model_requests/` → `/deployments/`

Sprint 3 B5 起，控制面**只写** `/models/{uid}/spec` + `/deployments/{uid}`。Scheduler 仅 watch `/deployments/`。

## 一次性迁移

```bash
nebula-cli admin migrate
# 或 POST {gateway}/v2/migrate
```

会把存量 `/model_requests/` 转成 ModelSpec + ModelDeployment（已有 spec 则跳过）。

## 写路径对照

| 旧（已停写） | 新 |
|--------------|----|
| `POST /v1/admin/models/load` → `/model_requests/{uuid}` | 同路由改为写 spec + deployment（`request_id` 兼容返回 `model_uid`） |
| `PUT .../requests/{id}/scale` | 按 `model_uid`（或遗留 request id 解析）更新 deployment |
| `DELETE .../requests/{id}` | 将 deployment 设为 `Stopped` |
| CLI `model load` / `scale --id` | 仍可用；`--id` 现优先当作 `model_uid` |
| CLI `model create/start/stop/scale-model` | v2，本来就是 deployments |

## 清理

迁移完成后可手工删除 etcd 中残留的 `/model_requests/*`。列表类 API 仍可读旧 key 做过渡展示，但不会再写入。
