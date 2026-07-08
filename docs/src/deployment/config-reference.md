# Configuration Reference

## Full Configuration

```yaml
# ── Server ──
server:
  host: "0.0.0.0"
  port: 8080
  workers: 4
  max_body_size: 10485760

# ── TLS ──
tls:
  enabled: false
  cert_path: ""
  key_path: ""

# ── LLM Providers ──
llm:
  default_provider: "openai"
  timeout_ms: 30000
  retry:
    max_attempts: 3
    base_delay_ms: 1000

# ── Database ──
database:
  url: "sqlite:///var/lib/lingshu/lingshu.db"
  pool_size: 10
  migration: true

# ── Memory ──
memory:
  vector:
    url: "http://localhost:8000"
  graph:
    url: "neo4j://localhost:7687"

# ── Observability ──
observability:
  tracing:
    exporter: "otlp"
  metrics:
    exporter: "prometheus"
  health:
    enabled: true

# ── Federation ──
federation:
  enabled: false
  port: 9090
  gossip_interval: 5s
  migrator:
    timeout_ms: 30000

# ── Rate Limit ──
ratelimit:
  strategy: "token_bucket"
  tokens_per_second: 100
  bucket_size: 200

# ── Billing ──
billing:
  enabled: false
  currency: "USD"
  price_per_token: 0.000002
```
