# Docker Deployment

## Quick Start

```bash
docker compose up -d
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `OPENAI_API_KEY` | OpenAI API key |
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `JWT_SECRET` | JWT signing secret |
| `LS_LOG_LEVEL` | Log level (default: info) |

## Docker Compose

See `docker-compose.yml` for the full configuration.

## Building Custom Image

```bash
docker build -t lingshu:custom .
docker run -d -p 8080:8080 \
  -e OPENAI_API_KEY=sk-xxx \
  -v /path/to/config:/etc/lingshu/config \
  lingshu:custom
```
