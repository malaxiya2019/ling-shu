# 快速开始

## 环境要求

- Rust 1.81+（推荐 1.88+）
- 内存：编译最低 1GB，运行最低 256MB
- 存储：编译 ~5GB，运行 ~200MB
- 系统：Linux / Termux / macOS（Windows 可通过 WSL）

## 安装方式

### 方式一：一键脚本（推荐）

```bash
git clone https://github.com/malaxiya2019/ling-shu.git
cd ling-shu
./start.sh
```

首次运行会自动：
1. 检查 Rust / 系统依赖
2. 选择 LLM 提供商并输入 API Key
3. 编译并启动服务

### 方式二：Docker

```bash
docker pull ghcr.io/malaxiya2019/ling-shu:latest
docker run -p 8080:8080 -e OPENAI_API_KEY=sk-... ghcr.io/malaxiya2019/ling-shu
```

### 方式三：预编译二进制

从 [Releases](https://github.com/malaxiya2019/ling-shu/releases) 下载对应平台的压缩包。

## 配置

```bash
cp .env.example .env
# 编辑 .env 填入你的 API Key
```

核心配置项：

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `LS_ENV` | `dev` | 运行环境: `dev` / `test` / `prod` |
| `LLM_PROVIDER` | `openai` | LLM 提供商 |
| `OPENAI_API_KEY` | — | OpenAI API Key |
| `DEEPSEEK_API_KEY` | — | DeepSeek API Key |
| `LS_LOG_LEVEL` | `debug`(dev) / `info`(prod) | 日志级别 |

## 启动模式

```bash
# 快速启动 (跳过环境检查)
./start.sh --quick

# 生产模式
./start.sh --env prod --addr 0.0.0.0:8080

# REPL 交互模式（直接对话）
./start.sh --repl

# 国内网络优化
./start.sh --china

# 集成 OpenClaw 通道网关
./start.sh --with-openclaw

# 集成 OmniVoice 语音引擎
./start.sh --with-omnivoice
```

## 验证

启动后访问：

- **健康检查**: `http://localhost:8080/health`
- **API 文档**: `http://localhost:8080/docs`
- **管理面板**: `http://localhost:8080/admin`
- **指标**: `http://localhost:8080/metrics`

```bash
# 测试 API
curl http://localhost:8080/health
curl http://localhost:8080/version

# 测试聊天
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-3.5-turbo","messages":[{"role":"user","content":"你好"}]}'
```

## 接下来

- 📖 [架构概览](architecture.md) — 了解系统设计
- ⚙️ [配置指南](configuration.md) — 完整配置说明
- 🚀 [部署指南](deployment.md) — Docker/K8s 部署
