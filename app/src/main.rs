//! 🚀 Lingshu Agent System — 主二进制入口
//!
//! 用法:
//!   lingshu                  启动 HTTP API 服务 (默认 :8080)
//!   lingshu --repl           启动交互式 REPL
//!   lingshu -e prod          生产模式
//!   lingshu --addr 0.0.0.0:8080

mod api;

use clap::Parser;
use std::sync::Arc;
use tracing::{error, info};

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_config::env::Environment;
use lingshu_config::settings::{LsConfig, LlmProvider};
use lingshu_eventbus::bus::InMemoryEventBus;
use lingshu_observability::ObservabilityConfig;
use lingshu_runtime::lifecycle::{LifecycleManager, LifecycleState};
use lingshu_runtime::recovery::RecoveryManager;
use lingshu_runtime::scheduler::InternalScheduler;
use lingshu_runtime::session::SessionManager;
use lingshu_security::service_auth::ServiceKeyBundle;
use lingshu_storage::LocalStorage;
use lingshu_runtime::ToolRegistry;

use crate::api::AppState;

/// Lingshu Agent System CLI
#[derive(Parser, Debug)]
#[command(name = "lingshu", version, about = "Lingshu Agent System v1.0.0")]
pub struct Cli {
    /// 环境选择 (dev/test/prod)
    #[arg(short = 'e', long, default_value = "dev")]
    env: String,

    /// 监听地址
    #[arg(long, default_value = "127.0.0.1:8080")]
    addr: String,

    /// 以 REPL 模式启动 (而非 HTTP 服务)
    #[arg(long)]
    repl: bool,

    /// 无头模式 (不启动任何前端)
    #[arg(long)]
    headless: bool,
}

/// Lingshu 系统运行时 — 全系统资源管控中心
pub struct LingshuRuntime {
    pub lifecycle: LifecycleManager,
    pub scheduler: InternalScheduler,
    pub session_mgr: SessionManager,
    pub event_bus: Arc<InMemoryEventBus>,
    pub recovery: RecoveryManager,
    pub storage: LocalStorage,
    pub config: LsConfig,
    pub llm: Option<Box<dyn lingshu_traits::llm::Llm>>,
    pub service_key: Option<ServiceKeyBundle>,
    pub root_ctx: LsContext,
    pub tool_registry: lingshu_runtime::ToolRegistry,
}

impl LingshuRuntime {
    /// 初始化全系统运行时
    pub async fn initialize(cli: &Cli) -> LsResult<Self> {
        let root_ctx = LsContext::with_session(LsId::new())
            .with_user("system")
            .with_metadata("source", "lingshu-init");

        let lifecycle = LifecycleManager::new();
        lifecycle.transition(&root_ctx, LifecycleState::Initializing)
            .map_err(|e| LsError::Internal(format!("lifecycle init failed: {e}")))?;

        let config = LsConfig::load_for_env(&cli.env)
            .unwrap_or_else(|_| LsConfig::default());
        let environment: Environment = cli.env.parse()
            .unwrap_or(Environment::Dev);
        info!(environment = %environment, "configuration loaded");

        let obs_config = ObservabilityConfig {
            service_name: "lingshu".into(),
            service_version: "1.0.0".into(),
            environment,
            json_output: matches!(environment, Environment::Prod),
            log_level: environment.log_level().to_string(),
            ..ObservabilityConfig::default()
        };
        lingshu_observability::tracing::init_tracing(&obs_config)
            .map_err(|e| LsError::Internal(format!("tracing init failed: {e}")))?;

        let event_bus = Arc::new(InMemoryEventBus::new());
        let recovery = RecoveryManager::new(3);
        let scheduler = InternalScheduler::new(config.runtime.max_concurrent_tasks as usize);
        let session_mgr = SessionManager::new(config.runtime.session_ttl_seconds);
        let service_key = ServiceKeyBundle::generate("lingshu");

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("./data"))
            .join("lingshu");
        std::fs::create_dir_all(&data_dir).ok();
        let storage = LocalStorage::new(data_dir);
        let tool_registry = ToolRegistry::new();

