# Lingshu (灵枢)

> 🚀 高性能 AI Agent 平台 — 46 个 Rust crate 组成的企业级智能体系统

Lingshu 是一个用 Rust 构建的 AI Agent 平台，支持多通道接入、多 LLM 后端、插件热加载、
联邦通信、TEE 安全执行等企业级特性。

## 特性

- 🔌 **多通道**: Telegram / 飞书 / QQ / 微信 / Webhook
- 🤖 **多 LLM**: OpenAI / Anthropic / DeepSeek / Qwen / 智谱 / 百度 等 27+ 提供商
- 🧩 **插件系统**: WASM 热加载 + 静态插件，5 个内置插件
- 🔒 **企业安全**: TEE (SGX/TDX)、多租户隔离、审计、Vault
- 🌐 **联邦通信**: 跨实例 gRPC 联邦 + 负载均衡
- 🖥️ **WebUI**: Yew 构建的管理面板
- 📦 **桌面端**: Tauri v2 原生应用
