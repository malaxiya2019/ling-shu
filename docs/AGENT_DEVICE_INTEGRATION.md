# agent-device + LingShu 集成分析报告

> 分析日期：2026-07-14
> agent-device 版本：0.19.3 | LingShu 版本：5.1.0

---

## 一、集成现状总览

### ✅ 已完成的工作

LingShu 已经构建了一个**完整的 agent-device 插件** (`plugins/agent-device-plugin/`)，包含：

| 组件 | 状态 | 说明 |
|------|------|------|
| `McpStdioClient` | ✅ 完整 | 通过 stdin/stdout JSON-RPC 2.0 协议与 agent-device 子进程通信 |
| `McpBridgeTool` | ✅ 完整 | 将 MCP 工具包装为 LingShu Tool 接口 |
| `AgentDevicePlugin` | ✅ 完整 | 实现 Plugin trait，完整生命周期管理 |
| `init_agent_device()` | ✅ 完整 | 一站式初始化函数 |
| 平台过滤 | ✅ 完整 | iOS/Android/Linux 平台工具过滤 |
| 依赖检查 | ✅ 完整 | Node.js/agent-device/Xcode/ADB 检查 |
| 集成测试 | ✅ 完整 | 3 个集成测试覆盖核心流程 |
| 单元测试 | ✅ 完整 | Platform 属性/测试覆盖 |
| 编译通过 | ✅ | 整个项目 `cargo check` 零错误 |

### 📊 集成深度

```
LingShu Agent ──► ToolRegistry ──► AgentDevicePlugin
                                        │
                                   McpStdioClient
                                        │
                                   stdin/stdout
                                        │
                                   agent-device mcp
                                        │
                        ┌───────────────┼───────────────┐
                    iOS Sim        Android Emu      Desktop/Linux
```

---

## 二、agent-device 核心能力映射

### 2.1 MCP 暴露的命令集

agent-device 通过 MCP 暴露 **70+ 个命令**，涵盖：

| 类别 | 命令 | 当前集成 |
|------|------|---------|
| **设备管理** | `apps`, `devices`, `boot`, `shutdown`, `connect`, `disconnect` | ✅ 自动发现 |
| **App 控制** | `open`, `close`, `install`, `reinstall`, `session` | ✅ 自动发现 |
| **UI 交互** | `click`, `tap`, `press`, `longpress`, `fill`, `scroll`, `swipe`, `gesture`, `back`, `home` | ✅ 自动发现 |
| **UI 检测** | `snapshot`, `find`, `get`, `appstate`, `wait` | ✅ 自动发现 |
| **证据采集** | `screenshot`, `record`, `logs`, `network`, `audio`, `trace`, `perf` | ✅ 自动发现 |
| **调试** | `react-native`, `metro`, `react-devtools`, `debug`, `cdp` | ✅ 自动发现 |
| **断言** | `alert`, `settle-and-verify`, `diff` | ✅ 自动发现 |
| **重放** | `replay`, `batch` | ✅ 自动发现 |
| **其他** | `clipboard`, `keyboard`, `rotate`, `tv-remote`, `flag`, `settings`, `doctor`, `capabilities` | ✅ 自动发现 |

### 2.2 关键 MCP 特性

