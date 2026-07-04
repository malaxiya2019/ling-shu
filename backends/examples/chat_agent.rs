//! 🚀 ChatAgent — Lingshu 端到端集成示例
//!
//! 串联: SessionManager → EventBus → LLM → Memory → Agent
//!
//! 运行:
//!   cargo run --example chat_agent -p lingshu-backends
//!
//! 环境变量:
//!   OPENAI_API_KEY  设置后使用 GPT-4o-mini
//!   未设置则使用 Mock LLM (不依赖外部 API)

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::info;

use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::event_bus::Event as BusEvent;
use lingshu_eventbus::topic::EventTopic;
use lingshu_runtime::session::SessionManager;
use lingshu_traits::agent::{Agent, AgentOutput, AgentSnapshot, AgentStatus};
use lingshu_traits::event_bus::EventBus;
use lingshu_traits::llm::{Llm, LlmChunk, LlmMessage, LlmRequest, LlmResponse, LlmRole, LlmUsage};
use lingshu_traits::memory::{Memory, MemoryItem, MemorySearchResult};

// ═══════════════════════════════════════════════════
// 1. Mock LLM — 不依赖外部 API 的模拟引擎
// ═══════════════════════════════════════════════════

pub struct MockLlm {
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
}

impl MockLlm {
    pub fn new() -> Self {
        Self { prompt_tokens: AtomicU64::new(0), completion_tokens: AtomicU64::new(0) }
    }
}

#[async_trait]
impl Llm for MockLlm {
    async fn invoke(&self, _ctx: LsContext, request: LlmRequest) -> LsResult<LlmResponse> {
        info!(model = %request.model, messages = %request.messages.len(), "mock llm invoke");
        let user_msg = request.messages.iter()
            .rev().find(|m| matches!(m.role, LlmRole::User))
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let reply_text = format!(
            "[Mock] 你好！你刚才说的是: 「{0}」\n我是 Lingshu 的 Mock LLM，正在验证端到端集成。",
            user_msg
        );
        let reply_len = reply_text.len() as u64;

        self.prompt_tokens.fetch_add(10, Ordering::AcqRel);
        self.completion_tokens.fetch_add(reply_len, Ordering::AcqRel);

        Ok(LlmResponse {
            message: LlmMessage {
                role: LlmRole::Assistant,
                content: reply_text,
                name: None,
                tool_calls: None,
            },
            finish_reason: "stop".into(),
            usage: LlmUsage {
                prompt_tokens: 10,
                completion_tokens: reply_len,
                total_tokens: 10 + reply_len,
            },
        })
    }

    async fn invoke_stream(
        &self,
        _ctx: LsContext,
        _request: LlmRequest,
    ) -> LsResult<mpsc::Receiver<LsResult<LlmChunk>>> {
        Err(LsError::NotImplemented("mock stream".into()))
    }

    async fn usage_stats(&self, _ctx: LsContext) -> LsResult<HashMap<String, u64>> {
        let mut map = HashMap::new();
        map.insert("prompt_tokens".into(), self.prompt_tokens.load(Ordering::Acquire));
        map.insert("completion_tokens".into(), self.completion_tokens.load(Ordering::Acquire));
        Ok(map)
    }
}

// ═══════════════════════════════════════════════════
// 2. InMemoryEventBus — 内存事件总线
// ═══════════════════════════════════════════════════

type BoxedHandler = Box<dyn Fn(lingshu_traits::event_bus::Event) -> LsResult<()> + Send + Sync>;

pub struct InMemoryEventBus {
    subscribers: RwLock<Vec<(String, String, BoxedHandler)>>,
    published: Arc<Mutex<Vec<BusEvent>>>,
}

impl InMemoryEventBus {
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(Vec::new()),
            published: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[allow(dead_code)]
    pub async fn history(&self) -> Vec<lingshu_traits::event_bus::Event> {
        self.published.lock().await.clone()
    }

