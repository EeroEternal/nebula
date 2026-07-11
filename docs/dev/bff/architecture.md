# BFF Components and Data Flow

## 1. Component View

```mermaid
flowchart LR
    UI[UI] -->|HTTP API| BFF[BFF Service]
    CLI[CLI] -->|HTTP API| BFF

    BFF -->|read/write| ETCD[(etcd)]
    BFF -->|metrics/health| Router[Router]
    BFF -->|optional admin ops| Node[Node]
    BFF -->|optional metrics| Scheduler[Scheduler]

    Router -->|route| Engine[Engine]
    Scheduler -->|placements| ETCD
    Node -->|watch/reconcile| ETCD
```

## 2. Data Flow (Load Model)

```mermaid
sequenceDiagram
    autonumber
    participant UI as UI/CLI
    participant BFF as BFF
    participant ETCD as etcd
    participant SCH as Scheduler
    participant NODE as Node

    UI->>BFF: POST /api/models/load
    BFF->>ETCD: put /model_requests/{id}
    SCH-->>ETCD: watch /model_requests
    SCH->>ETCD: put /placements/{model_uid}
    NODE-->>ETCD: watch /placements
    NODE->>ETCD: put /endpoints/{model_uid}/{replica_id}
    BFF-->>UI: 200 {request_id, status}
```

## 3. Data Flow (Overview Read)

```mermaid
sequenceDiagram
    autonumber
    participant UI as UI/CLI
    participant BFF as BFF
    participant ETCD as etcd
    participant RT as Router

    UI->>BFF: GET /api/overview
    BFF->>ETCD: list /nodes, /endpoints, /placements, /model_requests
    BFF->>RT: GET /healthz (optional)
    BFF-->>UI: aggregated payload
```

## 4. Notes

- BFF does not call gateway. Gateway remains unchanged.
- Node/Scheduler HTTP admin endpoints are optional and can be added later if needed.
- Logs and metrics aggregation can be integrated via external systems (Prometheus/Loki) without changing gateway.
