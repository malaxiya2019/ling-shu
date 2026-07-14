//! WorkflowApproval — 人工审批节点
//!
//! 支持工作流中的人机交互（Human-in-the-Loop），当工作流执行到审批节点时，
//! 暂停执行并将审批请求持久化到存储中，等待人工批准/拒绝后继续执行。
//!
//! # 架构
//!
//! ```text
//! Workflow → Approval Node
//!     │
//!     ├── 创建审批请求 (status=pending)
//!     ├── 暂停工作流执行
//!     │
//!     └── 等待外部 API 调用
//!         ├── POST /v1/workflow/approve → 批准 → 继续执行
//!         └── POST /v1/workflow/reject  → 拒绝 → 终止执行
//! ```

use chrono::{DateTime, Utc};
use lingshu_core::{LsId, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// 审批状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    /// 待审批
    Pending,
    /// 已批准
    Approved,
    /// 已拒绝
    Rejected,
    /// 已超时
    TimedOut,
}

impl std::fmt::Display for ApprovalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApprovalStatus::Pending => write!(f, "pending"),
            ApprovalStatus::Approved => write!(f, "approved"),
            ApprovalStatus::Rejected => write!(f, "rejected"),
            ApprovalStatus::TimedOut => write!(f, "timed_out"),
        }
    }
}

/// 审批请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// 审批请求 ID
    pub id: String,
    /// 工作流 ID
    pub workflow_id: LsId,
    /// 工作流名称
    pub workflow_name: String,
    /// 节点 ID
    pub node_id: LsId,
    /// 节点名称
    pub node_name: String,
    /// 审批标题
    pub title: String,
    /// 审批描述 / 详情
    pub description: String,
    /// 上下文数据（供审批人参考）
    pub context: serde_json::Value,
    /// 审批人（可选，指定谁可以审批）
    pub assignee: Option<String>,
    /// 当前状态
    pub status: ApprovalStatus,
    /// 审批意见
    pub comment: Option<String>,
    /// 超时秒数（到期自动拒绝）
    pub timeout_secs: u64,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 审批时间
    pub decided_at: Option<DateTime<Utc>>,
}

impl ApprovalRequest {
    /// 创建新的审批请求
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workflow_id: LsId,
        workflow_name: &str,
        node_id: LsId,
        node_name: &str,
        title: &str,
        description: &str,
        context: serde_json::Value,
        assignee: Option<String>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            workflow_id,
            workflow_name: workflow_name.to_string(),
            node_id,
            node_name: node_name.to_string(),
            title: title.to_string(),
            description: description.to_string(),
            context,
            assignee,
            status: ApprovalStatus::Pending,
            comment: None,
            timeout_secs,
            created_at: Utc::now(),
            decided_at: None,
        }
    }

    /// 检查是否已过期
    pub fn is_expired(&self) -> bool {
        if self.timeout_secs == 0 {
            return false;
        }
        let elapsed = Utc::now() - self.created_at;
        elapsed.num_seconds() >= self.timeout_secs as i64
    }

    /// 是否已决定（批准/拒绝/超时）
    pub fn is_decided(&self) -> bool {
        self.status != ApprovalStatus::Pending
    }
}

/// 审批管理器
pub struct ApprovalManager {
    /// 所有审批请求 (id -> request)
    requests: Arc<RwLock<HashMap<String, ApprovalRequest>>>,
    /// 通知通道 (request_id -> oneshot sender)
    notifiers: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<ApprovalStatus>>>>,
}