    fn topic_matches(pattern: &str, topic: &str) -> bool {
        if pattern == "*" || pattern == topic {
            return true;
        }
        if let Some(prefix) = pattern.strip_suffix(".*") {
            return topic.starts_with(prefix);
        }
        false
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn publish(&self, _ctx: LsContext, event: BusEvent) -> LsResult<()> {
        self.published.lock().await.push(event.clone());
        let subscribers = self.subscribers.read().await;
        for (_, pattern, handler) in subscribers.iter() {
            if Self::topic_matches(pattern, &event.topic) {
                let _ = handler(event.clone());
            }
        }
        Ok(())
    }

    async fn publish_batch(&self, ctx: LsContext, events: Vec<BusEvent>) -> LsResult<()> {
        for event in events {
            self.publish(ctx.clone(), event).await?;
        }
        Ok(())
    }

    async fn subscribe(&self, _ctx: LsContext, topic_pattern: &str, handler: BoxedHandler) -> LsResult<String> {
        let id = LsId::new().to_string();
        self.subscribers.write().await.push((id.clone(), topic_pattern.to_string(), handler));
        Ok(id)
    }

    async fn unsubscribe(&self, _ctx: LsContext, subscription_id: &str) -> LsResult<()> {
        let mut subs = self.subscribers.write().await;
        subs.retain(|(id, _, _)| id != subscription_id);
        Ok(())
    }

    async fn list_subscriptions(&self, _ctx: LsContext) -> LsResult<Vec<lingshu_traits::event_bus::SubscriptionInfo>> {
        let subs = self.subscribers.read().await;
        Ok(subs.iter().map(|(id, pattern, _)| {
            lingshu_traits::event_bus::SubscriptionInfo {
                id: id.clone(),
                topic_pattern: pattern.clone(),
                created_at: chrono::Utc::now(),
            }
        }).collect())
    }
}

// ═══════════════════════════════════════════════════
// 2. InMemoryMemory — 基于 HashMap 的记忆存储
// ═══════════════════════════════════════════════════

pub struct InMemoryMemory {
    items: RwLock<Vec<MemoryItem>>,
}

impl InMemoryMemory {
    pub fn new() -> Self {
        Self { items: RwLock::new(Vec::new()) }
    }
}

#[async_trait]
impl Memory for InMemoryMemory {
    async fn write(&self, _ctx: LsContext, item: MemoryItem) -> LsResult<LsId> {
        let id = item.memory_id;
        self.items.write().await.push(item);
        Ok(id)
    }

    async fn write_batch(&self, _ctx: LsContext, items: Vec<MemoryItem>) -> LsResult<Vec<LsId>> {
        let ids: Vec<LsId> = items.iter().map(|i| i.memory_id).collect();
        self.items.write().await.extend(items);
        Ok(ids)
    }

    async fn read(&self, _ctx: LsContext, memory_id: LsId) -> LsResult<MemoryItem> {
        let items = self.items.read().await;
        items.iter()
            .find(|item| item.memory_id == memory_id)
            .cloned()
            .ok_or_else(|| LsError::NotFound(format!("memory {}", memory_id)))
    }

    async fn search(&self, _ctx: LsContext, query: &str, limit: u64) -> LsResult<MemorySearchResult> {
        let items = self.items.read().await;
        let q = query.to_lowercase();
        let mut results: Vec<MemoryItem> = items.iter()
            .filter(|item| {
                item.content.as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&q)
                    || item.metadata.values().any(|v| v.to_lowercase().contains(&q))
            })
            .cloned()
            .collect();
        results.reverse();
        let total = results.len() as u64;
        results.truncate(limit as usize);
        Ok(MemorySearchResult { items: results, total })
    }

    async fn delete(&self, _ctx: LsContext, memory_id: LsId) -> LsResult<()> {
        let mut items = self.items.write().await;
        let before = items.len();
        items.retain(|item| item.memory_id != memory_id);
        if items.len() == before {
            return Err(LsError::NotFound(format!("memory {}", memory_id)));
        }
        Ok(())
    }

    async fn clean_expired(&self, _ctx: LsContext) -> LsResult<u64> {
        let mut items = self.items.write().await;
        let before = items.len();
        let now = chrono::Utc::now();
        items.retain(|item| {
            match item.ttl_seconds {
                Some(ttl) => {
                    let expires = item.created_at + chrono::Duration::seconds(ttl as i64);
                    expires > now
                }
                None => true,
            }
        });
        Ok((before - items.len()) as u64)
    }

