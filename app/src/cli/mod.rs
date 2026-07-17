pub mod app;
pub mod commands;
pub mod context;
pub mod logging;
pub mod mcp;
pub mod model;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{event, execute};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::CancellationToken;

use app::App;

/// RAII 终端生命周期管理器。
///
/// 保证：
/// - `new()` 时进入 Alternate Screen + Raw Mode
/// - `drop()` 时恢复终端 (即使 panic / Ctrl+C / 异常退出)
///
/// 使用示例：
/// ```ignore
/// let guard = TerminalGuard::new()?;
/// run_tui(workspace, guard.terminal(), token)?;
/// drop(guard);
/// ```
pub struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stderr>>,
}

impl TerminalGuard {
    /// 创建终端守卫，进入 Alternate Screen 并启用 Raw Mode。
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stderr = io::stderr();
        execute!(stderr, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stderr);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self { terminal })
    }

    /// 获取可变的 Terminal 引用，供 TUI 渲染使用。
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<io::Stderr>> {
        &mut self.terminal
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // 忽略错误：drop 期间不应 panic
        let _ = self.terminal.clear();
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}

/// 启动 CIL (Command Interaction Loop) 交互式会话。
///
/// 生命周期：
/// 1. TerminalGuard::new() → 进入 Alternate Screen + Raw Mode
/// 2. 注册 Ctrl+C 取消令牌 → 后台信号处理器
/// 3. run_tui() → TUI 渲染循环
/// 4. drop(guard) → 恢复终端 (即使 panic / 异常)
/// 5. println() → 在正常屏幕输出 session 统计
pub fn run_cil(workspace_dir: Option<String>) -> Result<()> {
    let workspace = workspace_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let mut guard = TerminalGuard::new()?;

    // 创建全局取消令牌，用于统一管理 TUI 生命周期
    // 任何后台任务可以通过监听 cancel_token 实现优雅退出
    let cancel_token = CancellationToken::new();

    // 注册 Ctrl+C 信号处理器
    // 当用户按下 Ctrl+C 时：
    // 1. cancel_token.cancel() 被调用
    // 2. 所有监听 token 的后台任务收到取消信号
    // 3. run_tui 退出主循环
    let cancel_on_ctrlc = cancel_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tokio::spawn(async move {
            // 简短延迟，让当前渲染帧完成
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_on_ctrlc.cancel();
        });
    });

    // TUI 主循环（在 Alternate Screen 中执行）
    let message_count = run_tui(workspace, guard.terminal(), cancel_token)?;

    // 显式丢弃 guard → LeaveAlternateScreen + disable_raw_mode
    // 必须在 println 之前执行，否则输出不可见
    drop(guard);

    // 此时终端已恢复为正常屏幕，println 可见
    println!(
        "LingShu CIL session ended. {message_count} messages logged."
    );

    Ok(())
}

/// TUI 渲染循环（在 Alternate Screen 中运行）。
///
/// # 参数
/// - `workspace`: 工作目录路径
/// - `terminal`: 由 TerminalGuard 管理的终端引用
/// - `cancel_token`: 取消令牌，Ctrl+C 时触发取消
///
/// # 返回
/// - 会话期间记录的消息数量
fn run_tui(
    workspace: PathBuf,
    terminal: &mut Terminal<CrosstermBackend<io::Stderr>>,
    cancel_token: CancellationToken,
) -> Result<usize> {
    let mut app = App::new(workspace)?;
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = std::time::Instant::now();

    while !app.should_exit && !cancel_token.is_cancelled() {
        terminal.draw(|frame| app.render(frame))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                        app.should_exit = true;
                        cancel_token.cancel();
                        break;
                    }
                    app.handle_key_event(key)?;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse_event(mouse.kind, mouse.row);
                }
                Event::Resize(_w, _h) => {}
                _ => {}
            }
        }

        if last_tick.elapsed() >= Duration::from_secs(5) {
            app.advance_tip();
            last_tick = std::time::Instant::now();
        }
    }

    Ok(app.messages.len())
}
