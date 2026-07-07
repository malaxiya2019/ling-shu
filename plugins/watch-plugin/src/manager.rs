//! 📹 Watch Skill Process Manager — 管理 watch-skill 子进程生命周期.
//!
//! 负责启动/停止/监控 watch-skill API 服务 (Python/FastAPI 子进程).

use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

/// Watch Skill 进程状态.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WatchStatus {
    Stopped,
    Starting,
    Running { pid: u32, port: u16 },
    Failed(String),
}

/// Watch Skill 子进程管理器.
pub struct WatchManager {
    python_bin: String,
    api_port: u16,
    api_host: String,
    status: Arc<RwLock<WatchStatus>>,
    child: Arc<Mutex<Option<Child>>>,
    health_url: String,
    api_base_url: String,
}

impl WatchManager {
    /// 创建新的 Watch Skill 管理器.
    pub fn new(python_bin: &str, port: u16) -> Self {
        Self {
            python_bin: python_bin.to_string(),
            api_port: port,
            api_host: "127.0.0.1".to_string(),
            status: Arc::new(RwLock::new(WatchStatus::Stopped)),
            child: Arc::new(Mutex::new(None)),
            health_url: format!("http://127.0.0.1:{}/health", port),
            api_base_url: format!("http://127.0.0.1:{}", port),
        }
    }

    /// 获取 API 基础 URL.
    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    /// 获取当前状态.
    pub async fn status(&self) -> WatchStatus {
        self.status.read().await.clone()
    }

    /// 检查 watch-skill 是否已安装 (pip).
    async fn check_installed(&self) -> Result<(), String> {
        let output = Command::new(&self.python_bin)
            .args(["-m", "pip", "show", "watch-skill"])
            .output()
            .await
            .map_err(|e| format!("Failed to run Python: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "watch-skill not installed. Install with: pip install watch-skill\n{}",
                stderr
            ));
        }
        Ok(())
    }

    /// 启动 watch-skill API 服务.
    pub async fn start(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        if *status != WatchStatus::Stopped {
            return Err(format!("Watch Skill is not stopped (current: {:?})", status));
        }

        *status = WatchStatus::Starting;
        drop(status);

        // 检查是否已安装
        self.check_installed().await?;

        info!(port = self.api_port, "starting watch-skill API server");

        // 启动 watch-skill api 服务 (uvicorn)
        let child = Command::new(&self.python_bin)
            .args([
                "-m", "watch_skill.surfaces.api",
                "--port", &self.api_port.to_string(),
                "--host", &self.api_host,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn watch-skill: {}", e))?;

        let pid = child.id().ok_or("Failed to get child PID")?;
        info!(pid = pid, "watch-skill process spawned");

        let mut child_lock = self.child.lock().await;
        *child_lock = Some(child);
        drop(child_lock);

        // 等待服务就绪
        let max_retries = 15u32;
        for i in 0..max_retries {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            // 检查进程是否存活
            let mut child_lock = self.child.lock().await;
            if let Some(ref mut child) = *child_lock {
                match child.try_wait() {
                    Ok(Some(exit)) => {
                        let msg = format!("watch-skill exited prematurely with code: {}", exit);
                        *child_lock = None;
                        let mut status = self.status.write().await;
                        *status = WatchStatus::Failed(msg.clone());
                        return Err(msg);
                    }
                    Ok(None) => {}
                    Err(e) => warn!("Error checking watch-skill process: {}", e),
                }
            }
            drop(child_lock);

            // 健康检查
            if self.health_check().await {
                let mut status = self.status.write().await;
                *status = WatchStatus::Running { pid, port: self.api_port };
                info!(pid = pid, port = self.api_port, "watch-skill API is ready");
                return Ok(());
            }

            if i % 5 == 4 {
                info!(retry = i + 1, "waiting for watch-skill to become ready...");
            }
        }

        let msg = format!("watch-skill did not become ready within {} seconds", max_retries);
        let mut status = self.status.write().await;
        *status = WatchStatus::Failed(msg.clone());
        Err(msg)
    }

    /// 停止 watch-skill 服务.
    pub async fn stop(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        let mut child_lock = self.child.lock().await;

        if let Some(ref mut child) = *child_lock {
            info!("stopping watch-skill process");

            // SIGTERM
            child.start_kill().map_err(|e| format!("kill failed: {}", e))?;

            for _ in 0..5 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                match child.try_wait() {
                    Ok(Some(exit)) => {
                        info!(code = %exit, "watch-skill process exited");
                        *child_lock = None;
                        *status = WatchStatus::Stopped;
                        return Ok(());
                    }
                    _ => {}
                }
            }

            // SIGKILL
            child.kill().await.map_err(|e| format!("force kill failed: {}", e))?;
            let _ = child.wait().await;
            info!("watch-skill force killed");
        }

        *child_lock = None;
        *status = WatchStatus::Stopped;
        Ok(())
    }

    /// 健康检查.
    async fn health_check(&self) -> bool {
        match reqwest::get(&self.health_url).await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// 确保服务运行中.
    pub async fn ensure_running(&self) -> bool {
        let status = self.status().await;
        match status {
            WatchStatus::Running { .. } => {
                if !self.health_check().await {
                    warn!("watch-skill health check failed, attempting restart");
                    let _ = self.stop().await;
                    self.start().await.is_ok()
                } else {
                    true
                }
            }
            WatchStatus::Stopped | WatchStatus::Failed(_) => {
                self.start().await.is_ok()
            }
            WatchStatus::Starting => true,
        }
    }
}
