# 快速开始

## 环境要求

- Rust 1.75+ (推荐 1.80+)
- protobuf-compiler (proto)
- SQLite (可选，默认内置)
- OpenSSL (可选)

## 一键启动

```bash
git clone https://github.com/malaxiya2019/ling-shu.git
cd ling-shu
./start.sh
```

## 配置

复制 `.env.example` 到 `.env`：

```bash
cp .env.example .env
# 编辑 .env 填入你的 API Key
```

## 运行模式

```bash
# 快速启动 (跳过环境检查)
./start.sh --quick

# 中国网络优化
./start.sh --china

# 环境诊断
./start.sh --doctor

# 更新到最新版
./start.sh --update
```

## 验证

```bash
curl http://localhost:8080/v1/health
# {"status":"ok","version":"3.5.0","uptime":123}
```
