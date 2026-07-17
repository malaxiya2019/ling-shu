# RFC-0052: Terminal Runtime Isolation

| 字段 | 值 |
|---|---|
| 状态 | Draft |
| 优先级 | P0（生命周期分离）/ P1（RAII）/ P2（抽象） |
| 目标版本 | v5.2 |
| 作者 | Codex Analysis |
| 关联文件 | `app/src/main.rs`, `app/src/cli/mod.rs`, `observability/src/tracing.rs` |

---

## 问题陈述

当前 `--cil` 模式复用完整的 Server Runtime 初始化路径：

```
lingshu --cil
  │
  ├── tracing init (→ stdout)
  ├── Runtime bootstrap (federation, config watcher, graph, memory, tools...)
  ├── 30+ 行日志写入 stdout
  │
  └── cli::run_cil()  ← 此时终端已被日志污染
```

三个具体问题：

1. **生命周期耦合**：Server Runtime 的后台任务（federation discovery、config reload）在 CIL 会话期间持续运行，与 TUI 生命周期无关
2. **输出目标冲突**：`tracing` 写入 stdout，TUI 使用 stderr（通过 crossterm），两者在同一个终端设备上竞争
3. **退出安全**：Ctrl+C 退出 CIL 时，后台任务可能处于未定义状态，且 termios raw mode 可能未正确恢复（Android/Termux 上概率更高）

---

## 修复方案

### 核心架构变动

```rust
// app/src/main.rs — 新增模式路由

#[derive(Debug, Clone, PartialEq)]
enum RunMode {
    Server,
    Cil,
    Batch,
}

fn detect_mode(cli: &Cli) -> RunMode {
    if cli.cil {
        RunMode::Cil
    } else {
        RunMode::Server
    }
}

#[tokio::main]
async fn main() -> LsResult<()> {
    let cli = Cli::parse();

    match detect_mode(&cli) {
        RunMode::Cil => {
            // ── Terminal Runtime Path ──
            // 不启动: federation, config watcher, HTTP/MCP server
            // 只启动: minimal LLM client, memory, agent session, TUI

            observability::init_cil_logging()?;
            cli::run_cil(None)
                .map_err(|e| LsError::Internal(format!("CIL error: {}", e)))?;
            Ok(())
        }

        RunMode::Server => {
            // ── Server Runtime Path (现有逻辑) ──
            let runtime = Arc::new(LingshuRuntime::initialize(&cli).await?);
            runtime.federation.start().await?;
            // ... existing server startup ...
            run_http_server(runtime, &cli.addr).await?;
            Ok(())
        }
    }
}
```

### tracing sink 分离

```rust
// observability/src/tracing.rs — 新增

/// CIL 模式的日志 sink：写入文件，不污染终端
pub fn init_cil_logging() -> Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("lingshu-cil.log")?;

    tracing_subscriber::fmt()
        .with_writer(file)
        .with_target(false)
        .init();

    Ok(())
}
```

### Terminal Runtime 重构（`app/src/cli/mod.rs`）

当前结构：
```
run_cil()
  └── run_tui()
        └── while !app.should_exit { draw; poll; handle; }
```

建议结构：
```
run_cil()
  ├── TerminalGuard::new()       // RAII: enable_raw_mode + EnterAlternateScreen
  ├── CilSession::new(workspace) // 最小化 session（LLM client + context engine）
  ├── session.run(&mut terminal) // tui_loop (owns CancellationToken)
  └── TerminalGuard::drop()      // RAII: restore_terminal
```

```rust
// app/src/cli/mod.rs — 重构后骨架

pub struct TerminalGuard {
    // RAII guard for terminal state
}

impl TerminalGuard {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        execute!(io::stderr(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stderr(), LeaveAlternateScreen);
    }
}

pub struct CilSession {
    app: App,
    cancel: CancellationToken,
}

impl CilSession {
    pub fn new(workspace: Option<PathBuf>) -> Result<Self> {
        // 最小化初始化：不涉及 federation/config/hot-reload
        let app = App::new(workspace.unwrap_or_else(|| PathBuf::from(".")))?;
        Ok(Self {
            app,
            cancel: CancellationToken::new(),
        })
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        // TUI event loop — 拥有 CancellationToken
        while !self.app.should_exit {
            terminal.draw(|frame| self.app.render(frame))?;
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.modifiers == KeyModifiers::CONTROL
                        && key.code == KeyCode::Char('c') => {
                        self.cancel.cancel();
                        self.app.should_exit = true;
                        break;
                    }
                    Event::Key(key) => self.app.handle_key_event(key)?,
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

pub fn run_cil(workspace: Option<PathBuf>) -> Result<()> {
    let _guard = TerminalGuard::new()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stderr()))?;
    let mut session = CilSession::new(workspace)?;
    session.run(&mut terminal)?;
    Ok(()) // TerminalGuard::drop() 确保终端恢复
}
```

---

## 后续扩展

```
Terminal Runtime
│
├── CIL (v5.2)
│   ├── ratatui TUI
│   ├── keyboard / mouse
│   └── 文件日志
│
├── Web Console (v5.3+)
│   ├── websocket transport
│   └── browser-based UI
│
└── Mobile Console (v5.4+)
    ├── lightweight client
    └── touch input
```

