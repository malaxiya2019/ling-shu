# 部署指南

LingShu 支持多种部署方式，根据使用场景选择合适的方式。

## Docker 部署（推荐）

### 单节点运行

```bash
docker pull ghcr.io/malaxiya2019/ling-shu:latest
docker run -d --name lingshu \
  -p 8080:8080 \
  -p 50051:50051 \
  --env-file .env \
  ghcr.io/malaxiya2019/ling-shu:latest
```

### Docker Compose（完整环境）

```bash
# 基础版（LingShu + SQLite）
docker compose up

# 完整版（LingShu + PostgreSQL + Redis + Prometheus）
docker compose --profile full up

# 集群版（多节点联邦）
docker compose --profile cluster up
```

## Kubernetes (Helm)

```bash
# 安装
helm repo add lingshu https://charts.lingshu.dev
helm install lingshu lingshu/lingshu

# 自定义配置
helm install lingshu lingshu/lingshu \
  --set env.LLM_PROVIDER=openai \
  --set env.OPENAI_API_KEY=sk-... \
  --set replicaCount=3

# 生产部署（带 HPA 自动扩缩容）
helm install lingshu lingshu/lingshu \
  -f helm/lingshu/values.yaml \
  --set autoscaling.enabled=true \
  --set autoscaling.minReplicas=3 \
  --set autoscaling.maxReplicas=10
```

### Helm Chart 包含

- `deployment-server.yaml` — API 服务部署
- `deployment-worker.yaml` — 后台 Worker 部署
- `hpa.yaml` — 自动扩缩容
- `ingress.yaml` — Ingress 控制器
- `configmap.yaml` — 配置映射
- `secrets.yaml` — 密钥管理
- `pvc.yaml` — 持久化卷
- `servicemonitor.yaml` — Prometheus 监控
- `service.yaml` — 服务暴露

## Termux (Android)

```bash
# 一键安装
bash <(curl -fsSL https://raw.githubusercontent.com/malaxiya2019/ling-shu/main/scripts/install.sh)

# 或手动
git clone https://github.com/malaxiya2019/ling-shu.git
cd ling-shu
./start.sh
```

## 生产环境建议

### 资源规划

| 规模 | CPU | 内存 | 存储 | 并发用户 |
|------|-----|------|------|---------|
| 开发 | 1 核 | 512MB | 1GB | 1-5 |
| 小型 | 2 核 | 2GB | 10GB | 10-50 |
| 中型 | 4 核 | 8GB | 50GB | 50-200 |
| 大型 | 8+ 核 | 32GB+ | 200GB+ | 200+ |

### 监控告警

- **Prometheus**: `/metrics` 端点提供标准指标
- **Grafana**: 内置 WebUI Dashboard
- **告警规则**: Helm Chart 包含 AlertManager 规则

### 高可用

- 水平扩展：基于 HPA 自动扩缩容
- 联邦集群：跨集群 Agent 迁移
- 自动恢复：崩溃自动重启和状态恢复
