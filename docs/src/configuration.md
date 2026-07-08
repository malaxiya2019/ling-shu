# Configuration

LingShu supports multi-environment configuration via YAML files.

## Configuration File

Default path: `config/lingshu.yaml`

```yaml
server:
  host: "0.0.0.0"
  port: 8080
  workers: 4

llm:
  default_provider: "openai"
  providers:
    openai:
      api_key: "${OPENAI_API_KEY}"
      model: "gpt-4"
    anthropic:
      api_key: "${ANTHROPIC_API_KEY}"
      model: "claude-3-opus"

database:
  url: "sqlite:///var/lib/lingshu/lingshu.db"

observability:
  tracing:
    exporter: "otlp"
    endpoint: "http://localhost:4317"
  metrics:
    exporter: "prometheus"

security:
  jwt_secret: "${JWT_SECRET}"
  oauth2:
    providers:
      - github
      - google

federation:
  enabled: false
  topology: "mesh"
  discovery: "dns"
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LS_ENV` | Environment (dev/prod) | `dev` |
| `LS_LOG_LEVEL` | Log level | `info` |
| `LS_CONFIG_PATH` | Config file path | `config/` |
| `OPENAI_API_KEY` | OpenAI API key | - |
| `ANTHROPIC_API_KEY` | Anthropic API key | - |
| `JWT_SECRET` | JWT signing secret | - |
