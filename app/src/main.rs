#![recursion_limit = "512"]
//! 🚀 Lingshu Agent System — 主二进制入口
//!
//! 用法:
//!   lingshu --cil            启动 CIL 终端 TUI
//!   lingshu --repl           启动交互式 REPL
//!   lingshu -e prod          生产模式
//!   lingshu --addr 0.0.0.0:8080

mod api;
mod cli;

use clap::Parser;
use lingshu_runtime::agent_runtime::AgentRuntime;
use std::io::Write;
use std::sync::Arc;
use tracing::{error, info};

use lingshu_channel::registry::ChannelRegistry;
use lingshu_config::env::Environment;
use lingshu_config::settings::{LlmProvider, LsConfig};
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_credentials::CredentialManager;
use lingshu_credentials::CredentialStore;
use lingshu_eventbus::bus::InMemoryEventBus;
use lingshu_observability::ObservabilityConfig;
use lingshu_rag_plugin::RagPlugin;
use lingshu_runtime::lifecycle::{LifecycleManager, LifecycleState};
use lingshu_runtime::recovery::RecoveryManager;
use lingshu_runtime::scheduler::InternalScheduler;
use lingshu_runtime::session::SessionManager;
use lingshu_runtime::ToolRegistry;
use lingshu_security::service_auth::ServiceKeyBundle;
use lingshu_storage::LocalStorage;
use lingshu_websocket::{ConnectionManager, SseBroadcaster};

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
    /// CIL 模式 (终端 TUI 界面)
    #[arg(long)]
    cil: bool,
    /// 禁用联邦通信 (Fed). 默认启用.
    #[arg(long)]
    no_federation: bool,
    /// 联邦监听端口.
    #[arg(long, default_value_t = 9550)]
    federation_port: u16,
    /// 联邦集群名称.
    #[arg(long, default_value = "lingshu-default")]
    cluster_name: String,
}

/// Lingshu 系统运行时 — 全系统资源管控中心
pub struct LingshuRuntime {
    /// 系统启动时间.
    pub start_time: std::time::Instant,
    pub lifecycle: LifecycleManager,
    pub scheduler: InternalScheduler,
    pub session_mgr: SessionManager,
    pub event_bus: Arc<InMemoryEventBus>,
    pub recovery: RecoveryManager,
    pub storage: LocalStorage,
    pub config: LsConfig,
    pub llm: Option<Arc<dyn lingshu_traits::llm::Llm>>,
    pub service_key: Option<ServiceKeyBundle>,
    pub root_ctx: LsContext,
    pub tool_registry: Arc<tokio::sync::RwLock<lingshu_runtime::ToolRegistry>>,
    pub agent_manager: lingshu_runtime::AgentManager,
    pub memory_manager: lingshu_memory::SessionMemoryManager,
    pub mcp_server: Arc<lingshu_mcp::McpServer>,
    /// 知识图谱缓存 <project_name, KnowledgeGraph>.
    pub graph_cache: Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<String, lingshu_knowledge_graph::KnowledgeGraph>,
        >,
    >,
    /// 图谱持久化存储 (SQLite, 重启恢复).
    pub graph_store: std::sync::Arc<lingshu_knowledge_graph::GraphStore>,
    /// 速率限制器.
    pub rate_limiter: std::sync::Arc<lingshu_ratelimit::MultiRateLimiter>,
    /// 审计日志.
    pub audit_log: std::sync::Arc<dyn lingshu_audit::AuditLogStore>,
    /// 提示词管理器.
    pub prompt_registry: std::sync::Arc<lingshu_prompt::PromptRegistry>,
    /// 计费系统.
    pub billing: std::sync::Arc<lingshu_billing::BillingSystem>,
    pub credential_manager: std::sync::Arc<CredentialManager>,
    /// 评测结果缓存 (ES).
    pub eval_store:
        std::sync::Arc<tokio::sync::RwLock<Option<lingshu_evaluator::EvaluationResult>>>,
    /// 联邦通信 (Fed).
    pub federation: std::sync::Arc<lingshu_federation::Federation>,
    /// 配置热重载通知接收器.
    pub config_rx: tokio::sync::broadcast::Receiver<lingshu_config::settings::ConfigEvent>,

    /// [可选] chidori 持久化恢复管理器 (feature = "chidori").
    pub chidori_recovery: Option<std::sync::Arc<lingshu_runtime::ChidoriRecoveryManager>>,
    /// [可选] AutoAgents 编排器桥接 (feature = "autoagents").
    pub autoagents: Option<std::sync::Arc<lingshu_orchestrator::AutoAgentsOrchestrator>>,
    /// [可选] Loong 轻量 Agent 适配器 (feature = "loong").
    pub loong_adapter: Option<std::sync::Arc<lingshu_orchestrator::LoongAdapter>>,
    /// 通道注册表 — 多平台消息通道.
    pub channel_registry: Arc<ChannelRegistry>,

    /// v4.0 Agent Runtime — 新一代 Agent 运行时.
    pub agent_runtime: Option<AgentRuntime>,
    /// v6.0 Swarm 群体智能引擎 (feature = "swarm").
    #[cfg(feature = "swarm")]
    pub swarm_engine:
        Option<std::sync::Arc<tokio::sync::RwLock<lingshu_swarm::engine::SwarmEngine>>>,

