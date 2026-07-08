# TypeScript SDK

## Installation

```bash
npm install lingshu-client
```

## Usage

```typescript
import { LingShuClient } from "lingshu-client";

const client = new LingShuClient("http://localhost:8080", "sk-xxx");

// Chat completion
const resp = await client.chat({
  model: "gpt-4",
  messages: [{ role: "user", content: "Hello!" }],
});
console.log(resp.content);

// Agent execution
const agentResp = await client.runAgent({
  agentId: "my-agent",
  input: "Process this data",
});
console.log(agentResp);
```

## API Reference

| Method | Description |
|--------|-------------|
| `chat()` | Chat completion |
| `runAgent()` | Execute agent |
| `getAgentStatus()` | Check agent status |
| `runEval()` | Run evaluation suite |
| `getFederationStatus()` | Get cluster status |
| `listFederationNodes()` | List cluster nodes |