    async fn clear_session(&self, _ctx: LsContext, session_id: LsId) -> LsResult<()> {
        let mut items = self.items.write().await;
        items.retain(|item| item.session_id != session_id);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════
// 3. ChatAgent — 智能体构建器 + 执行引擎
// ═══════════════════════════════════════════════════

pub struct ChatAgentBuilder {
    llm: Option<Box<dyn Llm>>,
    memory: Option<Box<dyn Memory>>,
    event_bus: Option<Box<dyn EventBus>>,
    session_manager: Option<SessionManager>,
    system_prompt: String,
}

impl ChatAgentBuilder {
    pub fn new() -> Self {
        Self {
            llm: None,
            memory: None,
            event_bus: None,
            session_manager: None,
            system_prompt: "你是 Lingshu 智能助手，一个由灵枢系统驱动的 AI 助手。".into(),
        }
    }

    pub fn llm(mut self, llm: impl Llm + 'static) -> Self {
        self.llm = Some(Box::new(llm));
        self
    }

    pub fn llm_box(mut self, llm: Box<dyn Llm>) -> Self {
        self.llm = Some(llm);
        self
    }

    pub fn memory(mut self, memory: impl Memory + 'static) -> Self {
        self.memory = Some(Box::new(memory));
        self
    }

    pub fn memory_box(mut self, memory: Box<dyn Memory>) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn event_bus(mut self, bus: impl EventBus + 'static) -> Self {
        self.event_bus = Some(Box::new(bus));
        self
    }

    pub fn event_bus_box(mut self, bus: Box<dyn EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    pub fn session_manager(mut self, mgr: SessionManager) -> Self {
        self.session_manager = Some(mgr);
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn build(self) -> ChatAgent {
        ChatAgent {
            agent_id: LsId::new(),
            llm: self.llm,
            memory: self.memory,
            event_bus: self.event_bus,
            session_manager: self.session_manager.unwrap_or_else(|| SessionManager::new(3600)),
            system_prompt: self.system_prompt,
            status: RwLock::new(AgentStatus::Idle),
        }
    }
}

/// ChatAgent — 支持多轮对话的智能体.
pub struct ChatAgent {
    agent_id: LsId,
    llm: Option<Box<dyn Llm>>,
    memory: Option<Box<dyn Memory>>,
    event_bus: Option<Box<dyn EventBus>>,
    session_manager: SessionManager,
    system_prompt: String,
    status: RwLock<AgentStatus>,
}

impl ChatAgent {
    /// 发起一轮对话.
    pub async fn chat(&self, ctx: &LsContext, message: &str) -> LsResult<String> {
        info!(session_id = %ctx.session_id, user_msg = %message, "chat: received");

        // 1. 会话管理
        let _ = self.session_manager.create(ctx).await;

        // 2. 更新状态 + 发布事件
        *self.status.write().await = AgentStatus::Running;
        self.publish_event(ctx, EventTopic::task_submitted(), serde_json::json!({
            "agent_id": self.agent_id.to_string(),
            "message": message,
        })).await;

        // 3. 存储用户消息到记忆
        if let Some(ref memory) = self.memory {
            let user_item = MemoryItem {
                memory_id: LsId::new(),
                session_id: ctx.session_id,
                content: serde_json::json!({ "role": "user", "content": message }),
                metadata: HashMap::from([("role".into(), "user".into())]),
                created_at: chrono::Utc::now(),
                ttl_seconds: None,
            };
            memory.write(ctx.clone(), user_item).await?;
        }

        // 4. 构建 LLM 请求 (系统提示 + 记忆上下文)
        let mut messages = vec![
            LlmMessage {
                role: LlmRole::System,
                content: self.system_prompt.clone(),
                name: None,
                tool_calls: None,
            },
        ];

        if let Some(ref memory) = self.memory {
            let history = memory.search(ctx.clone(), "", 10).await?;
            for item in history.items.into_iter().rev() {
                if let Some(msg_content) = item.content.get("content").and_then(|v| v.as_str()) {
                    let role = match item.metadata.get("role").map(|s| s.as_str()) {
                        Some("user") => LlmRole::User,
                        Some("assistant") => LlmRole::Assistant,
                        _ => continue,
                    };
                    messages.push(LlmMessage {
                        role,
                        content: msg_content.to_string(),
                        name: None,
                        tool_calls: None,
                    });
                }
            }
        }

        messages.push(LlmMessage {
            role: LlmRole::User,
            content: message.to_string(),
            name: None,
            tool_calls: None,
        });

        // 5. 调用 LLM
        let llm = self.llm.as_ref()
            .ok_or_else(|| LsError::NotImplemented("no LLM backend".into()))?;
        let llm_request = LlmRequest {
            model: "gpt-4o-mini".into(),
            messages,
            temperature: Some(0.7),
            max_tokens: Some(1024),
            tools: None,
            stream: false,
        };
        let response = llm.invoke(ctx.clone(), llm_request).await?;
        let reply_text = response.message.content.clone();

        info!(tokens = %response.usage.total_tokens, "chat: llm response received");

        // 6. 存储助手回复到记忆
        if let Some(ref memory) = self.memory {
            let assistant_item = MemoryItem {
                memory_id: LsId::new(),
                session_id: ctx.session_id,
                content: serde_json::json!({ "role": "assistant", "content": reply_text }),
                metadata: HashMap::from([("role".into(), "assistant".into())]),
                created_at: chrono::Utc::now(),
                ttl_seconds: None,
            };
            memory.write(ctx.clone(), assistant_item).await?;
        }

        // 7. 发布完成事件
        *self.status.write().await = AgentStatus::Completed;
        self.publish_event(ctx, EventTopic::agent_step_finished(), serde_json::json!({
            "agent_id": self.agent_id.to_string(),
            "reply_length": reply_text.len(),
            "usage": {
                "prompt_tokens": response.usage.prompt_tokens,
                "completion_tokens": response.usage.completion_tokens,
                "total_tokens": response.usage.total_tokens,
            },
        })).await;

        Ok(reply_text)
    }

    async fn publish_event(&self, ctx: &LsContext, topic: EventTopic, payload: serde_json::Value) {
        if let Some(ref bus) = self.event_bus {
            let event = lingshu_traits::event_bus::Event {
                event_id: uuid::Uuid::now_v7().to_string(),
                topic: topic.to_string(),
                session_id: ctx.session_id.to_string(),
                trace_id: ctx.trace_id.to_string(),
                payload,
                timestamp: chrono::Utc::now(),
            };
            let evt_ctx = ctx.child();
            let _ = bus.publish(evt_ctx, event).await;
        }
    }
}

#[async_trait]
impl Agent for ChatAgent {
    fn id(&self) -> LsId {
        self.agent_id
    }

    async fn run(&mut self, _ctx: LsContext, input: serde_json::Value) -> LsResult<AgentOutput> {
        let message = input.get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let ctx = LsContext::with_session(LsId::new());
        let reply = self.chat(&ctx, message).await?;
        Ok(AgentOutput {
            agent_id: self.agent_id,
            status: AgentStatus::Completed,
            data: Some(serde_json::json!({ "reply": reply })),
            error: None,
        })
    }

    async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> {
        *self.status.write().await = AgentStatus::Paused;
        Ok(())
    }

    async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> {
        *self.status.write().await = AgentStatus::Running;
        Ok(())
    }

    async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> {
        *self.status.write().await = AgentStatus::Idle;
        Ok(())
    }

    async fn snapshot(&self, _ctx: LsContext) -> LsResult<AgentSnapshot> {
        Err(LsError::NotImplemented("snapshot".into()))
    }

    async fn restore(&mut self, _ctx: LsContext, _snapshot: AgentSnapshot) -> LsResult<()> {
        Err(LsError::NotImplemented("restore".into()))
    }

    async fn status(&self, _ctx: LsContext) -> LsResult<AgentStatus> {
        Ok(*self.status.read().await)
    }
}

// ═══════════════════════════════════════════════════
// 4. Main — 端到端演示
// ═══════════════════════════════════════════════════

#[tokio::main]
async fn main() -> LsResult<()> {
    tracing_subscriber::fmt::init();

    println!("╔══════════════════════════════════════════════╗");
    println!("║   Lingshu LSCode v1.0.0 · Agent 集成验证    ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();

    // 加载配置并选择 LLM 提供商
    let config = lingshu_config::ConfigLoader::with_cwd().load(None).unwrap_or_default();
    let provider = lingshu_config::settings::LlmProvider::from_env();

    // 使用工厂模式构建 LLM 实例
    let mut llm_config = config.llm.clone();
    llm_config.provider = provider;
    let llm = lingshu_backends::build_llm(&llm_config);

    info!(provider = %provider, model = %llm_config.default_model, "llm configured");

    let bus = InMemoryEventBus::new();
    let memory = InMemoryMemory::new();
    let session_mgr = SessionManager::new(3600);

    let _sub_id = bus.subscribe(
        LsContext::with_session(LsId::new()),
        "ls.*",
        Box::new(move |event| {
            info!(topic = %event.topic, "event received");
            Ok(())
        }),
    ).await?;

    let agent = ChatAgentBuilder::new()
        .llm_box(llm)
        .memory_box(Box::new(memory))
        .event_bus_box(Box::new(bus))
        .session_manager(session_mgr.clone())
        .system_prompt("你是 Lingshu 智能助手。请用中文回答，保持简洁友好。")
        .build();

    let session_id = LsId::new();
    let ctx = LsContext::with_session(session_id)
        .with_user("demo_user")
        .with_metadata("source", "chat_agent_example");

    session_mgr.create(&ctx).await?;
    info!(session_id = %session_id, "session created");

    let messages = vec![
        "你好，请介绍一下你自己",
        "灵枢系统能做什么？",
        "帮我写一个 Rust 的 Hello World",
    ];

    for msg in &messages {
        println!("\n---");
        println!("User: {msg}");
        let child_ctx = ctx.child();
        match agent.chat(&child_ctx, msg).await {
            Ok(reply) => {
                println!("Agent: {reply}");
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }
    }

    let session = session_mgr.get(session_id).await?;
    println!("\n--- Session Info ---");
    println!("  State: {:?}", session.state);
    println!("  User:  {:?}", session.user_id);
    println!("  Active Sessions: {}", session_mgr.active_count().await);

    let status = agent.status(ctx.child()).await?;
    println!("  Agent Status: {status:?}");

    println!("\nEnd-to-end integration verified!");
    Ok(())
}