    /// v6.0 Autonomy 自我进化引擎 (feature = "autonomy").
    #[cfg(feature = "autonomy")]
    pub evolution_engine: Option<std::sync::Arc<lingshu_autonomy::evolution::EvolutionEngine>>,

    /// v6.0 WorkflowAccess — 工作流注册与执行.
    pub workflow_access:
        Option<std::sync::Arc<lingshu_runtime::workflow_access::RuntimeWorkflowAccess>>,
}

impl LingshuRuntime {
    /// 初始化全系统运行时
    pub async fn initialize(cli: &Cli) -> LsResult<Self> {
        let root_ctx = LsContext::with_session(LsId::new())
            .with_user("system")
            .with_metadata("source", "lingshu-init");

        let lifecycle = LifecycleManager::new();
        lifecycle
            .transition(&root_ctx, LifecycleState::Initializing)
            .map_err(|e| LsError::Internal(format!("lifecycle init failed: {e}")))?;

        let config = LsConfig::load_for_env(&cli.env).unwrap_or_else(|_| LsConfig::default());
        let environment: Environment = cli.env.parse().unwrap_or(Environment::Dev);
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
        let scheduler = InternalScheduler::new(config.runtime.max_concurrent_tasks);
        let session_mgr = SessionManager::new(config.runtime.session_ttl_seconds);
        let service_key = ServiceKeyBundle::generate("lingshu");

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("./data"))
            .join("lingshu");
        std::fs::create_dir_all(&data_dir).ok();
        #[allow(unused_variables)]
        let audit_db_path = data_dir.join("audit.db");
        let storage = LocalStorage::new(data_dir);
        let tool_registry = Arc::new(tokio::sync::RwLock::new(ToolRegistry::new()));
        let agent_manager = lingshu_runtime::AgentManager::new();

        let llm: Option<Arc<dyn lingshu_traits::llm::Llm>> = match config.llm.provider {
            LlmProvider::Mock
            | LlmProvider::Openai
            | LlmProvider::Anthropic
            | LlmProvider::Groq
            | LlmProvider::Llmkit
            | LlmProvider::Llamacpp
            | LlmProvider::DeepSeek
            | LlmProvider::Qwen
            | LlmProvider::Zhipu
            | LlmProvider::Baidu => Some(Arc::from(lingshu_backends::build_llm(&config.llm))),
        };

