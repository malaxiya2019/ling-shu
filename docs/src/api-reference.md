# API Reference

## REST Endpoints

### Health

```http
GET /health
```

Response: `{"status": "ok", "version": "3.0.0"}`

### Chat Completions

```http
POST /v1/chat/completions
```

Request body:
```json
{
  "model": "gpt-4",
  "messages": [{"role": "user", "content": "Hello"}],
  "temperature": 0.7,
  "stream": false
}
```

### Agent Operations

```http
POST /agents/{agent_id}/run
GET  /agents/{agent_id}/status
```

### Evaluation

```http
POST /eval/run
```

### Federation

```http
GET /federation/status
GET /federation/nodes
```

### Metrics

```http
GET /metrics
```

Prometheus format metrics endpoint.
