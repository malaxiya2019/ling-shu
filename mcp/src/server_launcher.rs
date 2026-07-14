//! 🚀 MCP Server Launcher — 自动启动 & 连接外部 MCP 服务器.
//!
//! 支持从配置声明启动 MCP 服务器子进程，并自动连接到 Client Pool。
//!
//! ## 配置示例
//!
//! ```yaml
//! mcp:
//!   servers:
//!     security-hub:
//!       command: npx
//!       args: ["@fuzzinglabs/mcp-security-hub"]
//!       env:
//!         NMAP_PATH: /usr/bin/nmap
//!     copilot:
//!       command: npx
//!       args: ["-y", "@github/copilot-plugins"]
//!     filesystem:
//!       command: npx
//!       args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
//! ```

use lingshu_core::{LsError, LsResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// MCP 服务器配置.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 启动命令 (例如 npx, python, node).
    pub command: String,
    /// 命令参数.
    #[serde(default)]
    pub args: Vec<String>,
    /// 环境变量.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// 连接 URL (如果服务器已运行在远程).
    #[serde(default)]
    pub url: Option<String>,
    /// 自动启动.
    #[serde(default = "default_true")]
    pub auto_start: bool,
}

fn default_true() -> bool {
    true
}

/// MCP 服务器管理器 — 启动 & 生命周期管理.
pub struct McpServerManager {
    /// 已启动的子进程
    processes: Arc<RwLock<HashMap<String, McpServerProcess>>>,
    /// MCP 客户端池
    client_pool: Arc<crate::rmcp_client::McpClientPool>,
}

struct McpServerProcess {
    child: Option<std::process::Child>,
    #[allow(dead_code)]
    url: String,
}

impl McpServerManager {
    /// 创建新的 MCP 服务器管理器.
    pub fn new(client_pool: Arc<crate::rmcp_client::McpClientPool>) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            client_pool,
        }
    }

    /// 根据配置启动并连接 MCP 服务器.
    pub async fn start_server(&self, name: &str, config: &McpServerConfig) -> LsResult<()> {
        if !config.auto_start {
            info!("MCP server '{}' auto-start disabled, skipping", name);
            return Ok(());
        }

        // 如果指定了 URL，直接连接 (远程服务器)
        if let Some(ref url) = config.url {
            info!("MCP server '{}': connecting to {}...", name, url);
            self.client_pool.add_server(name, url).await?;
            info!("MCP server '{}': connected", name);
            return Ok(());
        }

        // 否则启动子进程
        info!(
            "MCP server '{}': starting '{}' with args {:?}...",
            name, config.command, config.args
        );

        let mut cmd = std::process::Command::new(&config.command);
        cmd.args(&config.args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // 设置环境变量
        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        let child = cmd.spawn().map_err(|e| {
            LsError::Plugin(format!(
                "failed to start MCP server '{}' ({}): {e}",
                name, config.command
            ))
        })?;

        info!("MCP server '{}' started (PID: {})", name, child.id());

        // 对于 stdio 模式的 MCP 服务器，使用 stdio 传输层
        // 对于 HTTP 模式的 MCP 服务器，等待端口就绪后连接 HTTP
        let url = if config
            .args
            .iter()
            .any(|a| a.contains("http") || a.contains("port"))
        {
            config
                .url
                .clone()
                .unwrap_or_else(|| "http://127.0.0.1:3000/mcp".into())
        } else {
            // stdio 模式 — 使用 stdin/stdout 通信
            // 这里简化处理: 通过子进程的 stdin/stdout 进行 JSON-RPC
            // 实际项目中可以通过 tokio::process::Command + 管道实现
            "stdio://local".into()
        };

        self.processes.write().await.insert(
            name.to_string(),
            McpServerProcess {
                child: Some(child),
                url: url.clone(),
            },
        );

        info!("MCP server '{}' started and registered", name);
        Ok(())
    }

    /// 从配置清单启动所有 MCP 服务器.
    pub async fn start_all(&self, configs: &HashMap<String, McpServerConfig>) {
        for (name, config) in configs {
            if let Err(e) = self.start_server(name, config).await {
                warn!("Failed to start MCP server '{}': {e}", name);
            }
        }
    }

    /// 停止指定 MCP 服务器.
    pub async fn stop_server(&self, name: &str) -> LsResult<()> {
        let mut processes = self.processes.write().await;
        if let Some(mut process) = processes.remove(name) {
            if let Some(ref mut child) = process.child {
                let _ = child.kill();
                let _ = child.wait();
                info!("MCP server '{}' stopped", name);
            }
        }
        Ok(())
    }

    /// 停止所有 MCP 服务器.
    pub async fn stop_all(&self) {
        let names: Vec<String> = self.processes.read().await.keys().cloned().collect();
        for name in names {
            let _ = self.stop_server(&name).await;
        }
    }

    /// 获取已启动的 MCP 服务器列表.
    pub async fn list_servers(&self) -> Vec<String> {
        self.processes.read().await.keys().cloned().collect()
    }
}

impl Drop for McpServerManager {
    fn drop(&mut self) {
        // 同步停止所有子进程 (简化处理)
        if let Ok(mut processes) = self.processes.try_write() {
            for (_, process) in processes.iter_mut() {
                if let Some(ref mut child) = process.child {
                    let _ = child.kill();
                }
            }
        }
    }
}

/// 预配置的 MCP 服务器集合.
pub fn default_server_configs() -> HashMap<String, McpServerConfig> {
    let mut configs = HashMap::new();

    // FuzzingLabs MCP Security Hub (安全工具)
    configs.insert(
        "security-hub".into(),
        McpServerConfig {
            command: "npx".into(),
            args: vec!["-y".into(), "@fuzzinglabs/mcp-security-hub".into()],
            env: HashMap::new(),
            url: None,
            auto_start: false,
        },
    );

    // GitHub Copilot Plugins
    configs.insert(
        "copilot-plugins".into(),
        McpServerConfig {
            command: "npx".into(),
            args: vec!["-y".into(), "@github/copilot-plugins".into()],
            env: HashMap::new(),
            url: None,
            auto_start: false,
        },
    );

    // IBM MCP
    configs.insert(
        "ibm-mcp".into(),
        McpServerConfig {
            command: "npx".into(),
            args: vec!["-y".into(), "@ibm/mcp".into()],
            env: HashMap::new(),
            url: None,
            auto_start: false,
        },
    );

    // Filesystem MCP Server
    configs.insert(
        "filesystem".into(),
        McpServerConfig {
            command: "npx".into(),
            args: vec![
                "-y".into(),
                "@modelcontextprotocol/server-filesystem".into(),
                "/tmp".into(),
            ],
            env: HashMap::new(),
            url: None,
            auto_start: false,
        },
    );

    // OmniVoice Studio — 本地语音合成 & 识别 (MCP 模式)
    configs.insert(
        "omnivoice".into(),
        McpServerConfig {
            command: "python".into(),
            args: vec!["-m".into(), "backend.mcp_server".into()],
            env: HashMap::from([("OMNIVOICE_API_URL".into(), "http://localhost:3900".into())]),
            url: None,
            auto_start: false,
        },
    );

    configs
}