        // ── Initialize graph store (SQLite persistence) ───
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("lingshu");
        std::fs::create_dir_all(&data_dir).ok();
        let db_path = data_dir.join("graphs.db");
        let graph_store = std::sync::Arc::new(
            lingshu_knowledge_graph::GraphStore::open(&db_path).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "failed to open graph store, using in-memory fallback");
                lingshu_knowledge_graph::GraphStore::in_memory().expect("in-memory store")
            }),
        );
        let graph_cache: Arc<
            tokio::sync::RwLock<
                std::collections::HashMap<String, lingshu_knowledge_graph::KnowledgeGraph>,
            >,
        > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
        tracing::info!(path = %db_path.display(), "graph store initialized");

        // ── Federation ─────────────────────────────────────────
        let federation = {
            let fed_config = lingshu_federation::FederationConfig {
                enabled: !cli.no_federation,
                listen_addr: format!("0.0.0.0:{}", cli.federation_port)
                    .parse()
                    .unwrap_or_else(|_| "0.0.0.0:9550".parse().unwrap()),
                ..Default::default()
            };
            std::sync::Arc::new(lingshu_federation::Federation::new(LsId::new(), fed_config).await)
        };

        // ── Eval store ──────────────────────────────────────────
        let eval_store = std::sync::Arc::new(tokio::sync::RwLock::new(None));

        // ── Initialize credential vault (encrypted SQLite) ───
        let cred_db_path = data_dir.join("credentials.db");
        let master_key = std::env::var("LINGSHU_CREDENTIAL_MASTER_KEY")
            .unwrap_or_else(|_| "lingshu-default-master-key-change-me".to_string());
        let credential_store = std::sync::Arc::new(
            CredentialStore::open(&cred_db_path, &master_key)
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "failed to open credential store, using in-memory fallback");
                    // In-memory fallback: use a temp path
                    CredentialStore::open(
                        &std::path::PathBuf::from("/tmp/lingshu-credentials-fallback.db"),
                        &master_key,
                    ).expect("in-memory credential store")
                })
        );
        let credential_manager = std::sync::Arc::new(CredentialManager::new(credential_store));
        tracing::info!(path = %cred_db_path.display(), "credential vault initialized");

        // ── Optional: chidori recovery (feature = "chidori") ────
        #[cfg(feature = "chidori")]
        let chidori_recovery = Some(std::sync::Arc::new(
            lingshu_runtime::ChidoriRecoveryManager::new(
                lingshu_runtime::CheckpointConfig::default(),
            ),
        ));
        #[cfg(not(feature = "chidori"))]
        let chidori_recovery: Option<
            std::sync::Arc<lingshu_runtime::ChidoriRecoveryManager>,
        > = None;

        // ── Optional: AutoAgents orchestrator (feature = "autoagents") ──
        #[cfg(feature = "autoagents")]
        let autoagents = Some(std::sync::Arc::new(
            lingshu_orchestrator::AutoAgentsOrchestrator::new(
                lingshu_orchestrator::OrchestratorConfig::default(),
            ),
        ));
        #[cfg(not(feature = "autoagents"))]
        let autoagents: Option<
            std::sync::Arc<lingshu_orchestrator::AutoAgentsOrchestrator>,
        > = None;

        // ── Optional: Loong adapter (feature = "loong") ──────────
        #[cfg(feature = "loong")]
        let loong_adapter = Some(std::sync::Arc::new(
            lingshu_orchestrator::LoongAdapter::new(),
        ));
        #[cfg(not(feature = "loong"))]
        let loong_adapter: Option<std::sync::Arc<lingshu_orchestrator::LoongAdapter>> = None;

        // ── 通道注册表 (Channel Registry) ────────────────────────
        let channel_registry = {
            let reg = Arc::new(ChannelRegistry::new());

            // Telegram (环境变量: LINGSHU_TELEGRAM_BOT_TOKEN)
            #[cfg(feature = "telegram")]
            if let Ok(token) = std::env::var("LINGSHU_TELEGRAM_BOT_TOKEN") {
                let ch = Arc::new(lingshu_channel::TelegramChannel::new(token));
                reg.register(ch).await;
                tracing::info!("channel registered: telegram");
            }

            // 飞书 (环境变量: LINGSHU_FEISHU_APP_ID + LINGSHU_FEISHU_APP_SECRET)
            #[cfg(feature = "feishu")]
            if let (Ok(app_id), Ok(app_secret)) = (
                std::env::var("LINGSHU_FEISHU_APP_ID"),
                std::env::var("LINGSHU_FEISHU_APP_SECRET"),
            ) {
                let ch = Arc::new(lingshu_channel::FeishuChannel::new(app_id, app_secret));
                reg.register(ch).await;
                tracing::info!("channel registered: feishu");
            }

            // QQ (环境变量: LINGSHU_QQ_APP_ID + LINGSHU_QQ_BOT_TOKEN)
            #[cfg(feature = "qq")]
            if let (Ok(app_id), Ok(bot_token)) = (
                std::env::var("LINGSHU_QQ_APP_ID"),
                std::env::var("LINGSHU_QQ_BOT_TOKEN"),
            ) {
                let ch = Arc::new(lingshu_channel::QqChannel::new(app_id, bot_token));
                reg.register(ch).await;
                tracing::info!("channel registered: qq");
            }

            // 微信 (环境变量: LINGSHU_WECHAT_APP_ID + LINGSHU_WECHAT_APP_SECRET + LINGSHU_WECHAT_TOKEN)
            #[cfg(feature = "wechat")]
            if let (Ok(app_id), Ok(app_secret), Ok(token)) = (
                std::env::var("LINGSHU_WECHAT_APP_ID"),
                std::env::var("LINGSHU_WECHAT_APP_SECRET"),
                std::env::var("LINGSHU_WECHAT_TOKEN"),
            ) {
                let ch = Arc::new(lingshu_channel::WeChatChannel::new(
                    app_id, app_secret, token,
                ));
                reg.register(ch).await;
                tracing::info!("channel registered: wechat");
            }

            // Discord (环境变量: DISCORD_BOT_TOKEN)
            #[cfg(feature = "discord")]
            if let Ok(_token) = std::env::var("DISCORD_BOT_TOKEN") {
                let ch = Arc::new(lingshu_channel::DiscordChannel::new().unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "failed to create Discord channel");
                    panic!("DiscordChannel::new() failed: {e}")
                }));
                reg.register(ch).await;
                tracing::info!("channel registered: discord");
            }

            reg
        };

        // ── v4.0 Agent Runtime ──
        let agent_rt_config = lingshu_runtime::agent_runtime::AgentRuntimeConfig {
            name: "lingshu".into(),
            session_ttl_seconds: config.runtime.session_ttl_seconds,
            ..Default::default()
        };
        let agent_runtime =
            lingshu_runtime::agent_runtime::AgentRuntime::new(agent_rt_config).await?;
        tracing::info!("agent runtime (v4.0) initialized");

        // ── v6.0 Pipeline Wiring: 将 AgentPipeline 接入 AgentRuntime ──
        if let Some(ref llm_backend) = llm {
            // 1. 构建默认 ReAct Pipeline
            let tool_registry_for_pipeline = tool_registry.clone();
            let llm_for_pipeline = llm_backend.clone();
            let memory_for_pipeline: Option<Arc<dyn lingshu_traits::memory::Memory>> = None;
            let pipeline = Arc::new(
                lingshu_runtime::agent_pipeline::AgentPipeline::default_react(
                    llm_for_pipeline,
                    config.llm.default_model.clone(),
                    tool_registry_for_pipeline,
                    memory_for_pipeline,
                )
                .with_max_iterations(10),
            );

            // 2. 注册到 AgentRuntime
            agent_runtime.set_tool_registry(tool_registry.clone()).await;
            agent_runtime.set_pipeline(pipeline.clone()).await;
            agent_runtime
                .set_event_bus(event_bus.clone() as Arc<dyn lingshu_traits::event_bus::EventBus>)
                .await;

            // 3. 注册默认 PipelineAgent 到 AgentManager
            let pipeline_agent = Box::new(lingshu_runtime::agent_pipeline::PipelineAgent::new(
                lingshu_core::LsId::new(),
                "default-agent",
                pipeline,
            ));
            agent_manager
                .register(
                    lingshu_core::LsId::new(),
                    "default-agent".into(),
                    pipeline_agent,
                )
                .await;

            tracing::info!("v6.0 pipeline wired: default-agent registered with ReAct pipeline");
        } else {
            tracing::warn!("v6.0 pipeline wiring skipped: no LLM backend available");
        }

        // ── v6.0 WorkflowAccess Wiring ──
        let workflow_access =
            Arc::new(lingshu_runtime::workflow_access::RuntimeWorkflowAccess::new());
        // 注册内置工作流
        workflow_access
            .register(
                "default-reason",
                serde_json::json!({
                    "name": "default-reason",
                    "description": "标准推理工作流：思考 → 行动 → 观察 → 回答",
                    "stages": ["think", "act", "observe", "respond"],
                }),
            )
            .await;
        workflow_access
            .register(
                "rag-retrieve",
                serde_json::json!({
                    "name": "rag-retrieve",
                    "description": "RAG 检索工作流：检索 → 注入 → 推理 → 回答",
                    "stages": ["retrieve", "inject", "think", "respond"],
                }),
            )
            .await;
        agent_runtime.set_workflow_access(workflow_access.clone() as Arc<dyn lingshu_runtime::WorkflowAccess>).await;

        // ── v6.0 Swarm Engine Wiring (if available) ──
        #[cfg(feature = "swarm")]
        let swarm_engine: Option<
            std::sync::Arc<tokio::sync::RwLock<lingshu_swarm::engine::SwarmEngine>>,
        > = {
            let swarm_cfg = lingshu_swarm::types::SwarmConfig {
                name: "default-swarm".into(),
                min_agents: 2,
                max_agents: 8,
                enable_emergent_specialization: true,
                ..Default::default()
            };
            let engine = lingshu_swarm::engine::SwarmEngine::new(swarm_cfg);
            engine.start().await.ok();
            tracing::info!("v6.0 swarm engine initialized");
            Some(Arc::new(tokio::sync::RwLock::new(engine)))
        };
        #[cfg(not(feature = "swarm"))]
        let _swarm_engine: Option<std::sync::Arc<tokio::sync::RwLock<()>>> = None;

        // ── v6.0 Autonomy Evolution Engine Wiring (if available) ──
        #[cfg(feature = "autonomy")]
        let evolution_engine: Option<
            std::sync::Arc<lingshu_autonomy::evolution::EvolutionEngine>,
        > = {
            let exp_store = Arc::new(lingshu_autonomy::experience::ExperienceStore::new(1000));
            let reflection_cfg = lingshu_autonomy::reflection::ReflectionConfig::default();
            let reflection_engine = Arc::new(lingshu_autonomy::reflection::ReflectionEngine::new(
                reflection_cfg,
                exp_store.clone(),
            ));
            let evol_cfg = lingshu_autonomy::evolution::EvolutionConfig {
                auto_apply_threshold: 8,
                cooldown: std::time::Duration::from_secs(300),
                enable_auto_rollback: true,
                ..Default::default()
            };
            let engine = lingshu_autonomy::evolution::EvolutionEngine::new(
                evol_cfg,
                exp_store,
                reflection_engine,
            );
            tracing::info!("v6.0 autonomy evolution engine initialized");
            Some(Arc::new(engine))
        };
        #[cfg(not(feature = "autonomy"))]
        let _evolution_engine: Option<std::sync::Arc<()>> = None;

        let runtime = Self {
            lifecycle,
            start_time: std::time::Instant::now(),
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
            agent_manager,
            agent_runtime: Some(agent_runtime),
            #[cfg(feature = "swarm")]
            swarm_engine,
            #[cfg(feature = "autonomy")]
            evolution_engine,
            workflow_access: Some(workflow_access),
            memory_manager: lingshu_memory::SessionMemoryManager::default(),
            mcp_server: {
                let mut mcp_server = lingshu_mcp::McpServer::new();
                let credential_tools = lingshu_mcp::credential_tools::create_credential_tools(
                    credential_manager.clone(),
                );
                mcp_server.register_tools(credential_tools);
                Arc::new(mcp_server)
            },
            graph_store: graph_store.clone(),
            graph_cache: graph_cache.clone(),
            rate_limiter: std::sync::Arc::new(lingshu_ratelimit::MultiRateLimiter::new()),
            audit_log: {
                #[cfg(feature = "audit-sqlite")]
                let log: std::sync::Arc<dyn lingshu_audit::AuditLogStore> = {
                    match lingshu_audit::SqliteAuditLog::new(&audit_db_path) {
                        Ok(sqlite) => std::sync::Arc::new(sqlite),
                        Err(e) => {
                            tracing::warn!(error = %e, path = %audit_db_path.display(), "failed to open SQLite audit log, falling back to in-memory");
                            std::sync::Arc::new(lingshu_audit::AuditLog::new())
                        }
                    }
                };
                #[cfg(not(feature = "audit-sqlite"))]
                let log: std::sync::Arc<dyn lingshu_audit::AuditLogStore> =
                    std::sync::Arc::new(lingshu_audit::AuditLog::new());
                log
            },
            prompt_registry: std::sync::Arc::new(lingshu_prompt::PromptRegistry::new()),
            billing: std::sync::Arc::new(
                lingshu_billing::BillingSystem::new(vec![]).unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "failed to create billing system, using defaults");
                    // Provide a default BillingSystem with no plans
                    let tracker = std::sync::Arc::new(lingshu_billing::UsageTracker::new());
                    let quota_mgr = std::sync::Arc::new(lingshu_billing::QuotaManager::new(vec![]));
                    let report_gen =
                        std::sync::Arc::new(lingshu_billing::ReportGenerator::new(tracker.clone()));
                    lingshu_billing::BillingSystem {
                        tracker,
                        quota_manager: quota_mgr,
                        report_generator: report_gen,
                    }
                }),
            ),
            credential_manager,
            eval_store,
            federation,
            chidori_recovery,
            autoagents,
            loong_adapter,
            channel_registry,
            config_rx: {
                let (config_tx, config_rx) = tokio::sync::broadcast::channel(16);
                let (std_tx, std_rx) =
                    std::sync::mpsc::channel::<lingshu_config::settings::ConfigEvent>();
                let _watcher = lingshu_config::settings::ConfigWatcher::spawn(&cli.env, std_tx);
                let std_rx = std::sync::Arc::new(std::sync::Mutex::new(std_rx));
                tokio::spawn(async move {
                    loop {
                        let rx = std_rx.clone();
                        let event =
                            tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await;
                        match event {
                            Ok(Ok(evt)) => {
                                let _ = config_tx.send(evt);
                            }
                            _ => break,
                        }
                    }
                });
                config_rx
            },
        };

        // Load persisted graphs from SQLite cache into memory
        match graph_store.load_all().await {
            Ok(cached) => {
                let mut cache = graph_cache.write().await;
                *cache = cached;
                tracing::info!("restored {} graphs from SQLite store", cache.len());
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to load cached graphs from store");
            }
        }

        // ── Register default tools ──────────────────────────────
        {
            use lingshu_code_analyzer::CodeAnalysisTool;
            let registry = runtime.tool_registry.write().await;

            // 基础工具 (with default permissions)
            registry
                .register(Box::new(lingshu_backends::tools::ListDirTool::new(None)))
                .await;
            registry
                .register(Box::new(lingshu_backends::tools::FileReadTool::new(None)))
                .await;
            registry
                .register(Box::new(lingshu_backends::tools::FileWriteTool::new(None)))
                .await;
            registry
                .register(Box::new(lingshu_backends::tools::ShellTool::new(None)))
                .await;
            registry
                .register(Box::new(lingshu_backends::tools::CalculatorTool))
                .await;
            registry
                .register(Box::new(lingshu_backends::tools::CurrentTimeTool))
                .await;

            // 代码分析工具 — 如果 LLM 可用则启用语义 enrichment
            let code_tool = if let Some(ref llm) = runtime.llm {
                let llm_config = lingshu_code_analyzer::LlmAnalyzerConfig {
                    model: runtime.config.llm.default_model.clone(),
                    ..Default::default()
                };
                let analyzer = Arc::new(lingshu_code_analyzer::LlmAnalyzer::new(
                    llm.clone(),
                    llm_config,
                ));
                CodeAnalysisTool::new().with_llm(analyzer)
            } else {
                CodeAnalysisTool::new()
            };
            registry.register(Box::new(code_tool)).await;

            info!("registered {} tools", registry.count().await);
        }

        runtime
            .lifecycle
            .transition(&runtime.root_ctx, LifecycleState::Running)
            .map_err(|e| LsError::Internal(format!("lifecycle startup failed: {e}")))?;

        info!("lingshu runtime initialized successfully");
        Ok(runtime)
    }

    /// 优雅关闭
    pub async fn shutdown(&self) -> LsResult<()> {
        self.lifecycle
            .transition(&self.root_ctx, LifecycleState::ShuttingDown)
            .map_err(|e| LsError::Internal(format!("lifecycle shutdown failed: {e}")))?;

        self.scheduler.pause();
        info!("scheduler paused");

        self.lifecycle
            .transition(&self.root_ctx, LifecycleState::Stopped)
            .map_err(|e| LsError::Internal(format!("lifecycle stop failed: {e}")))?;

        // 优雅关闭联邦服务
        // 优雅关闭 Agent Runtime
        if let Some(ref rt) = self.agent_runtime {
            let _ = rt.shutdown().await;
            info!("agent runtime shut down");
        }
        self.federation.stop().await;

        info!("lingshu runtime shut down gracefully");
        Ok(())
    }

    /// 创建用户会话
    pub async fn create_session(&self, user_id: &str) -> LsResult<LsId> {
        let session_id = LsId::new();
        let ctx = LsContext::with_session(session_id).with_user(user_id);
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
                println!(
                    "  /channels         List registered channels
  /channel <id> <cmd> Channel commands (send/health)
  /llm <prompt>    Send prompt to LLM"
                );
                println!("  <any text>       Chat with the agent");
            }
            "/stats" => {
                println!("Runtime Statistics:");
                println!("  Lifecycle:    {:?}", runtime.lifecycle.current());
                println!(
                    "  Sessions:     {}",
                    runtime.session_mgr.active_count().await
                );
                println!("  Tasks:        {}", runtime.scheduler.task_count().await);
                println!(
                    "  Events:       {}",
                    runtime.event_bus.history().await.len()
                );
                let healing = runtime.recovery.is_circuit_open();
                println!(
                    "  Circuit:      {}",
                    if healing { "⚠ OPEN" } else { "✓ closed" }
                );
                println!("  Storage:      {}", runtime.storage.base_path().display());
            }
            "/session" => {
                let sessions = runtime.session_mgr.list_all().await;
                println!("Active Sessions ({}):", sessions.len());
                for s in &sessions {
                    println!(
                        "  {} | user={:?} | state={:?} | created={}",
                        s.session_id, s.user_id, s.state, s.created_at
                    );
                }
            }
            "/channels" => {
                let channels = runtime.channel_registry.list_meta().await;
                if channels.is_empty() {
                    println!("No channels registered.");
                    println!("  Set env vars to enable:");
                    println!("  - LINGSHU_TELEGRAM_BOT_TOKEN  → Telegram");
                    println!("  - LINGSHU_FEISHU_APP_ID + LINGSHU_FEISHU_APP_SECRET → Feishu");
                    println!("  - LINGSHU_QQ_APP_ID + LINGSHU_QQ_BOT_TOKEN → QQ");
                    println!("  - LINGSHU_WECHAT_APP_ID + LINGSHU_WECHAT_APP_SECRET + LINGSHU_WECHAT_TOKEN → WeChat");
                } else {
                    println!("Registered Channels ({}):", channels.len());
                    for (id, meta) in &channels {
                        println!("  {} — {} ({})", id, meta.label, meta.description);
                    }
                }
            }
            cmd if cmd.starts_with("/channel ") => {
                let parts: Vec<&str> = cmd.splitn(3, " ").collect();
                let channel_id = parts.get(1).unwrap_or(&"");
                let subcmd = parts.get(2).unwrap_or(&"");
                match *subcmd {
                    "health" => match runtime.channel_registry.get(channel_id).await {
                        Some(ch) => match ch.health_check().await {
                            Ok(status) => {
                                println!(
                                    "Channel [{}] health: {}",
                                    ch.id(),
                                    if status.healthy {
                                        "✓ healthy"
                                    } else {
                                        "✗ unhealthy"
                                    }
                                );
                                if let Some(ms) = status.latency_ms {
                                    println!("  Latency: {ms}ms");
                                }
                                if let Some(err) = &status.error {
                                    println!("  Error: {err}");
                                }
                            }
                            Err(e) => println!("⚠ Health check failed: {e}"),
                        },
                        None => println!("⚠ Unknown channel: {channel_id}"),
                    },
                    _ if subcmd.starts_with("send ") => {
                        let send_args: Vec<&str> = subcmd.splitn(3, " ").collect();
                        let target = send_args.get(1).unwrap_or(&"");
                        let text = send_args.get(2).unwrap_or(&"");
                        if target.is_empty() || text.is_empty() {
                            println!("Usage: /channel <id> send <target> <message>");
                        } else {
                            match runtime.channel_registry.get(channel_id).await {
                                Some(ch) => {
                                    use lingshu_channel::types::SendTextContext;
                                    let ctx = SendTextContext {
                                        to: target.to_string(),
                                        text: text.to_string(),
                                        reply_to_id: None,
                                        thread_id: None,
                                        silent: false,
                                        account_id: None,
                                    };
                                    match ch.send_text(ctx).await {
                                        Ok(receipt) => {
                                            println!("✓ Sent! message_id={}", receipt.message_id)
                                        }
                                        Err(e) => println!("⚠ Send failed: {e}"),
                                    }
                                }
                                None => println!("⚠ Unknown channel: {channel_id}"),
                            }
                        }
                    }
                    _ => {
                        println!("Usage:");
                        println!("  /channel <id> health          Check channel health");
                        println!("  /channel <id> send <t> <msg>  Send message");
                    }
                }
            }

            cmd if cmd.starts_with("/llm ") || !cmd.starts_with('/') => {
                let prompt = if cmd.starts_with("/llm ") {
                    cmd.trim_start_matches("/llm ")
                } else {
                    cmd
                };

                if let Some(llm) = &runtime.llm {
                    let child_ctx = ctx.child();
                    let request = lingshu_traits::llm::LlmRequest {
                        model: runtime.config.llm.default_model.clone(),
                        messages: vec![lingshu_traits::llm::LlmMessage {
                            role: lingshu_traits::llm::LlmRole::User,
                            content: prompt.to_string(),
                            content_parts: None,
                            name: None,
                            tool_calls: None,
                        }],
                        temperature: Some(0.7),
                        max_tokens: Some(runtime.config.llm.max_tokens),
                        tools: None,
                        stream: true,
                    };

                    match llm.invoke_stream(child_ctx, request).await {
                        Ok(mut rx) => {
                            print!("\n🤖 ");
                            std::io::stdout().flush().ok();
                            while let Some(chunk_result) = rx.recv().await {
                                match chunk_result {
                                    Ok(chunk) => {
                                        if let Some(ref content) = chunk.content {
                                            print!("{}", content);
                                            std::io::stdout().flush().ok();
                                        }
                                        if let Some(ref reason) = chunk.finish_reason {
                                            println!("\n\n[finish_reason: {reason}]");
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("\n⚠ Stream error: {e}");
                                        break;
                                    }
                                }
                            }
                            println!();
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
    let health_registry = Arc::new(lingshu_observability::health::HealthRegistry::new(
        "lingshu", "1.0.0",
    ));

    // Register built-in health checks
    {
        use lingshu_observability::health::RuntimeHealth;
        use std::sync::Arc;

        let (_, ready_rx) = tokio::sync::watch::channel(true);
        let runtime_check = RuntimeHealth::new("runtime", Arc::new(ready_rx));
        health_registry.register(Box::new(runtime_check)).await;
    }

    // Register LLM health check
    {
        let llm = runtime.llm.clone();
        let check = lingshu_observability::health::RuntimeHealth::new(
            "llm",
            Arc::new(tokio::sync::watch::channel(llm.is_some()).1),
        );
        health_registry.register(Box::new(check)).await;
    }

    // Register storage health check
    {
        let _storage_path = format!("{}", runtime.storage.base_path().display());
        let check = lingshu_observability::health::RuntimeHealth::new(
            "storage",
            Arc::new(tokio::sync::watch::channel(true).1),
        );
        health_registry.register(Box::new(check)).await;
    }

    // Register federation health check (if enabled)
    {
        let fed_enabled = runtime.federation.config.enabled;
        let check = lingshu_observability::health::RuntimeHealth::new(
            "federation",
            Arc::new(tokio::sync::watch::channel(fed_enabled).1),
        );
        health_registry.register(Box::new(check)).await;
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

    let plugin_event_bus = Arc::new(lingshu_plugin::event::EventBus::new());
    runtime
        .agent_manager
        .set_event_bus(plugin_event_bus.clone())
        .await;
    runtime
        .federation
        .set_event_bus(plugin_event_bus.clone())
        .await;
    // 启动 Agent Runtime (v4.0)
    if let Some(ref rt) = runtime.agent_runtime {
        rt.start().await?;
        info!("agent runtime (v4.0) started");
    }
    let state = Arc::new(AppState {
        runtime: runtime.clone(),
        plugin_event_bus: plugin_event_bus.clone(),
        plugin_registry: Arc::new(lingshu_plugin::PluginRegistry::with_event_bus(
            plugin_event_bus.clone(),
        )),
        plugin_market: tokio::sync::RwLock::new(lingshu_plugin::market::PluginMarket::new(
            vec![lingshu_plugin::market::RegistrySource::GitHubReleases(
                "lingshu-org/lingshu-plugins".into(),
            )],
            std::path::PathBuf::from("plugins"),
        )),
        hot_reload_watcher: lingshu_plugin::hot_reload::HotReloadWatcher::new(
            std::path::PathBuf::from("plugins"),
        ),
        beef_manager: Arc::new(tokio::sync::RwLock::new(None)),
        watch_manager: Arc::new(tokio::sync::RwLock::new(None)),
        health_registry,
        ws_manager: Arc::new(ConnectionManager::new(300)),
        sse_broadcaster: Arc::new(SseBroadcaster::new(1024)),
        file_store: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        credential_manager: runtime.credential_manager.clone(),
        tenant_manager: std::sync::Arc::new(lingshu_tenant::TenantManager::new()),
        vault_client: std::sync::Arc::new(lingshu_vault::MockVaultClient::new()),
        tee_system: std::sync::Arc::new(
            lingshu_tee::TeeSystem::initialize()
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("Failed to initialize TEE system: {e}, using fallback");
                    lingshu_tee::TeeSystem {
                        platform: lingshu_tee::TeePlatform::None,
                        sgx: None,
                        tdx: None,
                        encrypted_memory: std::sync::Arc::new(
                            lingshu_tee::EncryptedMemoryRegion::new(),
                        ),
                        policy_engine: std::sync::Arc::new(std::sync::RwLock::new(
                            lingshu_tee::TeePolicyEngine::default(),
                        )),
                    }
                }),
        ),
        jwt_service: lingshu_security::auth::JwtService::from_env_or("lingshu-dev-secret", 86400),
    });
    // 桥接 MCP 进度通知到 SSE
    runtime.mcp_server.bridge_sse(state.sse_broadcaster.clone());

    // 桥接 Plugin EventBus 到 SSE — 所有插件生命周期事件自动推送到 WebUI
    {
        let sse = state.sse_broadcaster.clone();
        let eb = state.plugin_event_bus.clone();
        tokio::spawn(async move {
            use lingshu_plugin::event::{Event, EventCallback, EventType};
            use lingshu_websocket::types::SseEvent;
            let cb: EventCallback = Arc::new(move |event: Event| {
                let data = serde_json::json!({
                    "type": format!("{:?}", event.event_type),
                    "source": event.source,
                    "payload": event.payload,
                    "timestamp": event.timestamp.to_rfc3339(),
                });
                let sse_event = SseEvent::new("plugin:event", data);
                sse.publish(sse_event);
            });
            eb.registrar()
                .register(EventType::PluginInstalled, cb.clone(), "sse-bridge")
                .await;
            eb.registrar()
                .register(EventType::PluginLoaded, cb.clone(), "sse-bridge")
                .await;
            eb.registrar()
                .register(EventType::PluginStarted, cb.clone(), "sse-bridge")
                .await;
            eb.registrar()
                .register(EventType::PluginStopped, cb.clone(), "sse-bridge")
                .await;
            eb.registrar()
                .register(EventType::PluginUninstalled, cb, "sse-bridge")
                .await;
        });
    }

    // 自动注册 BeEF 安全测试插件
    {
        let beef_manager = state.beef_manager.clone();
        let plugin_registry = state.plugin_registry.clone();
        let _plugin_event_bus = state.plugin_event_bus.clone();
        tokio::spawn(async move {
            match plugin_registry
                .register(
                    Box::new(lingshu_beef_plugin::BeefPlugin::new(
                        std::path::PathBuf::from("beef"),
                    )),
                    None,
                )
                .await
            {
                Ok(id) => {
                    *beef_manager.write().await =
                        Some(Arc::new(lingshu_beef_plugin::BeefManager::new(
                            std::path::PathBuf::from("beef"),
                            "ruby",
                            3000,
                        )));
                    tracing::info!(plugin_id = %id, "BeEF security testing plugin registered");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to register BeEF plugin (non-fatal)");
                }
            }
        });
    }

    // 自动注册 Watch Skill 视频分析插件
    {
        let watch_manager = state.watch_manager.clone();
        let plugin_registry = state.plugin_registry.clone();
        let _plugin_event_bus = state.plugin_event_bus.clone();
        tokio::spawn(async move {
            let plugin = lingshu_watch_plugin::WatchPlugin::new("python3");
            let manager = plugin.manager().clone();
            match plugin_registry.register(Box::new(plugin), None).await {
                Ok(id) => {
                    *watch_manager.write().await = Some(manager);
                    tracing::info!(plugin_id = %id, "Watch Skill video analysis plugin registered");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to register Watch plugin (non-fatal)");
                }
            }
        });
    }

    // 自动注册 RAG 文档检索插件
    {
        let plugin_registry = state.plugin_registry.clone();
        tokio::spawn(async move {
            let plugin = RagPlugin::default();
            match plugin_registry.register(Box::new(plugin), None).await {
                Ok(id) => {
                    tracing::info!(plugin_id = %id, "RAG document retrieval plugin registered");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to register RAG plugin (non-fatal)");
                }
            }
        });
    }
    let app = api::build_router(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
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
    println!("   📋 GET  /docs");
    println!("   📋 GET  /admin");
    println!("   📋 POST /v1/eval/run");
    println!("   📋 GET  /v1/federation/status");

    axum::serve(listener, app)
        .await
        .map_err(|e| LsError::Internal(format!("server error: {e}")))?;

    Ok(())
}

/// LingShu 运行模式
///
/// 决定启动路径和生命周期管理策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunMode {
    /// 服务端模式 — HTTP + MCP + Federation (默认)
    Server,
    /// 终端 TUI 模式 — 独占终端，不启动后台服务
    Cil,
    /// 批处理模式 (预留)
    #[allow(dead_code)]
    Batch,
}

/// 根据 CLI 参数检测运行模式
fn detect_mode(cli: &Cli) -> RunMode {
    if cli.cil {
        RunMode::Cil
    } else {
        RunMode::Server
    }
}

#[tokio::main]
async fn main() -> LsResult<()> {
    // 自动加载 .env 文件 (如果存在)
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match detect_mode(&cli) {
        RunMode::Cil => {
            // ── Terminal Runtime Path ──
            // 不启动: federation, config watcher, HTTP/MCP server, plugins
            // 使用 CilProfile 日志 (写文件，不污染终端)
            lingshu_observability::tracing::init_cil_logging()
                .map_err(|e| LsError::Internal(format!("CIL logging init failed: {}", e)))?;
            // Phase 3: 追加 TerminalGuard RAII
            cli::run_cil(None)
                .map_err(|e| LsError::Internal(format!("CIL error: {}", e)))?;
            info!("lingshu exited cleanly");
            Ok(())
        }

        RunMode::Server => {
            let runtime = Arc::new(LingshuRuntime::initialize(&cli).await?);
            // 启动联邦服务
            runtime.federation.start().await?;
            if !cli.no_federation {
                info!("federation started on port {}", cli.federation_port);
            } else {
                info!("federation disabled by --no-federation flag");
            }

            // 配置热重载消费 — 监听 ConfigEvent 并应用到运行时
            {
                let runtime = runtime.clone();
                let mut rx = runtime.config_rx.resubscribe();
                tokio::spawn(async move {
                    use lingshu_config::settings::ConfigEvent;
                    while let Ok(event) = rx.recv().await {
                        match event {
                            ConfigEvent::Reloaded(ref new_config) => {
                                info!(model = %new_config.llm.default_model, "config reloaded");
                            }
                            ConfigEvent::Error(msg) => {
                                error!("config reload error: {msg}");
                            }
                        }
                    }
                });
            }

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

        RunMode::Batch => {
            // 预留: 批处理模式
            Err(LsError::Internal("Batch mode not yet implemented".into()))
        }
    }
}
