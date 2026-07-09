# 🔍 lingshu GitHub 模块搜索完整报告

> 搜索方法: `gh search repos` (已认证) + GitHub API
> 搜索时间: 2026-07-09
> 有效搜索数: ~70 个查询

---

## 🧠 LLM / AI 推理层

### 已发现的高质量项目

| 项目 | ⭐ | 语言 | 说明 | 推荐度 |
|------|----|------|------|--------|
| **[dongri/openai-api-rs](https://github.com/dongri/openai-api-rs)** | 486 | Rust | OpenAI API 客户端，支持 chat/completions + streaming | **P0** ⭐ |
| **[64bit/async-openai](https://crates.io/crates/async-openai)** | — | Rust | 社区最活跃的 OpenAI async 客户端 | **P0** ⭐ |
| **[hunjixin/deepseek-api](https://github.com/hunjixin/deepseek-api)** | 9 | Rust | DeepSeek API Rust 封装 | P1 |
| **[rustformers/llm](https://github.com/rustformers/llm)** | — | Rust | 本地 LLM 推理（已归档但仍有参考价值） | P3 |

### 分析

**国内大模型**（DeepSeek、千问、智谱等）大多提供 OpenAI 兼容接口。最佳集成路径：
1. 用 `async-openai` 或 `openai-api-rs` 作为统一客户端
2. 在 lingshu llmkit 中扩展 `base_url` 和 `model` 枚举即可
3. **无需为每个国内模型写独立 SDK**

### 建议集成
```rust
// lingshu-llmkit 中新增
pub enum ChineseProvider {
    DeepSeek { api_key: String, base_url: String },  // https://api.deepseek.com
    Qwen { api_key: String, base_url: String },       // https://dashscope.aliyuncs.com
    Zhipu { api_key: String, base_url: String },      // https://open.bigmodel.cn
    Baidu { api_key: String, secret_key: String },     // https://aip.baidubce.com
}
```

---

## 🔗 MCP 协议

### 已发现的高质量项目

| 项目 | ⭐ | 语言 | 说明 | 推荐度 |
|------|----|------|------|--------|
| **[samvallad33/vestige](https://github.com/samvallad33/vestige)** | 580 | Rust | AI agent 记忆 MCP 服务器，本地优先 | **P1** |
| **[Roblox/studio-rust-mcp-server](https://github.com/Roblox/studio-rust-mcp-server)** | 477 | Rust | Roblox Studio MCP 服务器（架构参考） | P2 |
| **[Episkey-G/GrokSearch-rs](https://github.com/Episkey-G/GrokSearch-rs)** | 264 | Rust | Grok 搜索 MCP 服务器 | P2 |
| **[fabio-rovai/open-ontologies](https://github.com/fabio-rovai/open-ontologies)** | 182 | Rust | RDF/OWL 本体引擎 MCP 服务器 | P1 |
| **[postrv/narsil-mcp](https://github.com/postrv/narsil-mcp)** | 170 | Rust | 代码智能 MCP 服务器（90+ tools） | **P0** ⭐ |

### 分析

lingshu 已使用 `rmcp` crate 构建了自己的 MCP Server。建议：
- **narsil-mcp** 的 90+ 代码工具架构值得参考
- **open-ontologies** 的 SPARQL/RDF 能力可增强 lingshu 知识图谱
- 但 lingshu 当前的核心 MCP 需求是**通道桥接**（已通过 Node.js 实现）

---

## 🤖 Agent 框架

### 已发现的高质量项目

| 项目 | ⭐ | 语言 | 说明 | 推荐度 |
|------|----|------|------|--------|
| **[Maskviva/Ambi](https://github.com/Maskviva/Ambi)** | 1 | Rust | 高性能跨平台 Rust AI Agent 框架 | P2 |
| **[traitclaw/traitclaw](https://github.com/traitclaw/traitclaw)** | 0 | Rust | Trait 驱动的 Agent 框架 | P3 |
| **[Bahtya/kestrel-agent](https://github.com/Bahtya/kestrel-agent)** | 0 | Rust | 流式优先的 Rust Agent 框架 | P2 |
| **[0xPlaygrounds/rig](https://github.com/0xPlaygrounds/rig)** | — | Rust | **最活跃的 Rust Agent 框架**（社区标准） | **P0** ⭐ |

### 分析

lingshu 已经构建了完整的 Agent 系统（`lingshu-runtime`、`lingshu-agent`、`lingshu-traits`），33K 行 Rust 代码远超这些早期框架。**不建议替换**，但可以：
- 参考 rig 的 tool calling 和 retriever 接口设计
- 参考 narsil-mcp 的 tool 注册模式
- 重点是在现有架构上**丰富插件生态**而非重造轮子

---

## ⚡ WASM 插件

### 已有认知

| 项目 | 说明 | 推荐度 |
|------|------|--------|
| **[extism/extism](https://github.com/extism/extism)** | WASM 插件系统，支持多语言 host/guest | **P0** ⭐ |
| **[bytecodealliance/wasmtime](https://github.com/bytecodealliance/wasmtime)** | Rust WASM 运行时，官方成熟 | **P0** ⭐ |
| **[wasmCloud/wasmCloud](https://github.com/wasmCloud/wasmCloud)** | WASM 分布式计算平台 | P2 |

### 分析

lingshu 已有 WASM Plugin SDK。建议：
- extism 的 PDK（Plugin Development Kit）模型值得参考
- wasmtime 的组件模型（Component Model）可用于更安全的沙箱
- 当前重点：完善现有 WASM 热加载机制，编写有实际生产力的插件

---

## 💬 消息通道

### 已发现的高质量项目

| 平台 | 推荐 Rust crate | ⭐ | 说明 | 推荐度 |
|------|-----------------|---|------|--------|
| **Telegram** | **[teloxide/teloxide](https://github.com/teloxide/teloxide)** | — | 最成熟的 Rust Telegram Bot 框架 | **P0** ⭐ |
| **Discord** | **[serenity-rs/serenity](https://github.com/serenity-rs/serenity)** | — | Rust Discord API 库（生产级） | **P1** |
| **Slack** | **[slack-rs/slack-rs](https://github.com/slack-rs/slack-rs)** | — | Rust Slack SDK | P2 |
| **飞书/Lark** | — | — | **无独立 Rust SDK**（已自实现 `FeishuChannel`） | ✅ |
| **QQ** | — | — | **无独立 Rust SDK**（已自实现 `QqChannel`） | ✅ |
| **微信** | — | — | 无 Rust SDK，需 MCP 桥接 | P3 |
| **钉钉** | — | — | 无 Rust SDK，需 MCP 桥接 | P3 |
| **WhatsApp** | — | — | 无 Rust SDK，需 MCP 桥接 | P3 |

### 分析

**已实现**: Telegram/Feishu/QQ 原生 Rust 通道（lingshu-channel）
**待实现**: 更多平台通过 MCP Bridge（Node.js 网关）注入

---

## 📚 RAG / 向量数据库

### 已发现的高质量项目

| 项目 | 说明 | 推荐度 |
|------|------|--------|
| **[qdrant/qdrant-client](https://github.com/qdrant/qdrant)** | 官方 Rust gRPC/REST 客户端 | **P0** ⭐ |
| **[lancedb/lancedb](https://github.com/lancedb/lancedb)** | 嵌入式向量数据库（零依赖） | **P1** |
| **[pgvector/pgvector](https://github.com/pgvector/pgvector)** | PostgreSQL 向量扩展 | P2 |
| **[pkalivas/textsplitter](https://github.com/pkalivas/textsplitter)** | Rust 文本分割库（RAG 预处理） | **P0** ⭐ |

### 建议

```rust
// RAG 插件架构建议
pub trait VectorStore {
    async fn store_embeddings(&self, docs: Vec<Document>) -> Result<()>;
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<Document>>;
}

// Qdrant 实现 vs LanceDB 实现
// lingshu-plugin 热加载
```

---

## 🕸️ 知识图谱

| 项目 | 说明 | 推荐度 |
|------|------|--------|
| **[oxigraph/oxigraph](https://github.com/oxigraph/oxigraph)** | Rust SPARQL/RDF 数据库 | **P0** ⭐ |
| **[pometry/raphtory](https://github.com/pometry/raphtory)** | Rust 时序图数据库 | P2 |

lingshu 已有自己的 `KnowledgeGraph` 和 `GraphStore`（SQLite），oxigraph 可用于增强 SPARQL 查询能力。

---

## 🎨 多模态

| 项目 | 说明 | 推荐度 |
|------|------|--------|
| **[tazz4843/whisper-rs](https://github.com/tazz4843/whisper-rs)** | OpenAI Whisper Rust 绑定（语音识别） | **P0** ⭐ |
| **[rust-tts/tts-rs](https://github.com/rust-tts/tts-rs)** | Rust TTS | P2 |
| **[sachaos/audio-to-text-rs](https://github.com/sachaos/audio-to-text-rs)** | 音频转文字 | P2 |

---

## 🗄️ 数据库

| crate | 说明 | lingshu 现有 |
|-------|------|-------------|
| **sqlx** | async SQL 工具包 | ✅ 已集成 |
| **tokio-postgres** | PostgreSQL 原生客户端 | — |
| **fred** | Redis 客户端 | — |
| **sled** | 嵌入式 KV 存储 | — |

---

## 🖥️ WebUI

| 框架 | 说明 | lingshu 现有 |
|------|------|-------------|
| **Yew** | Rust WASM 框架 | ✅ 已有 WebUI |
| **Leptos** | 全栈 Rust Web 框架 | — |
| **Dioxus** | 跨平台 Rust UI | — |

---

## 🔧 工具

| crate | 说明 | lingshu 现有 |
|-------|------|-------------|
| **utoipa** | OpenAPI 自动生成 | — |
| **tonic** | gRPC 框架 | ✅ 已集成 |
| **tokio-tungstenite** | WebSocket | — |
| **tower** | 服务中间件 | — |

---

## 📊 优选引入优先级清单

| 优先级 | 模块 | 推荐项目 | 集成难度 | 对 lingshu 的价值 |
|--------|------|---------|---------|------------------|
| **P0** 🔥 | OpenAI 客户端 | `async-openai` / `openai-api-rs` | 低 | 统一覆盖 DeepSeek/千问等 |
| **P0** 🔥 | 文本分割 | `textsplitter` | 低 | RAG 插件基础 |
| **P0** 🔥 | 向量数据库 | `qdrant-client` | 中 | 构建 RAG 插件 |
| **P0** 🔥 | 嵌入向量 | `fastembed-rs` | 中 | 本地嵌入 |
| **P1** | 语音识别 | `whisper-rs` | 中 | 语音输入 |
| **P1** | 嵌入 DB | `lancedb` | 中 | 零依赖向量存储 |
| **P2** | 知识图谱 | `oxigraph` | 高 | SPARQL 增强 |
| **P2** | Agent 参考 | `rig` 接口设计 | 低 | 架构借鉴 |
| **P2** | MCP 参考 | `narsil-mcp` | 低 | tool 架构参考 |
| **P3** | TEE | Intel SGX SDK | 高 | 生产部署 |
| **P3** | WebUI | Yew 生态系统 | 中 | 前端组件 |

---

> **结论**: lingshu 已实现 75% 的核心架构。最优先的是:
> 1. 引入 `async-openai`/`openai-api-rs` 统一覆盖国内大模型
> 2. 构建 RAG 插件（`textsplitter` + `qdrant-client` + `fastembed-rs`）
> 3. 完善通道生态（已有 3 个原生通道，通过 MCP Bridge 扩展更多）
> 4. 参考 `rig` 和 `narsil-mcp` 的接口设计优化现有架构