        let llm: Option<Box<dyn lingshu_traits::llm::Llm>> = match config.llm.provider {
            LlmProvider::Mock | LlmProvider::Openai | LlmProvider::Anthropic | LlmProvider::Groq => {
                Some(lingshu_backends::build_llm(&config.llm))
            }
        };

        let runtime = Self {
            lifecycle,
            scheduler,
            session_mgr,
            event_bus,
            recovery,
            storage,
            config,
            llm,
            service_key: Some(service_key),
            root_ctx,
            tool_registry,
        };

        runtime.lifecycle.transition(&runtime.root_ctx, LifecycleState::Running)
            .map_err(|e| LsError::Internal(format!("lifecycle startup failed: {e}")))?;

        info!("lingshu runtime initialized successfully");
        Ok(runtime)
    }

    /// 优雅关闭
    pub async fn shutdown(&self) -> LsResult<()> {
        self.lifecycle.transition(&self.root_ctx, LifecycleState::ShuttingDown)
            .map_err(|e| LsError::Internal(format!("lifecycle shutdown failed: {e}")))?;

        self.scheduler.pause();
        info!("scheduler paused");

        self.lifecycle.transition(&self.root_ctx, LifecycleState::Stopped)
            .map_err(|e| LsError::Internal(format!("lifecycle stop failed: {e}")))?;

        info!("lingshu runtime shut down gracefully");
        Ok(())
    }

    /// 创建用户会话
    pub async fn create_session(&self, user_id: &str) -> LsResult<LsId> {
        let session_id = LsId::new();
        let ctx = LsContext::with_session(session_id)
            .with_user(user_id);
        self.session_mgr.create(&ctx).await?;
        Ok(session_id)
    }
}

async fn run_repl(runtime: &LingshuRuntime) -> LsResult<()> {
    println!();
    println!("╔══════════════════════════════════════════╗");
    println!("║        🚀 Lingshu Agent System v1.0      ║");
    println!("║     Type '/help' for commands            ║");
    println!("║     Type '/quit' to exit                 ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    let session_id = runtime.create_session("repl_user").await?;
    let ctx = LsContext::with_session(session_id).with_user("repl_user");

    use tokio::io::{AsyncBufReadExt, BufReader};

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    info!(session_id = %session_id, "REPL session started");

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        match line.as_str() {
            "/quit" | "/exit" | "/q" => {
                println!("👋 Goodbye!");
                break;
            }
            "/help" | "/h" | "/?" => {
                println!("Commands:");
                println!("  /help, /h, /?    Show this help");
                println!("  /quit, /exit, /q Exit");
                println!("  /stats           Show runtime statistics");
                println!("  /session         Show current session info");
                println!("  /llm <prompt>    Send prompt to LLM");
                println!("  <any text>       Chat with the agent");
            }
            "/stats" => {
                println!("Runtime Statistics:");
                println!("  Lifecycle:    {:?}", runtime.lifecycle.current());
                println!("  Sessions:     {}", runtime.session_mgr.active_count().await);
                println!("  Tasks:        {}", runtime.scheduler.task_count().await);
                println!("  Events:       {}", runtime.event_bus.history().await.len());
                let healing = runtime.recovery.is_circuit_open();
                println!("  Circuit:      {}", if healing { "⚠ OPEN" } else { "✓ closed" });
                println!("  Storage:      {}", runtime.storage.base_path().display());
            }
            "/session" => {
                let sessions = runtime.session_mgr.list_all().await;
                println!("Active Sessions ({}):", sessions.len());
                for s in &sessions {
                    println!("  {} | user={:?} | state={:?} | created={}",
                        s.session_id, s.user_id, s.state, s.created_at);
                }
            }
            cmd if cmd.starts_with("/llm ") || !cmd.starts_with('/') => {
                let prompt = if cmd.starts_with("/llm ") {
                    cmd.trim_start_matches("/llm ")
                } else {
                    &cmd
                };

                if let Some(llm) = &runtime.llm {
                    let child_ctx = ctx.child();
                    let request = lingshu_traits::llm::LlmRequest {
                        model: runtime.config.llm.default_model.clone(),
                        messages: vec![
                            lingshu_traits::llm::LlmMessage {
                                role: lingshu_traits::llm::LlmRole::User,
                                content: prompt.to_string(),
                                name: None,
                                tool_calls: None,
                            },
                        ],
                        temperature: Some(0.7),
                        max_tokens: Some(runtime.config.llm.max_tokens),
                        tools: None,
                        stream: false,
                    };

                    match llm.invoke(child_ctx, request).await {
                        Ok(response) => {
                            println!();
                            println!("🤖 {}", response.message.content);
                            println!("\n[usage: {} prompt + {} completion | total: {} tokens]",
                                response.usage.prompt_tokens,
                                response.usage.completion_tokens,
                                response.usage.total_tokens);
                        }
                        Err(e) => {
                            eprintln!("⚠ LLM error: {e}");
                        }
                    }
                } else {
                    println!("⚠ No LLM backend configured.");
                }
            }
            _ => {
                println!("Unknown command: {line}. Type /help for available commands.");
            }
        }
    }

    Ok(())
}

