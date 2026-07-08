# Python SDK

## Installation

```bash
pip install lingshu-client
```

## Usage

### Synchronous Client

```python
from lingshu import LingShuClient, ChatRequest, ChatMessage

client = LingShuClient("http://localhost:8080", api_key="sk-xxx")

# Chat completion
resp = client.chat(ChatRequest(
    model="gpt-4",
    messages=[ChatMessage(role="user", content="Hello!")]
))
print(resp.content)
```

### Async Client

```python
import asyncio
from lingshu import AsyncLingShuClient, ChatRequest, ChatMessage

async def main():
    async with AsyncLingShuClient("http://localhost:8080") as client:
        resp = await client.chat(ChatRequest(
            model="gpt-4",
            messages=[ChatMessage(role="user", content="Hello!")]
        ))
        print(resp.content)

asyncio.run(main())
```

## API Reference

| Method | Description |
|--------|-------------|
| `chat()` | Chat completion |
| `run_agent()` | Execute agent |
| `get_agent_status()` | Check agent status |
| `run_eval()` | Run evaluation suite |
| `get_federation_status()` | Get cluster status |
| `list_federation_nodes()` | List cluster nodes |
