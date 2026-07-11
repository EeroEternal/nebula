# Nebula 远端登录、部署与测试（Memory）

本文记录当前可用的一套标准流程：
- 登录服务器
- 从本机同步代码并部署
- 使用统一配置 `deploy/nebula.env` 启动
- 验证 Gateway / BFF / Audit Logs

## 1) 登录服务器

目标机器：`10.21.11.92`，用户名：`ai`

```bash
ssh ai@10.21.11.92
```

快速确认：

```bash
hostname
whoami
pwd
```

## 2) 本机同步代码到远端（不走 clone）

在本机执行：

```bash
rsync -az --delete \
  --exclude '.git' \
  --exclude 'target' \
  --exclude 'node_modules' \
  --exclude 'frontend/node_modules' \
  /Users/xinference/github/nebula/ \
  ai@10.21.11.92:/home/ai/github/nebula/
```

## 3) 远端准备统一配置（nebula.env）

在远端执行：

```bash
cd ~/github/nebula
mkdir -p deploy
cp -f deploy/nebula.env.example deploy/nebula.env
```

把 xtrace token 写入 `deploy/nebula.env`：

```bash
OBSERVE_TOKEN=$(grep -E '^API_BEARER_TOKEN=' ~/github/xtrace/.env | head -n1 | cut -d= -f2-)

cat > ~/github/nebula/deploy/nebula.env <<EOF
ETCD_ENDPOINT=http://127.0.0.1:2379
ROUTER_PORT=18081
GATEWAY_PORT=8081
BFF_PORT=18090
NODE_PORT=10824
NODE_ID=node_gpu0
MODEL_UID=qwen2_5_0_5b
MODEL_NAME=Qwen/Qwen2.5-0.5B-Instruct
START_BFF=1
BFF_DATABASE_URL=postgresql://<nebula_user>:<nebula_pass>@127.0.0.1:5432/nebula
OBSERVE_URL=http://127.0.0.1:8742
OBSERVE_AUTH_MODE=service
OBSERVE_TOKEN=$OBSERVE_TOKEN
EOF

chmod 600 ~/github/nebula/deploy/nebula.env
```

注意：BFF 的 `BFF_DATABASE_URL` 必须是 Nebula 独立数据库（`/nebula`）；xtrace/observe 使用独立数据库（`/observe`）。

## 4) 远端编译与重启

> 如未初始化 Rust 环境，先：`. "$HOME/.cargo/env"`

```bash
cd ~/github/nebula
. "$HOME/.cargo/env"

cargo build --release \
  -p nebula-router \
  -p nebula-scheduler \
  -p nebula-bff \
  -p nebula-gateway \
  -p nebula-node

./bin/nebula-down.sh || true
./bin/nebula-up.sh
```

## 5) 验证测试

### 5.1 健康检查

```bash
curl -sS -i http://127.0.0.1:8081/healthz | sed -n '1,8p'
curl -sS -i http://127.0.0.1:18090/api/healthz | sed -n '1,8p'
```

期望：都返回 `200 OK`。

### 5.2 Audit Logs

```bash
curl -sS -i "http://127.0.0.1:18090/api/audit-logs?page=1&limit=1" \
  -H "Authorization: Bearer devtoken" | sed -n '1,28p'
```

期望：`HTTP/1.1 200 OK` 且 body 包含 `data`。

## 6) 常见问题

### A. `cargo: command not found`

```bash
. "$HOME/.cargo/env"
```

### B. `Could not find protoc`

需要安装：

```bash
sudo apt-get update && sudo apt-get install -y protobuf-compiler
protoc --version
```

### C. `pkill ... Operation not permitted`

`nebula-down.sh` 某些 PID 可能无权限清理。通常可先忽略，继续 `up` 并以健康检查为准；如服务异常再针对对应 PID 处理。

### D. `{"message":"Unauthorized"}`（Audit Logs）

这是高频问题，通常是 `OBSERVE_TOKEN` 缺失或未生效：

```bash
# 1) 对齐 token
TOKEN=$(grep -E '^API_BEARER_TOKEN=' ~/github/xtrace/.env | head -n1 | cut -d= -f2-)
sed -i "s|^OBSERVE_TOKEN=.*$|OBSERVE_TOKEN=${TOKEN}|" ~/github/nebula/deploy/nebula.env

# 2) 重启
cd ~/github/nebula
./bin/nebula-down.sh || true
START_BFF=1 ./bin/nebula-up.sh

# 3) 验证
curl -sS -i "http://127.0.0.1:18090/api/audit-logs?limit=1" \
  -H "Authorization: Bearer devtoken" | sed -n '1,20p'
```

期望返回 `HTTP/1.1 200 OK`。