async fn run_http_server(runtime: Arc<LingshuRuntime>, addr: &str) -> LsResult<()> {
    let health_registry = Arc::new(lingshu_observability::health::HealthRegistry::new("lingshu", "1.0.0"));

    // Register built-in health checks
    {
        use lingshu_observability::health::RuntimeHealth;
        use std::sync::Arc;

        let (_, ready_rx) = tokio::sync::watch::channel(true);
        let runtime_check = RuntimeHealth::new("runtime", Arc::new(ready_rx));
        health_registry.register(Box::new(runtime_check)).await;
    }

    // Register build info metric
    {
        let registry = lingshu_observability::metrics::MetricsRegistry::global();
        if let Ok(build_info) = registry.gauge(
            "lingshu_build_info",
            "Build information",
            &["version", "rustc"],
        ) {
            build_info.with_label_values(&["1.0.0", "stable"]).set(1);
        }
    }

    let state = Arc::new(AppState {
        runtime: runtime.clone(),
        plugin_registry: Arc::new(lingshu_plugin::PluginRegistry::new()),
        health_registry,
    });
    let app = api::build_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await
        .map_err(|e| LsError::Internal(format!("bind {addr} failed: {e}")))?;

    info!(addr = %addr, "HTTP server started");
    println!("🌐 Lingshu HTTP API server listening on http://{addr}");
    println!("   📋 GET  /health");
    println!("   📋 GET  /version");
    println!("   📋 GET  /v1/models");
    println!("   📋 POST /v1/chat/completions");
    println!("   📋 POST /v1/chat");
    println!("   📋 POST /v1/agent/run");
    println!("   📋 GET  /v1/chat/stream?prompt=...");
    println!("   📋 WS   /ws");
    println!("   📋 POST /v1/embeddings");
    println!("   📋 POST /v1/embed");

    axum::serve(listener, app)
        .await
        .map_err(|e| LsError::Internal(format!("server error: {e}")))?;

    Ok(())
}

#[tokio::main]
async fn main() -> LsResult<()> {
    let cli = Cli::parse();
    let runtime = Arc::new(LingshuRuntime::initialize(&cli).await?);

    // 优雅关闭信号
    let rt = runtime.clone();
    let shutdown_done = Arc::new(tokio::sync::Notify::new());
    let sd = shutdown_done.clone();

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
        }
        info!("shutdown signal received, stopping runtime");
        if let Err(e) = rt.shutdown().await {
            error!(error = %e, "error during shutdown");
        }
        sd.notify_one();
    });

    if cli.repl {
        run_repl(&runtime).await?;
    } else if cli.headless {
        info!("headless mode — waiting for shutdown signal");
        shutdown_done.notified().await;
    } else {
        run_http_server(runtime, &cli.addr).await?;
    }

    info!("lingshu exited cleanly");
    Ok(())
}
