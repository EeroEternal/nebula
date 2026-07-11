# xtrace 服务部署信息

xtrace 服务已部署在远程服务器 10.21.11.92:8742。

## 部署详情

- **二进制**：~/github/xtrace/target/release/xtrace
- **配置**：~/github/xtrace/.env
- **DATABASE_URL**：postgresql://ai:xtrace123@127.0.0.1:5432/observe
- **API_BEARER_TOKEN**：nebula-xtrace-token-2026
- **BIND_ADDR**：0.0.0.0:8742

## 启动方式

```bash
cd ~/github/xtrace && set -a && . .env && set +a && nohup ./target/release/xtrace > /tmp/xtrace.log 2>&1 &
```

## 数据库

PostgreSQL 14 安装在同一台服务器，pg_hba.conf 已配置 md5 认证给 ai 用户。

## Nebula Node 集成参数

```
--xtrace-url http://10.21.11.92:8742/ --xtrace-token nebula-xtrace-token-2026
```
