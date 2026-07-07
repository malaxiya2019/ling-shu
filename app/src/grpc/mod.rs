//! Lingshu gRPC API — Phase 4: 高性能 gRPC 接口.
//!
//! 提供 Agent / LLM 的 gRPC 端点，与 HTTP REST API 共享 `LingshuRuntime`。
//!
//! ## 实现的服务
//! - `AgentService` — Run, GetStatus, Cancel, List
//! - `LLMService` — Chat, ChatStream, Embed

pub mod agent;
pub mod llm;

use lingshu_core::{LsError, LsResult};
use std::sync::Arc;
use tonic::transport::Server;
use tracing::info;

use crate::LingshuRuntime;

/// 启动 gRPC 服务器.
pub async fn start_grpc_server(
    runtime: Arc<LingshuRuntime>,
    addr: &str,
) -> LsResult<()> {
    let socket_addr: std::net::SocketAddr = addr
        .parse()
        .map_err(|e| LsError::Internal(format!("invalid gRPC addr {addr}: {e}")))?;

    let agent_svc = agent::AgentServiceImpl::new(runtime.clone());
    let llm_svc = llm::LLMServiceImpl::new(runtime.clone());

    info!(addr = %addr, "gRPC server starting");

    Server::builder()
        .add_service(proto::agent_service_server::AgentServiceServer::new(agent_svc))
        .add_service(proto::llm_service_server::LLMServiceServer::new(llm_svc))
        .serve(socket_addr)
        .await
        .map_err(|e| LsError::Internal(format!("gRPC server error: {e}")))?;

    Ok(())
}
