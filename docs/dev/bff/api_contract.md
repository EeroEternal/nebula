# BFF API Contract (Draft)

## 1. Overview

- Base URL: `http://<bff-host>:<port>`
- Versioning: prefix all endpoints with `/api`.
- Auth: `Authorization: Bearer <token>` (BFF-managed token).
- Content-Type: `application/json` for requests and responses unless noted.

## 2. Error Schema

All errors return JSON with a consistent envelope.

```json
{
  "error": {
    "code": "invalid_request",
    "message": "human readable message",
    "request_id": "req_123",
    "details": {
      "field": "model_uid"
    }
  }
}
```

- `code`: stable error identifier (string).
- `message`: user-friendly error message.
- `request_id`: server-generated correlation id.
- `details`: optional object for structured info.

## 3. Auth and Roles

- Tokens map to BFF roles: `admin`, `operator`, `viewer`.
- BFF enforces role checks, independent of gateway.

## 4. Endpoints

### 4.1 Health

`GET /api/healthz`

Response:

```json
{ "status": "ok" }
```

### 4.2 Who Am I

`GET /api/whoami`

Response:

```json
{
  "principal": "user@example.com",
  "role": "admin"
}
```

### 4.3 Overview (Aggregated)

`GET /api/overview`

Response:

```json
{
  "nodes": [
    {
      "node_id": "node-1",
      "last_heartbeat_ms": 1700000000000,
      "gpus": [
        { "index": 0, "memory_total_mb": 24576, "memory_used_mb": 1024 }
      ]
    }
  ],
  "endpoints": [
    {
      "model_uid": "qwen2_5_0_5b",
      "replica_id": 0,
      "plan_version": 3,
      "node_id": "node-1",
      "endpoint_kind": "native_http",
      "api_flavor": "openai",
      "status": "ready",
      "last_heartbeat_ms": 1700000000000,
      "grpc_target": null,
      "base_url": "http://127.0.0.1:10814"
    }
  ],
  "placements": [
    {
      "model_uid": "qwen2_5_0_5b",
      "version": 3,
      "assignments": [
        {
          "replica_id": 0,
          "node_id": "node-1",
          "engine_config_path": "/tmp/engine.yaml",
          "port": 10814,
          "gpu_index": 0,
          "extra_args": ["--max-model-len", "4096"]
        }
      ]
    }
  ],
  "model_requests": [
    {
      "id": "req_abc",
      "request": {
        "model_name": "Qwen/Qwen2.5-0.5B-Instruct",
        "model_uid": "qwen2_5_0_5b",
        "replicas": 1,
        "config": {
          "tensor_parallel_size": 1,
          "gpu_memory_utilization": 0.9,
          "max_model_len": 4096,
          "required_vram_mb": 12000,
          "lora_modules": []
        }
      },
      "status": "Pending",
      "created_at_ms": 1700000000000
    }
  ]
}
```

### 4.4 List Model Requests

`GET /api/requests`

Response: list of `ModelRequest`.

### 4.5 Load Model

`POST /api/models/load`

Request:

```json
{
  "model_name": "Qwen/Qwen2.5-0.5B-Instruct",
  "model_uid": "qwen2_5_0_5b",
  "replicas": 1,
  "config": {
    "tensor_parallel_size": 1,
    "gpu_memory_utilization": 0.9,
    "max_model_len": 4096
  }
}
```

Response:

```json
{
  "request_id": "req_abc",
  "status": "pending"
}
```

### 4.6 Unload Model (Cancel Request)

`DELETE /api/models/requests/:id`

Response:

```json
{
  "status": "unloading_triggered"
}
```

### 4.7 Scale Model Replicas

`PUT /api/models/requests/:id/scale`

Request:

```json
{
  "replicas": 3
}
```

Response:

```json
{
  "request_id": "req_abc",
  "old_replicas": 1,
  "new_replicas": 3
}
```

Role: `operator`+. Scheduler reconcile loop will automatically adjust endpoint count.

### 4.8 Drain Endpoint

`POST /api/endpoints/drain`

Request:

```json
{
  "model_uid": "qwen2_5_0_5b",
  "replica_id": 0
}
```

Response:

```json
{
  "model_uid": "qwen2_5_0_5b",
  "replica_id": 0,
  "status": "draining"
}
```

Role: `operator`+. Sets endpoint status to `Draining`; Router stops routing new requests to it. Already-draining endpoints return `{"status": "already_draining"}`.

### 4.9 Metrics (Read-only)

`GET /api/metrics`

- Returns raw Prometheus text or a JSON summary (choose one and document).

### 4.10 Logs (Read-only)

`GET /api/logs?lines=200`

Status: pending (log source to be decided; centralized logging preferred).

Response:

```json
{
  "lines": [
    "2026-02-10T12:00:00Z INFO ...",
    "2026-02-10T12:00:01Z WARN ..."
  ]
}
```

## 5. BFF Data Sources (No Gateway Dependency)

- etcd:
  - `/nodes/{node_id}/status`
  - `/endpoints/{model_uid}/{replica_id}`
  - `/placements/{model_uid}`
  - `/model_requests/{request_id}`
- router:
  - `/healthz`, `/metrics`
- node/scheduler:
  - no HTTP endpoints yet (optional future additions)

## 6. Notes

- BFF writes to `/model_requests` to trigger scheduler flow.
- Role enforcement happens at BFF; downstream services remain unchanged.
