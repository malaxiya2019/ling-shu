# Building from Source

## Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Protobuf compiler (`protoc`)
- OpenSSL dev libraries
- Make

## Build Commands

```bash
# Debug build
cargo build

# Release build
cargo build --release

# All features
cargo build --all-features

# WASM WebUI
cd webui && trunk build --release

# Docker image
docker build -t lingshu:latest .
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `federation` | Cross-cluster federation |
| `llm-router` | Multi-LLM routing |
| `mcp` | MCP protocol support |
| `full` | All features enabled |