---

## 变更清单

| 文件 | 改动 | 优先级 |
|---|---|---|
| `app/src/main.rs` | 新增 `RunMode` enum + `match mode` 路由 | P0 |
| `observability/src/tracing.rs` | 新增 `init_cil_logging()` | P0 |
| `app/src/cli/mod.rs` | `TerminalGuard` RAII / `CilSession` / 取消令牌 | P1 |
| `app/src/Cargo.toml` | 新增依赖：`tokio-util` (CancellationToken) | P1 |

## 不在此 RFC 范围内

- Federation 与 CIL 的懒加载
- Agent Swarm 终端控制
- Truth-Video UI 集成
- 这些是后续 RFC 的主题

---

## 实施检查项（v5.2 开票时补充）

### 1. Mode Router 演进

当前草案 `main.rs` 中直接 `match detect_mode(&cli)`，后续不应让 `main.rs` 膨胀。

**目标结构：**

```
app/src/
├── main.rs          ← 仅路由
├── cil/
│   └── mod.rs       ← cil::run(), cil::bootstrap()  
├── server/
│   └── mod.rs       ← server::run(), server::bootstrap()
└── batch/
    └── mod.rs       ← batch::run(), batch::bootstrap()
```

```rust
// main.rs — 最终形态
match mode {
    RunMode::Cil    => cil::run(),
    RunMode::Server => server::run(),
    RunMode::Batch  => batch::run(),
}
```

每个模式目录拥有自己的 `bootstrap()`、`run()`、`shutdown()`。

---

### 2. TerminalGuard 必须 RAII（P0）

禁止以下模式：
```rust
// ❌ 错误：非 RAII，panic / Ctrl+C / Termux 杀进程 时无法恢复
enable_raw_mode();
run();
disable_raw_mode();
```

正确做法：
```rust
// ✅ RAII：无论任何退出路径，终端状态一定恢复
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
}

impl TerminalGuard {
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stderr = io::stderr();
        execute!(stderr, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stderr);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        // Drop of Terminal 自动恢复 cursor visibility 等
    }
}
```

---

### 3. CancellationToken（建议升级为 P0）

LingShu 后台任务清单（未来会更多）：

```
Runtime
├── Memory worker
├── Agent scheduler
├── Federation discovery
├── Tool runtime
├── Config watcher
└── Graph engine
```

当前 CIL 退出时这些任务无统一 shutdown → 资源泄漏 + 终端状态损坏。

```rust
use tokio_util::sync::CancellationToken;

pub struct CilSession {
    cancel: CancellationToken,
    // ...
}

impl CilSession {
    pub fn run(&mut self, guard: &mut TerminalGuard) -> Result<()> {
        let token = self.cancel.clone();

        // 所有后台任务：
        tokio::select! {
            _ = token.cancelled() => {
                self.shutdown().await;
            }
            _ = worker_loop() => {}
        }

        while !self.app.should_exit {
            // TUI loop
        }
    }
}
```

---

### 4. TracingProfile 抽象

不要只做 "CIL → file, Server → stdout" 的二选一。建议引入 Profile：

```rust
pub enum TracingProfile {
    Server,          // stdout / journald
    Cil,             // lingshu-cil.log
    Test,            // 吞掉所有日志
}

pub fn init_tracing(profile: TracingProfile) -> Result<()> {
    match profile {
        TracingProfile::Server => {
            // 现有 tracing init
        }
        TracingProfile::Cil => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("logs/cil.log")?;
            tracing_subscriber::fmt()
                .with_writer(file)
                .with_target(false)
                .init();
        }
        TracingProfile::Test => {
            tracing_subscriber::fmt()
                .with_writer(std::io::sink())
                .init();
        }
    }
    Ok(())
}
```

以后 `lingshu --cil` 自动走 `CilProfile`，日志写入 `logs/cil.log`。

---

### 5. 测试计划

#### Terminal Isolation Test
```rust
#[test]
fn test_cil_owns_terminal() {
    // 验证：
    // ✓ 启动 CIL 后无 stdout 日志
    // ✓ Alternate Screen 已激活
    // ✓ 退出后终端完全恢复
}
```

#### Runtime Isolation Test
```rust
#[test]
fn test_cil_does_not_start_server() {
    // 验证：
    // ✓ federation 未启动
    // ✓ server listener 未打开
    // ✓ memory session 可用
}
```

#### Shutdown Test
```rust
#[test]
fn test_cil_cancellation_shuts_down_workers() {
    // 验证：
    // ✓ Ctrl+C → cancellation triggered
    // ✓ workers stopped
    // ✓ terminal restored
}
```

---

## RFC-0052 定位

> **不是修复 CIL，而是建立 LingShu 多运行模式的基础设施。**

| 版本 | 主题 |
|---|---|
| v5.1 | Agent Runtime 成型 |
| **v5.2** | **Terminal Runtime Isolation** |
| v5.3 | Web Console（websocket + browser UI） |
| v5.4 | Mobile Console（lightweight client） |

这条 RFC 现在冻结，等 v5.2 开工再实施。

---

*RFC-0052 · Draft · 2026-07-17 · 实施检查项补充自 Codex 分析会话*
