# Quick Start

## 前置条件

- Rust 1.75+
- Docker & Docker Compose (可选)
- Make

## 快速启动

### 1. 克隆仓库

```bash
git clone https://github.com/ling-shu/lingshu.git
cd lingshu
```

### 2. 编译

```bash
cargo build --release
```

### 3. 启动服务

```bash
# 使用默认配置启动
make serve
```

### 4. 验证

```bash
curl http://localhost:8080/health
```

### 5. 使用 Python SDK

```bash
pip install lingshu-client
```

```python
from lingshu import LingShuClient, ChatRequest, ChatMessage

client = LingShuClient("http://localhost:8080")
resp = client.chat(ChatRequest(
    model="gpt-4",
    messages=[ChatMessage(role="user", content="Hello!")]
))
print(resp.content)
```

### Docker 部署

```bash
make docker-build
make docker-up
```
