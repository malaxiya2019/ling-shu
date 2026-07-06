# Lingshu — 项目进度报告

> 生成日期: 2026-07-05

---

## 1. 已完成功能

### Phase 1: 核心基础设施
| 组件 | 状态 | 说明 |
|------|------|------|
| MCP 工具注册 (`CodeAnalysisTool`) | ✅ | 已注册到 `ToolRegistry`，可通过 MCP 被 Claude/Cursor 调用 |
| Graph REST API | ✅ | `GET/POST /v1/graph/{project}` — 查询/触发分析 |
| 项目列表 API | ✅ | `GET /v1/projects` — 列出已缓存项目 |
| 图谱可视化 WebUI | ✅ | `GET /v1/graph/{project}/view` — vis-network 渲染 |

### Phase 2: 实时能力
| 组件 | 状态 | 说明 |
|------|------|------|
| WebSocket 推送 | ✅ | 分析完成后推送 `graph.updated` 事件 |
| SSE 推送 | ✅ | 通过 `/v2/events` 推送到 WebUI 前端 |
| SSE 错误处理 | ✅ | 无客户端时使用 `warn!` 而非 `error!` |

### Phase 3: 持久化与监听
| 组件 | 状态 | 说明 |
|------|------|------|
| `NotifyFileObserver` | ✅ | 基于 `notify` crate v6 的原生文件事件监听 |
| `PollingFileObserver` | ✅ | 跨平台轮询回退方案 |
| 自动回退机制 | ✅ | 优先 `Notify` → 失败自动回退 `Polling` |
| SQLite 持久化 (`GraphStore`) | ✅ | 图谱存入 `~/.local/share/lingshu/graphs.db` |
| 启动恢复 | ✅ | 重启时自动从 SQLite 恢复图谱到内存 |

---

## 2. 架构总览

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  MCP 调用    │ ──▶ │  Graph API   │ ──▶ │  WebUI      │
│  (Tool注册)   │     │  (axum缓存)   │     │  (vis-network)│
└─────────────┘     └──────┬───────┘     └─────────────┘
                           │                        ▲
                           ▼                        │
                    ┌──────────────┐        ┌──────────────┐
                    │ LLM Enrich   │        │ SSE 推送      │
                    │ (异步队列)    │ ──────▶ │ (实时更新)    │
                    └──────────────┘        └──────────────┘
                           ▲
                           │
                    ┌──────────────┐
                    │ NotifyObserver│
                    │ (原生文件监听) │
                    └──────────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │  SQLite 持久化 │
                    │ (重启恢复)    │
                    └──────────────┘
```

---

## 3. 性能基准

测试环境: Termux on Android (资源受限)
测试数据: `lingshu/app/src` (2 个源文件)

| 指标 | 值 |
|------|-----|
| 文件扫描 | 2 files |
| 图谱节点 | 90 |
| 图谱边 | 88 |
| 流水线耗时 | ~210ms |
| 节点类型 | Function, Class, File |
| 持久化 DB 大小 | ~4KB |

---

## 4. 文件变更

### 新增文件
- `knowledge-graph/src/store.rs` — `GraphStore` SQLite 持久化
- `PROGRESS_REPORT.md` — 本报告

### 修改文件
- `app/src/api.rs` — 添加 Graph API 路由、持久化保存逻辑
- `app/src/main.rs` — 添加 `graph_cache`、`graph_store` 到运行时
- `code-analyzer/src/observer.rs` — 添加 `NotifyFileObserver`
- `code-analyzer/src/lib.rs` — 导出新类型
- `orchestrator/src/pipeline.rs` — 集成 `NotifyFileObserver`
- `websocket/src/broadcast.rs` — SSE 错误级别降级
- `knowledge-graph/Cargo.toml` — 添加 `rusqlite` 依赖
- `knowledge-graph/src/lib.rs` — 导出 `GraphStore`

---

## 5. 测试结果

```
cargo test -p lingshu-knowledge-graph: 16 passed
cargo test -p lingshu-code-analyzer:   29 passed
cargo test -p lingshu-orchestrator:    24 passed
```

---

## 6. 下一步建议

1. **B: MCP 体验完善** — 流式进度、状态查询工具
2. **D: 错误处理** — LLM 降级、监听异常恢复
3. **全量扫描优化** — 当前限制 5 文件，放开到大项目
4. **Docker 部署** — Container 化一键启动
