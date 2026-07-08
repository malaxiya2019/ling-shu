# Monitoring

## Grafana Dashboard

LingShu ships with a built-in Grafana dashboard (14 panels) covering:

- **CPU Usage** — Per-pod CPU utilization
- **Memory Usage** — Memory consumption
- **Request Rate** — RPS per endpoint
- **Latency** — P50/P95/P99 response times
- **Token Usage** — Input/output token counts per model
- **Active Agents** — Concurrent agent executions
- **Error Rate** — 4xx/5xx error percentages
- **Federation** — Inter-cluster traffic and latency
- **LLM Router** — Routing decisions per strategy

## Metrics Endpoint

```bash
curl http://localhost:8080/metrics
```

## Prometheus Configuration

```yaml
scrape_configs:
  - job_name: "lingshu"
    static_configs:
      - targets: ["localhost:8080"]
    metrics_path: "/metrics"
```

## Logging

- Structured JSON logs
- Loki integration for log aggregation
- Configurable log levels per module
