# Evaluation Framework

LingShu's evaluation framework provides automated testing for agents.

## Test Suite

```yaml
name: "agent-benchmark"
categories: ["reasoning", "coding"]
tests:
  - name: "math-reasoning"
    input: "Solve: 2x + 5 = 13"
    expected: "x = 4"
    weight: 1.0
    timeout_ms: 30000
```

## Scoring Types

| Type | Description |
|------|-------------|
| ExactMatch | Exact string comparison |
| Contains | Response contains expected text |
| Regex | Regex pattern matching |
| Semantic | Embedding similarity |
| CodeExec | Code execution test |
| Duration | Response time check |
| Composite | Weighted combination |

## Running

```bash
# Via CLI
cargo run -- eval --suite agent-benchmark

# Via API
curl -X POST http://localhost:8080/eval/run \
  -d '{"suite_name": "agent-benchmark"}'
```
