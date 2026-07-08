# Memory System

LingShu provides four memory types:

## Session Memory
- Per-session conversation history
- Configurable context window
- Automatic pruning

## Buffer Memory  
- Working memory for agent operations
- Short-term storage
- FIFO eviction policy

## Vector Memory
- Embedding-based similarity search
- Persistent storage via SQLite/PostgreSQL
- Cosine similarity retrieval

## Graph Memory
- Knowledge graph relationships
- Entity extraction and linking
- Cypher query support (Neo4j)

## Configuration

```yaml
memory:
  session:
    max_tokens: 4096
    ttl_seconds: 3600
  buffer:
    capacity: 100
  vector:
    dimensions: 1536
    similarity: "cosine"
  graph:
    url: "neo4j://localhost:7687"
```
