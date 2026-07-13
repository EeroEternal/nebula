# 日志收集（Loki 等）

Nebula 各组件把日志打成 **JSON 行** 写到标准输出；由公司现有的 **Promtail、Vector** 等工具收走，再存进 Loki 或 ELK。**程序本身不会直接连 Loki。**

背景见 [`observability.md`](./observability.md)。

---

## 第一步：让组件输出 JSON 日志

```bash
export NEBULA_LOG_FORMAT=json
```

建议生产环境对所有 Nebula 进程统一设置。日志里会带 **时间、级别、组件名、请求编号**，方便和控制台里的某次调用对应。

---

## 第二步：用采集器送进 Loki

仓库里提供了示例配置（给实施工程师）：

- `deploy/observe/promtail-nebula.yaml`  
- `deploy/observe/vector-nebula.toml`  

容器部署时，通常读各容器的标准输出即可，不必改 Nebula 代码。

---

## 怎么查某次请求

在 Loki 里按 **request_id** 搜索，例如：

```logql
{job="nebula"} | json | request_id="req_xxxxxxxx"
```

排障路径建议：控制台看 trace → 复制 request_id → 到 Loki 看详细日志 → 需要曲线时去 Grafana。

---

## 不会默认做的事

- 不把完整 prompt/回答正文打进普通日志（敏感内容走审计通道）  
- 不要求 Nebula 直连 Loki 服务器  

Promtail / Vector 配置片段见下文（实施参考）。

---

## 实施参考：Promtail 片段

```yaml
# deploy/observe/promtail-nebula.yaml
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

完整文件见仓库 `deploy/observe/` 目录。
