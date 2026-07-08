# Kubernetes Deployment (Helm)

## Install

```bash
helm repo add lingshu https://charts.lingshu.dev
helm install lingshu lingshu/lingshu
```

## Configuration

```yaml
replicaCount: 3
image:
  repository: lingshu/lingshu
  tag: latest
service:
  type: ClusterIP
  port: 8080
resources:
  limits:
    cpu: "4"
    memory: "8Gi"
  requests:
    cpu: "1"
    memory: "2Gi"
federation:
  enabled: true
  replicas: 3
```

## Monitoring

See `helm/lingshu/templates/grafana-dashboard.yaml` for the built-in Grafana dashboard.
