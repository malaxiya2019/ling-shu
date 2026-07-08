# Federation

LingShu federation enables cross-cluster agent communication and execution.

## Topologies

### Mesh
- Full mesh between all nodes
- Lowest latency
- Highest resource usage

### Hub-Spoke
- Central hub routes all traffic
- Easier management
- Scales to many nodes

## Features

- **Agent Migration**: Move agents between clusters
- **State Replication**: Sync state across nodes
- **Node Discovery**: Automatic peer discovery (DNS/gossip)
- **TLS Encryption**: Secure inter-node communication
- **Load Balancing**: Distribute agents across clusters

## Configuration

```yaml
federation:
  enabled: true
  node_id: "node-1"
  topology: "mesh"
  discovery: "dns"
  peers:
    - "node-2.cluster.local:9090"
    - "node-3.cluster.local:9090"
  tls:
    cert: "/etc/lingshu/tls/cert.pem"
    key: "/etc/lingshu/tls/key.pem"
```
