# 插件系统

Lingshu 支持 **静态插件** 和 **WASM 热加载插件** 两种模式。

## 内置插件

| 插件 | 类型 | 功能 |
|------|------|------|
| web-search | 静态 | DuckDuckGo/Bing/Google 搜索 |
| scheduler | 静态 | cron/间隔/一次性任务调度 |
| rag | 静态 | 简易 RAG (零外部依赖) |
| code-sandbox | 静态 | 安全代码执行 |
| beef | 静态 | Beef 协议集成 |
| watch | 静态 | 文件系统监听 |

## 插件开发

参考 `plugins/web-search-plugin/` 或 `plugins/scheduler-plugin/` 的源码结构。
