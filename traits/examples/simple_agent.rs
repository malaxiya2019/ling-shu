//! 简单示例: 展示 LSCode v1.0.0 核心 API 使用模式.
//!
//! 运行: cargo run --example simple_agent

use async_trait::async_trait;
use lingshu_core::{LsContext, LsError, LsId, LsResult};
use lingshu_traits::agent::{Agent, AgentOutput, AgentSnapshot, AgentStatus};
use serde_json::json;

/// 一个最小化的 Agent 实现，演示 LSCode 规范接口.
struct HelloAgent {
    id: LsId,
    status: AgentStatus,
}

#[async_trait]
impl Agent for HelloAgent {
    fn id(&self) -> LsId {
        self.id
    }

    async fn run(&mut self, ctx: LsContext, input: serde_json::Value) -> LsResult<AgentOutput> {
        self.status = AgentStatus::Running;
        tracing::info!(agent_id = %self.id, session_id = %ctx.session_id, "agent running");

        // 模拟处理
        let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("world");
        let output = AgentOutput {
            agent_id: self.id,
            status: AgentStatus::Completed,
            data: Some(json!({ "greeting": format!("Hello, {name}!") })),
            error: None,
        };
        self.status = AgentStatus::Completed;
        Ok(output)
    }

    async fn pause(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.status = AgentStatus::Paused;
        Ok(())
    }

    async fn resume(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.status = AgentStatus::Running;
        Ok(())
    }

    async fn cancel(&mut self, _ctx: LsContext) -> LsResult<()> {
        self.status = AgentStatus::Idle;
        Ok(())
    }

    async fn snapshot(&self, _ctx: LsContext) -> LsResult<AgentSnapshot> {
        Err(LsError::NotImplemented("snapshot".into()))
    }

    async fn restore(&mut self, _ctx: LsContext, _snapshot: AgentSnapshot) -> LsResult<()> {
        Err(LsError::NotImplemented("restore".into()))
    }

    async fn status(&self, _ctx: LsContext) -> LsResult<AgentStatus> {
        Ok(self.status)
    }
}

#[tokio::main]
async fn main() -> LsResult<()> {
    tracing_subscriber::fmt::init();

    // 构造上下文
    let ctx = LsContext::with_session(LsId::new()).with_user("demo_user");

    // 创建并执行 Agent
    let mut agent = HelloAgent {
        id: LsId::new(),
        status: AgentStatus::Idle,
    };

    let output = agent.run(ctx.child(), json!({ "name": "Lingshu" })).await?;
    println!("✅ Agent output: {:?}", output.data);

    // 查询状态
    let status = agent.status(ctx.child()).await?;
    println!("📊 Agent status: {status:?}");

    Ok(())
}
