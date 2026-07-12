//! LSFed — 跨集群远程 Agent 执行.
//!
//! 通过联邦链路在远端集群上执行 Agent/Tool，并返回结果。
//!
//! ## 执行流程
//!
//! ```text
//! 本地集群                        远端集群
//!    │                              │
//!    │── RemoteExecRequest ──────►  │
//!    │                              │── 查找目标 Agent/Tool
//!    │                              │── 执行
//!    │◄── RemoteExecResponse ──────│
//! ```

use crate::link::LinkManager;
use crate::protocol::FederationMessage;
use crate::types::{RemoteExecRequest, RemoteExecResponse};
use lingshu_core::{LsContext, LsId, LsResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, RwLock};
use tracing::{debug, info};

/// 远程执行器.
pub struct RemoteExecutor {
    /// 连接管理器.
    link_mgr: Arc<LinkManager>,
    /// 待处理的请求.
    pending: Arc<RwLock<std::collections::HashMap<String, oneshot::Sender<RemoteExecResponse>>>>,
}

impl RemoteExecutor {
    /// 创建远程执行器.
    pub fn new(link_mgr: Arc<LinkManager>) -> Self {
        Self {
            link_mgr,
            pending: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// 在远端集群上执行 Agent/Tool.
    pub async fn execute(
        &self,
        target_cluster: &str,
        target: &str,
        payload: serde_json::Value,
        timeout_secs: u64,
    ) -> LsResult<RemoteExecResponse> {
        let request_id = LsId::new().to_string();
        let req = RemoteExecRequest {
            request_id: request_id.clone(),
            target: target.to_string(),
            payload,
            timeout_secs,
            stream: false,
        };

        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(request_id.clone(), tx);

        // 发送远程执行请求
        let msg = FederationMessage::RemoteExecRequest(req);
        let sent = self.link_mgr.send(target_cluster, msg);
        if !sent {
            self.pending.write().await.remove(&request_id);
            return Err(lingshu_core::LsError::Internal(format!(
                "no connection to cluster '{target_cluster}'"
            )));
        }

        // 等待响应（带超时）
        let timeout_dur = Duration::from_secs(timeout_secs.max(10));
        match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(response)) => {
                debug!(
                    cluster = %target_cluster,
                    target = %target,
                    latency_ms = response.latency_ms,
                    "remote execution completed"
                );
                Ok(response)
            }
            Ok(Err(_)) => Err(lingshu_core::LsError::Internal(
                "execution cancelled".into(),
            )),
            Err(_) => {
                self.pending.write().await.remove(&request_id);
                Err(lingshu_core::LsError::Internal(format!(
                    "remote execution timed out after {timeout_dur:?}"
                )))
            }
        }
    }

    /// 处理远端返回的响应.
    pub async fn handle_response(&self, response: RemoteExecResponse) {
        if let Some(tx) = self.pending.write().await.remove(&response.request_id) {
            if tx.send(response).is_err() {
                debug!("response receiver dropped");
            }
        }
    }

    /// 处理入站的远程执行请求（在本地执行）.
    pub async fn handle_incoming_request(
        &self,
        _ctx: &LsContext,
        request: RemoteExecRequest,
    ) -> RemoteExecResponse {
        let start = std::time::Instant::now();
        info!(
            target = %request.target,
            id = %request.request_id,
            "incoming remote execution request"
        );

        // 在实际场景中，这里会通过 Agent Manager 或 Tool Registry 执行
        // 当前返回占位结果
        let result = serde_json::json!({
            "status": "executed",
            "target": request.target,
            "request_id": request.request_id,
        });

        RemoteExecResponse {
            request_id: request.request_id,
            result,
            success: true,
            error: None,
            latency_ms: start.elapsed().as_millis() as u64,
        }
    }
}

/// 远端发现 — 查找提供特定能力的远端集群.
pub struct RemoteDiscovery {
    link_mgr: Arc<LinkManager>,
}

impl RemoteDiscovery {
    /// 创建远端发现实例.
    pub fn new(link_mgr: Arc<LinkManager>) -> Self {
        Self { link_mgr }
    }

    /// 查找提供指定能力的远端集群.
    pub async fn find_provider(&self, capability: &str) -> Vec<String> {
        let mut providers = Vec::new();
        for node in self.link_mgr.online_nodes().await {
            if node.has_capability(capability) {
                providers.push(node.cluster_id.to_string());
            }
        }
        providers
    }

    /// 查找延迟最低的远端集群.
    pub async fn find_best_provider(&self, capability: &str) -> Option<String> {
        let mut best: Option<(String, u64)> = None;
        for node in self.link_mgr.online_nodes().await {
            if node.has_capability(capability) {
                let latency = node.latency_ms;
                match best {
                    Some((_, best_lat)) if latency < best_lat => {
                        best = Some((node.cluster_id.to_string(), latency));
                    }
                    None => {
                        best = Some((node.cluster_id.to_string(), latency));
                    }
                    _ => {}
                }
            }
        }
        best.map(|(id, _)| id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FederationConfig;

    #[tokio::test]
    async fn test_remote_executor_create() {
        let config = FederationConfig::default();
        let link_mgr = Arc::new(LinkManager::new(LsId::new(), "test", config));
        let _executor = RemoteExecutor::new(link_mgr.clone());
        let discovery = RemoteDiscovery::new(link_mgr);

        assert!(discovery.find_provider("gpt-4").await.is_empty());
        let providers = discovery.find_best_provider("gpt-4").await;
        assert!(providers.is_none());
    }

    #[tokio::test]
    async fn test_handle_incoming() {
        let config = FederationConfig::default();
        let link_mgr = Arc::new(LinkManager::new(LsId::new(), "test", config));
        let executor = RemoteExecutor::new(link_mgr);
        let ctx = LsContext::with_session(LsId::new());

        let req = RemoteExecRequest {
            request_id: "test-1".into(),
            target: "echo".into(),
            payload: serde_json::json!("hello"),
            timeout_secs: 10,
            stream: false,
        };

        let resp = executor.handle_incoming_request(&ctx, req).await;
        assert!(resp.success);
        assert_eq!(resp.request_id, "test-1");
    }
}