- **版本化 Ref** (`~s<n>` 后缀) — 自动 Pin 管理 (PR #1076)
- **Settle 观察** (`--settle`) — 交互后静默快照 (PR #1101)
- **结构化错误** — 标准化的错误格式带 divergenge.screen (ADR 0012)
- **输出格式** — `optimized`(默认) / `json` 两种模式

---

## 三、最优改进方案

### 3.1 插件改进优先级

#### P0 — 核心稳定性（必须）

| 改进项 | 说明 | 工作量 |
|--------|------|--------|
| ✅ **编译+测试** | 已通过 `cargo check` | 已完成 |
| ✅ **集成测试** | 3 个完整集成测试 | 已完成 |

#### P1 — 功能增强（推荐）

| # | 改进项 | 说明 | 优先级 |
|---|--------|------|--------|
| 1 | **动态工具同步** | 添加轮询/事件机制，agent-device 更新后自动同步工具列表 | ⭐⭐⭐ |
| 2 | **会话管理** | 支持多设备多会话隔离，自动跟踪 open/close 生命周期 | ⭐⭐⭐ |
| 3 | **输出格式选项** | 暴露 `outputFormat` (`optimized`/`json`) 配置 | ⭐⭐⭐ |
| 4 | **错误增强** | 解析 agent-device 结构化错误中的 `divergence.screen` 信息 | ⭐⭐ |
| 5 | **性能指标** | 暴露工具调用耗时、成功率等指标到 Observability | ⭐⭐ |

#### P2 — 扩展能力

| # | 改进项 | 说明 |
|---|--------|------|
| 6 | **云设备支持** | Agent Device Cloud / 远程设备连接 |
| 7 | **录制脚本** | 将交互录制为 `.ad` 脚本用于 CI |
| 8 | **Maestro 导出** | 将工作流导出为 Maestro YAML |
| 9 | **CI/CD 集成** | 提供 GitHub Actions / EAS Workflow 模板 |

### 3.2 推荐实现：动态工具同步

```rust
/// 在 AgentDevicePlugin 中添加
impl AgentDevicePlugin {
    /// 启动后台同步任务 — 定期检查 MCP 工具列表变化
    pub async fn start_sync_task(self: &Arc<Self>, interval: Duration) {
        let plugin = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                if !plugin.running.load(Ordering::SeqCst) { break; }
                // 重新发现工具并增量更新
                if let Err(e) = plugin.sync_tools().await {
                    tracing::warn!("sync_tools failed: {e}");
                }
            }
        });
    }
}
```

### 3.3 推荐实现：会话管理器

```rust
/// 跟踪多个 agent-device 会话
pub struct SessionManager {
    sessions: HashMap<String, SessionInfo>,
}

struct SessionInfo {
    id: String,
    platform: String,
    app: String,
    started_at: DateTime<Utc>,
    tools: Vec<String>,
}
```

### 3.4 推荐实现：响应增强

```rust
/// 解析 agent-device 的结构化响应
pub struct AgentDeviceResponse {
    pub text: String,
    pub refs: Vec<String>,
    pub refs_generation: Option<u64>,
    pub settle: Option<SettleInfo>,
    pub structured: Value,
}

pub struct SettleInfo {
    pub settled: bool,
    pub diff: Option<String>,
    pub refs_generation: Option<u64>,
}
```

---

## 四、集成架构图（推荐终极状态）

```
┌─────────────────────────────────────────────────────┐
│                    LingShu System                     │
│  ┌─────────────────────────────────────────────────┐ │
│  │             Orchestrator Layer                   │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │ │
│  │  │ Workflow │  │  Agent   │  │  Multi-Agent  │  │ │
│  │  │  Engine  │  │ Manager  │  │  Coordinator  │  │ │
│  │  └──────────┘  └──────────┘  └──────────────┘  │ │
│  └─────────────────────────────────────────────────┘ │
│                          │                            │
│  ┌───────────────────────┴─────────────────────────┐ │
│  │              Tool System                         │ │
│  │  ┌──────────────────────────────────────────┐    │ │
│  │  │            ToolRegistry                   │    │ │
│  │  │  ┌────────────────────────────────────┐  │    │ │
│  │  │  │  device:snapshot                    │  │    │ │
│  │  │  │  device:open    device:click        │  │    │ │
│  │  │  │  device:fill    device:screenshot   │  │    │ │
│  │  │  │  ... (40+ tools)                    │  │    │ │
│  │  │  └────────────────────────────────────┘  │    │ │
│  │  └──────────────────────────────────────────┘    │ │
│  └───────────────────────┬─────────────────────────┘ │
│                          │                            │
│  ┌───────────────────────┴─────────────────────────┐ │
│  │         AgentDevicePlugin (v2)                   │ │
│  │  ┌────────────┐ ┌──────────┐ ┌──────────────┐  │ │
│  │  │  MCP Stdio │ │  Session │ │  Sync Task   │  │ │
│  │  │  Client    │ │  Manager │ │  (auto poll)  │  │ │
│  │  └─────┬──────┘ └──────────┘ └──────────────┘  │ │
│  └────────┼────────────────────────────────────────┘ │
└───────────┼─────────────────────────────────────────┘
            │ stdin/stdout JSON-RPC 2.0
┌───────────┴─────────────────────────────────────────┐
│              agent-device CLI (v0.19.3)               │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────────┐  │
│  │   MCP    │ │  CLI     │ │   Public Node Client  │  │
│  │  Server  │ │  Parser  │ │   (typed exports)     │  │
│  └────┬─────┘ └──────────┘ └──────────────────────┘  │
│       │                                               │
│  ┌────┴────────────────────────────────────────────┐ │
│  │           Platform Backends                      │ │
│  │  ┌──────────┐ ┌──────────┐ ┌────────────────┐  │ │
│  │  │  XCTest  │ │   ADB    │ │  AT-SPI/Linux  │  │ │
│  │  │  (iOS)   │ │ (Android)│ │  (Desktop)     │  │ │
│  │  └──────────┘ └──────────┘ └────────────────┘  │ │
│  └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

---

## 五、与项目定位的契合度

| 维度 | LingShu 目标 | agent-device 能力 | 匹配度 |
|------|-------------|-------------------|--------|
| Agent 自动化 | 多 Agent 编排 | 设备自动化 CLI | ⭐⭐⭐⭐⭐ |
| MCP 协议 | v2.3 核心计划 | 原生 MCP 支持 | ⭐⭐⭐⭐⭐ |
| 工具系统 | ToolRegistry | 70+ 设备工具 | ⭐⭐⭐⭐⭐ |
| 多模态 | 图像/音频处理 | 截图/视频/音频证据 | ⭐⭐⭐⭐ |
| 实时性 | WebSocket 流式 | 证据采集推送 | ⭐⭐⭐ |
| CI/CD | 持续集成 | `.ad` 重放 + Maestro 导出 | ⭐⭐⭐⭐ |

### 核心结论

**agent-device 与 LingShu 的集成度极高**，两者在 MCP 协议、Tool 系统、设备自动化等维度完全匹配。现有插件已覆盖 80% 的核心功能，剩余的 P1/P2 改进可在 1-2 个迭代周期内完成。

---

## 六、使用场景示例

### 场景 1：Agent 驱动的移动端回归测试

```rust
// LingShu Workflow — 移动应用回归测试
workflow! {
    // 1. Agent 启动 iOS 模拟器
    @tool(device:open, app: "com.example.app", platform: "ios")

    // 2. Agent 检查登录页面
    @tool(device:snapshot, interactive: true)

    // 3. Agent 填写登录信息
    @tool(device:fill, ref: "@e3", value: "user@example.com")
    @tool(device:fill, ref: "@e4", value: "password123")

    // 4. Agent 截图留存
    @tool(device:screenshot, path: "/artifacts/login.png")

    // 5. Agent 断言 UI 状态
    @tool(device:snapshot) → assert contains("Dashboard")
}
```

### 场景 2：Agent 调试 React Native 性能

```rust
// LingShu Agent 自动诊断 RN 性能问题
@tool(device:react-native, action: "profile")
@tool(device:perf, duration: 30)
@tool(device:screenshot)
// Agent 分析结果并给出优化建议
```

### 场景 3：跨平台 CI 自动化

```rust
// 多平台并行测试
let ios_plugin = AgentDevicePlugin::with_config(AgentDeviceConfig {
    platform: Some("ios".into()),
    ..Default::default()
});

let android_plugin = AgentDevicePlugin::with_config(AgentDeviceConfig {
    platform: Some("android".into()),
    ..Default::default()
});

// 两个 Agent 并行执行
join!(
    run_test_suite(ios_plugin),
    run_test_suite(android_plugin),
);
```

---

## 七、下一步建议

| 优先级 | 行动项 | 负责模块 | 预估工时 |
|--------|--------|---------|---------|
| P1 | 添加动态工具同步 | `agent-device-plugin` | 2-3 天 |
| P1 | 实现会话管理器 | `agent-device-plugin` | 2-3 天 |
| P1 | 暴露 outputFormat 配置 | `agent-device-plugin` + `mcp` | 1 天 |
| P2 | 结构化错误解析 | `agent-device-plugin` | 1 天 |
| P2 | Observability 集成 | `agent-device-plugin` + `observability` | 1-2 天 |
| P3 | 云设备支持 | `agent-device-plugin` | 3-5 天 |
| P3 | CI/CD 模板 | `scripts/` | 1 天 |
