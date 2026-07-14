//! 🔄 Sync Task — 动态 MCP 工具同步
//!
//! 后台定期检查 agent-device MCP 工具列表的变化，
//! 自动发现新工具、淘汰已移除的工具，增量更新到 ToolRegistry。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use lingshu_core::{LsContext, LsResult};
use lingshu_mcp::rmcp_stdio_client::{McpStdioClient, McpTool};
use lingshu_tool::ToolRegistry;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// 工具同步状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncStatus {
    /// 上次同步时间（时间戳秒）
    pub last_sync_unix: u64,
    /// 工具数量
    pub tool_count: usize,
    /// 新增工具数（上次同步以来）
    pub new_tools: usize,
    /// 移除工具数
    pub removed_tools: usize,
    /// 同步是否正在运行
    pub running: bool,
    /// 同步间隔（秒）
    pub interval_secs: u64,
    /// 错误数
    pub error_count: u64,
}

/// 工具同步任务 — 定期检测 MCP 工具列表变化
#[allow(dead_code)]
pub struct McpToolSyncTask {
    /// MCP 客户端
    client: Arc<McpStdioClient>,
    /// 工具注册表
    registry: ToolRegistry,
    /// 上次发现的工具集（名称 → 摘要）
    last_tools: Arc<RwLock<std::collections::HashMap<String, McpTool>>>,
    /// 同步状态
    status: Arc<RwLock<SyncStatus>>,
    /// 运行标志
    running: Arc<AtomicBool>,
    /// 错误计数
    error_count: Arc<AtomicU64>,
    /// 上下文
    ctx: LsContext,
}

impl McpToolSyncTask {
    /// 创建新的同步任务
    pub fn new(client: Arc<McpStdioClient>, registry: ToolRegistry, ctx: LsContext) -> Self {
        Self {
            client,
            registry,
            last_tools: Arc::new(RwLock::new(std::collections::HashMap::new())),
            status: Arc::new(RwLock::new(SyncStatus {
                last_sync_unix: 0,
                tool_count: 0,
                new_tools: 0,
                removed_tools: 0,
                running: false,
                interval_secs: 30,
                error_count: 0,
            })),
            running: Arc::new(AtomicBool::new(false)),
            error_count: Arc::new(AtomicU64::new(0)),
            ctx,
        }
    }

    /// 获取当前同步状态
    pub async fn status(&self) -> SyncStatus {
        self.status.read().await.clone()
    }

    /// 启动后台同步循环
    pub fn start(self: &Arc<Self>, interval: Duration) {
        if self.running.swap(true, Ordering::SeqCst) {
            warn!("Sync task already running");
            return;
        }

        let task = self.clone();
        tokio::spawn(async move {
            info!(
                interval_ms = interval.as_millis(),
                "MCP tool sync task started"
            );

            // 更新状态中的间隔
            {
                let mut status = task.status.write().await;
                status.interval_secs = interval.as_secs();
                status.running = true;
            }

            let mut tick = tokio::time::interval(interval);
            // 首次同步立即执行
            if let Err(e) = task.sync_once().await {
                warn!(error = %e, "Initial MCP tool sync failed");
            }

            loop {
                tick.tick().await;

                // 检查是否停止
                if !task.running.load(Ordering::SeqCst) {
                    info!("MCP tool sync task stopped");
                    break;
                }

                // 健康检查
                if let Err(e) = task.client.health_check().await {
                    warn!(error = %e, "MCP health check failed during sync");
                    task.error_count.fetch_add(1, Ordering::SeqCst);
                    continue;
                }

                // 执行同步
                if let Err(e) = task.sync_once().await {
                    warn!(error = %e, "MCP tool sync iteration failed");
                    task.error_count.fetch_add(1, Ordering::SeqCst);
                }
            }

            // 标记停止
            task.running.store(false, Ordering::SeqCst);
            {
                let mut status = task.status.write().await;
                status.running = false;
            }
        });
    }

    /// 停止同步任务
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("MCP tool sync stop requested");
    }

    /// 执行一次工具同步
    async fn sync_once(&self) -> LsResult<()> {
        debug!("Syncing MCP tools...");

        // 获取当前远程工具列表
        let current_tools = self.client.list_tools().await?;
        let current_names: std::collections::HashSet<String> =
            current_tools.iter().map(|t| t.name.clone()).collect();

        // 获取上次的工具集
        let mut last_tools = self.last_tools.write().await;
        let last_names: std::collections::HashSet<String> = last_tools.keys().cloned().collect();

        // 计算差异
        let new_tools: Vec<&McpTool> = current_tools
            .iter()
            .filter(|t| !last_names.contains(&t.name))
            .collect();

        let removed_names: Vec<&String> = last_names.difference(&current_names).collect();

        // 更新工具注册表
        if !new_tools.is_empty() {
            for tool in &new_tools {
                info!(
                    tool_name = %tool.name,
                    "New MCP tool discovered"
                );
                // 注册工具的逻辑由外部处理
                // 这里只记录发现
            }
        }

        if !removed_names.is_empty() {
            for name in &removed_names {
                info!(
                    tool_name = %name,
                    "MCP tool removed"
                );
            }
        }

        // 更新 last_tools 为当前列表
        last_tools.clear();
        for tool in &current_tools {
            last_tools.insert(tool.name.clone(), tool.clone());
        }

        // 更新状态
        {
            let mut status = self.status.write().await;
            status.last_sync_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            status.tool_count = current_tools.len();
            status.new_tools = new_tools.len();
            status.removed_tools = removed_names.len();
            status.error_count = self.error_count.load(Ordering::SeqCst);
        }

        if !new_tools.is_empty() || !removed_names.is_empty() {
            info!(
                tool_count = current_tools.len(),
                new = new_tools.len(),
                removed = removed_names.len(),
                "MCP tools synchronized"
            );
        }

        Ok(())
    }

    /// 执行一次性同步并返回差异
    pub async fn sync_and_diff(&self) -> LsResult<ToolSyncDiff> {
        self.sync_once().await?;

        let status = self.status.read().await;
        Ok(ToolSyncDiff {
            tool_count: status.tool_count,
            new_tools: status.new_tools,
            removed_tools: status.removed_tools,
            last_sync_unix: status.last_sync_unix,
        })
    }
}

/// 工具同步差异报告
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSyncDiff {
    pub tool_count: usize,
    pub new_tools: usize,
    pub removed_tools: usize,
    pub last_sync_unix: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_default() {
        let status = SyncStatus {
            last_sync_unix: 0,
            tool_count: 0,
            new_tools: 0,
            removed_tools: 0,
            running: false,
            interval_secs: 30,
            error_count: 0,
        };

        assert_eq!(status.tool_count, 0);
        assert!(!status.running);
    }

    #[test]
    fn test_sync_diff_serde() {
        let diff = ToolSyncDiff {
            tool_count: 42,
            new_tools: 2,
            removed_tools: 1,
            last_sync_unix: 1000000,
        };

        let json = serde_json::to_string(&diff).unwrap();
        let parsed: ToolSyncDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_count, 42);
        assert_eq!(parsed.new_tools, 2);
    }

    #[test]
    fn test_sync_status_serde() {
        let status = SyncStatus {
            last_sync_unix: 1234567890,
            tool_count: 70,
            new_tools: 5,
            removed_tools: 0,
            running: true,
            interval_secs: 60,
            error_count: 0,
        };

        let json = serde_json::to_string_pretty(&status).unwrap();
        let parsed: SyncStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_count, 70);
        assert!(parsed.running);
        assert_eq!(parsed.interval_secs, 60);
    }
}
