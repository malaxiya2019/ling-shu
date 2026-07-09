# 开发指南

## Workspace 结构

```
ling-shu/
├── app/          # HTTP API 服务器
├── core/         # 核心类型
├── traits/       # 公共 trait 定义
├── channel/      # 消息通道
├── backends/     # LLM 后端
├── plugins/      # 插件目录
├── webui/        # Yew WASM 面板
├── desktop/      # Tauri 桌面端
└── docs/         # 文档站 (mdBook)
```

## 编译

```bash
# 完整编译
cargo build --release

# 仅核心
cargo build -p lingshu

# 单个插件
cargo build -p lingshu-scheduler

# WebUI
cd webui && trunk build --release
```

## 测试

```bash
cargo test --all
cargo clippy --all
```
