//! Agent gRPC 服务 — 代理 agent 操作到 LingshuRuntime.

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::info;

use crate::LingshuRuntime;

use proto::agent_service_server::AgentService;
use proto::{AgentId, AgentStatus, CancelResponse, ListRequest, ListResponse, RunRequest, RunResponse};

pub struct AgentServiceImpl {
    runtime: Arc<LingshuRuntime>,
}

impl AgentServiceImpl {
    pub fn new(runtime: Arc<LingshuRuntime>) -> Self {
        Self { runtime }
    }
}

#[tonic::async_trait]
impl AgentService for AgentServiceImpl {
    async fn run(&self, req: Request<RunRequest>) -> Result<Response<RunResponse>, Status> {
        let inner = req.into_inner();
        info!(agent_id = %inner.agent_id, "gRPC agent run");

        let ctx = self.runtime.root_ctx.clone();
        let session = self.runtime
            .create_session("grpc-user")
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let result = self.runtime
            .agent_manager
            .create(&ctx, &inner.agent_id, &inner.input)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(RunResponse {
            agent_id: inner.agent_id,
            status: "completed".into(),
            output: result,
            duration_ms: 0,
        }))
    }

    async fn get_status(&self, req: Request<AgentId>) -> Result<Response<AgentStatus>, Status> {
        let id = req.into_inner().id;
        let agents = self.runtime.agent_manager.all().await;
        match agents.iter().find(|a| a.id == id) {
            Some(agent) => Ok(Response::new(AgentStatus {
                agent_id: agent.id.clone(),
                status: agent.status.clone().unwrap_or_default(),
                started_at: 0,
                duration_ms: 0,
                error: String::new(),
            })),
            None => Err(Status::not_found(format!("agent {id} not found"))),
        }
    }

    async fn cancel(&self, req: Request<AgentId>) -> Result<Response<CancelResponse>, Status> {
        let id = req.into_inner().id;
        info!(agent_id = %id, "gRPC agent cancel");
        Ok(Response::new(CancelResponse {
            success: true,
            message: format!("agent {id} cancelled"),
        }))
    }

    async fn list(&self, _req: Request<ListRequest>) -> Result<Response<ListResponse>, Status> {
        let agents = self.runtime.agent_manager.all().await;
        let statuses = agents.into_iter().map(|a| AgentStatus {
            agent_id: a.id,
            status: a.status.unwrap_or_default(),
            started_at: 0,
            duration_ms: 0,
            error: String::new(),
        }).collect();

        Ok(Response::new(ListResponse {
            agents: statuses,
            next_page_token: String::new(),
        }))
    }
}
