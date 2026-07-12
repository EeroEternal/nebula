# Loki 日志接入（stdout JSON → 采集器）

> 对齐 [`observability.md`](./observability.md) §4.4。  
> **应用不直连 Loki HTTP**；只写带关联字段的 JSON 到 stdout。

## 1. 组件侧

```bash
export NEBULA_LOG_FORMAT=json
# 可选：推 OTLP traces 到 xtrace（与日志解耦）
export OBSERVE_URL=http://127.0.0.1:8742
export OBSERVE_TOKEN=...
# OTLP 完整路径示例（若组件需要）：
#   http://127.0.0.1:8742/api/public/otel
```

`nebula_common::telemetry::init_tracing` 在 `json` 模式下输出 **flatten JSON lines**，并启用：

- W3C TraceContext 传播（Gateway → Router）
- span 字段（如 `request_id`、`service`）进入日志

**最小字段约定**（经 span / 结构化字段）：

| 字段 | 说明 |
|------|------|
| `timestamp` | tracing-subscriber JSON 默认 |
| `level` | info/warn/error |
| `target` / `fields.service` | 组件名 |
| `fields.request_id` | 控制面请求 ID |
| `span` / otel | 可选 `trace_id` 关联 |

## 2. Promtail 示例（Docker / 主机文件）

```yaml
# deploy/observe/promtail-nebula.yaml
server:
  http_listen_port: 9080
positions:
  filename: /tmp/positions.yaml
clients:
  - url: http://loki:3100/loki/api/v1/push

scrape_configs:
  - job_name: nebula
    static_configs:
      - targets: [localhost]
        labels:
          job: nebula
          __path__: /var/log/nebula/*.log
    pipeline_stages:
      - json:
          expressions:
            level: level
            request_id: fields.request_id
            service: fields.service
            message: fields.message
      - labels:
          level:
          service:
          request_id:
```

容器场景：将组件 stdout 挂到 Docker logging driver，或用 Vector 读 container logs。

## 3. Vector 示例

```toml
# deploy/observe/vector-nebula.toml
[sources.nebula_stdout]
type = "file"
include = ["/var/log/nebula/*.log"]
read_from = "beginning"

[transforms.parse_json]
type = "remap"
inputs = ["nebula_stdout"]
source = '''
. = parse_json!(.message)
.service = .fields.service ?? .target
.request_id = .fields.request_id
'''

[sinks.loki]
type = "loki"
inputs = ["parse_json"]
endpoint = "http://loki:3100"
encoding.codec = "json"
labels.job = "nebula"
labels.service = "{{ service }}"
```

## 4. LogQL 按 trace / request 互跳

```logql
{job="nebula"} | json | request_id="req_xxxxxxxx"
{job="nebula", service="nebula-router"} |= "aborted"
```

控制台排障：xtrace 看 LLM 语义 trace → 用 `request_id` / `trace_id` 在 Loki 拉日志 → Grafana 用 Prometheus `/metrics` 看运维曲线。

## 5. 明确不做

- 应用 `POST` 到 Loki API  
- xtrace → Loki 官方桥  
- 把 prompt/generation 全文当日志默认落盘（审计走专用 audit 通道）