impl ApprovalManager {
    /// 创建新的审批管理器
    pub fn new() -> Self {
        Self {
            requests: Arc::new(RwLock::new(HashMap::new())),
            notifiers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 创建审批请求并等待结果
    ///
    /// 返回一个 future，在工作流中等待直到审批完成。
    pub async fn create_and_wait(&self, request: ApprovalRequest) -> LsResult<ApprovalStatus> {
        let request_id = request.id.clone();

        // 创建 oneshot 通道
        let (tx, rx) = tokio::sync::oneshot::channel::<ApprovalStatus>();

        // 存储请求
        {
            let mut requests = self.requests.write().await;
            requests.insert(request_id.clone(), request);
        }

        // 注册通知器
        {
            let mut notifiers = self.notifiers.write().await;
            notifiers.insert(request_id.clone(), tx);
        }

        info!("approval: created request={}", request_id);

        // 等待审批结果
        match rx.await {
            Ok(status) => {
                // 更新请求状态
                let mut requests = self.requests.write().await;
                if let Some(req) = requests.get_mut(&request_id) {
                    req.status = status;
                    req.decided_at = Some(Utc::now());
                }
                info!(
                    "approval: decided request={}, status={:?}",
                    request_id, status
                );
                Ok(status)
            }
            Err(_) => {
                // 通道关闭（通知器被移除）
                warn!("approval: channel closed for request={}", request_id);
                Ok(ApprovalStatus::TimedOut)
            }
        }
    }

    /// 批准请求
    pub async fn approve(&self, request_id: &str, comment: Option<String>) -> LsResult<bool> {
        let notifier = {
            let mut notifiers = self.notifiers.write().await;
            notifiers.remove(request_id)
        };

        match notifier {
            Some(tx) => {
                // 更新状态
                {
                    let mut requests = self.requests.write().await;
                    if let Some(req) = requests.get_mut(request_id) {
                        req.status = ApprovalStatus::Approved;
                        req.comment = comment;
                        req.decided_at = Some(Utc::now());
                    }
                }
                let _ = tx.send(ApprovalStatus::Approved);
                Ok(true)
            }
            None => {
                // 请求不存在或已处理
                let requests = self.requests.read().await;
                match requests.get(request_id) {
                    Some(req) => {
                        warn!(
                            "approval: request={} already decided (status={:?})",
                            request_id, req.status
                        );
                        Ok(false)
                    }
                    None => {
                        warn!("approval: request={} not found", request_id);
                        Ok(false)
                    }
                }
            }
        }
    }

    /// 拒绝请求
    pub async fn reject(&self, request_id: &str, comment: Option<String>) -> LsResult<bool> {
        let notifier = {
            let mut notifiers = self.notifiers.write().await;
            notifiers.remove(request_id)
        };

        match notifier {
            Some(tx) => {
                {
                    let mut requests = self.requests.write().await;
                    if let Some(req) = requests.get_mut(request_id) {
                        req.status = ApprovalStatus::Rejected;
                        req.comment = comment;
                        req.decided_at = Some(Utc::now());
                    }
                }
                let _ = tx.send(ApprovalStatus::Rejected);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// 列出待审批的请求
    pub async fn list_pending(&self) -> Vec<ApprovalRequest> {
        let requests = self.requests.read().await;
        requests
            .values()
            .filter(|r| r.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    /// 获取单个请求
    pub async fn get(&self, request_id: &str) -> Option<ApprovalRequest> {
        self.requests.read().await.get(request_id).cloned()
    }

    /// 列出所有请求
    pub async fn list_all(&self) -> Vec<ApprovalRequest> {
        self.requests.read().await.values().cloned().collect()
    }

    /// 清理过期的审批请求
    pub async fn clean_expired(&self) -> usize {
        let mut count = 0;
        let expired_ids: Vec<String> = {
            let requests = self.requests.read().await;
            requests
                .values()
                .filter(|r| r.status == ApprovalStatus::Pending && r.is_expired())
                .map(|r| r.id.clone())
                .collect()
        };

        for id in &expired_ids {
            let notifier = {
                let mut notifiers = self.notifiers.write().await;
                notifiers.remove(id)
            };
            if let Some(tx) = notifier {
                let _ = tx.send(ApprovalStatus::TimedOut);
            }
            let mut requests = self.requests.write().await;
            if let Some(req) = requests.get_mut(id) {
                req.status = ApprovalStatus::TimedOut;
                req.decided_at = Some(Utc::now());
            }
            count += 1;
        }

        if count > 0 {
            info!("approval: cleaned {} expired requests", count);
        }

        count
    }

    /// 获取待审批数量
    pub async fn pending_count(&self) -> usize {
        let requests = self.requests.read().await;
        requests
            .values()
            .filter(|r| r.status == ApprovalStatus::Pending)
            .count()
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── 测试 ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request() -> ApprovalRequest {
        ApprovalRequest::new(
            LsId::new(),
            "test-workflow",
            LsId::new(),
            "approval-node",
            "请审批",
            "需要人工确认",
            serde_json::json!({"key": "value"}),
            None,
            3600,
        )
    }

    #[tokio::test]
    async fn test_create_and_list() {
        let mgr = ApprovalManager::new();
        let req = make_request();
        let id = req.id.clone();

        // 不等待，直接插入
        {
            let mut requests = mgr.requests.write().await;
            requests.insert(id.clone(), req);
        }

        assert_eq!(mgr.pending_count().await, 1);
        let pending = mgr.list_pending().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id);
    }

    #[tokio::test]
    async fn test_approve() {
        let mgr = ApprovalManager::new();
        let req = make_request();
        let id = req.id.clone();

        // 创建 notifier
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut requests = mgr.requests.write().await;
            requests.insert(id.clone(), req);
        }
        {
            let mut notifiers = mgr.notifiers.write().await;
            notifiers.insert(id.clone(), tx);
        }

        // 批准
        let approved = mgr.approve(&id, Some("同意".into())).await.unwrap();
        assert!(approved);

        // 验证状态
        let result = rx.await.unwrap();
        assert_eq!(result, ApprovalStatus::Approved);

        let req = mgr.get(&id).await.unwrap();
        assert_eq!(req.status, ApprovalStatus::Approved);
        assert_eq!(req.comment, Some("同意".to_string()));
    }

    #[tokio::test]
    async fn test_reject() {
        let mgr = ApprovalManager::new();
        let req = make_request();
        let id = req.id.clone();

        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut requests = mgr.requests.write().await;
            requests.insert(id.clone(), req);
        }
        {
            let mut notifiers = mgr.notifiers.write().await;
            notifiers.insert(id.clone(), tx);
        }

        let rejected = mgr.reject(&id, Some("拒绝".into())).await.unwrap();
        assert!(rejected);

        let result = rx.await.unwrap();
        assert_eq!(result, ApprovalStatus::Rejected);

        let req = mgr.get(&id).await.unwrap();
        assert_eq!(req.status, ApprovalStatus::Rejected);
    }

    #[tokio::test]
    async fn test_approve_nonexistent() {
        let mgr = ApprovalManager::new();
        let result = mgr.approve("nonexistent", None).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_clean_expired() {
        let mgr = ApprovalManager::new();
        let mut req = make_request();
        req.timeout_secs = 0; // 设置立即过期
                              // 使用负的时间来模拟过期
        req.created_at = Utc::now() - chrono::Duration::hours(1);
        let id = req.id.clone();

        let (tx, _rx) = tokio::sync::oneshot::channel();
        {
            let mut requests = mgr.requests.write().await;
            requests.insert(id.clone(), req);
        }
        {
            let mut notifiers = mgr.notifiers.write().await;
            notifiers.insert(id.clone(), tx);
        }

        // 设置 timeout_secs 为 0 不会过期，用另一种方式
        // 实际上 clean_expired 检查的是 is_expired，timeout_secs=0 意味着不过期
        // 所以我们创建一个真正过期的：timeout_secs=1, created_at=-2h
        drop(mgr.requests);
        drop(mgr.notifiers);

        let mgr = ApprovalManager::new();
        let mut req2 = make_request();
        req2.timeout_secs = 1; // 1秒超时
        req2.created_at = Utc::now() - chrono::Duration::hours(2); // 2小时前创建
        let id2 = req2.id.clone();

        let (tx2, _rx2) = tokio::sync::oneshot::channel();
        {
            let mut requests = mgr.requests.write().await;
            requests.insert(id2.clone(), req2);
        }
        {
            let mut notifiers = mgr.notifiers.write().await;
            notifiers.insert(id2.clone(), tx2);
        }

        let cleaned = mgr.clean_expired().await;
        assert!(cleaned >= 1);

        let req = mgr.get(&id2).await.unwrap();
        assert_eq!(req.status, ApprovalStatus::TimedOut);
    }

    #[tokio::test]
    async fn test_create_and_wait() {
        let mgr = Arc::new(ApprovalManager::new());
        let mgr_clone = mgr.clone();
        let req = make_request();
        let id = req.id.clone();

        // 在另一个任务中等待审批
        let handle = tokio::spawn(async move { mgr_clone.create_and_wait(req).await });

        // 确保等待已启动
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 批准
        mgr.approve(&id, None).await.unwrap();

        let result = handle.await.unwrap().unwrap();
        assert_eq!(result, ApprovalStatus::Approved);
    }

    #[test]
    fn test_is_expired() {
        let mut req = make_request();
        req.timeout_secs = 1;
        req.created_at = Utc::now() - chrono::Duration::seconds(2);
        assert!(req.is_expired());

        req.timeout_secs = 0;
        assert!(!req.is_expired());
    }

    #[test]
    fn test_approval_status_display() {
        assert_eq!(ApprovalStatus::Pending.to_string(), "pending");
        assert_eq!(ApprovalStatus::Approved.to_string(), "approved");
        assert_eq!(ApprovalStatus::Rejected.to_string(), "rejected");
        assert_eq!(ApprovalStatus::TimedOut.to_string(), "timed_out");
    }
}
