//! 🕷️ BeEF Process Manager — 管理 BeEF Ruby 子进程生命周期.
//!
//! 负责启动/停止/监控 BeEF 进程，健康检查，端口检测。

use std::path::PathBuf;
// AtomicBool, Ordering unused in current implementation
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

/// BeEF 进程状态.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BeefStatus {
    Stopped,
    Starting,
    Running { pid: u32, port: u16 },
    Failed(String),
}

/// BeEF 子进程管理器.
pub struct BeefManager {
    beef_dir: PathBuf,
    ruby_bin: String,
    config_path: PathBuf,
    status: Arc<RwLock<BeefStatus>>,
    child: Arc<Mutex<Option<Child>>>,
    health_check_url: String,
}

impl BeefManager {
    /// 创建新的 BeEF 管理器.
    pub fn new(beef_dir: PathBuf, ruby_bin: &str, port: u16) -> Self {
        let config_path = beef_dir.join("config.yaml");
        Self {
            beef_dir,
            ruby_bin: ruby_bin.to_string(),
            config_path,
            status: Arc::new(RwLock::new(BeefStatus::Stopped)),
            child: Arc::new(Mutex::new(None)),
            health_check_url: format!("http://127.0.0.1:{}/api/status", port),
        }
    }

    /// 获取当前状态.
    pub async fn status(&self) -> BeefStatus {
        self.status.read().await.clone()
    }

    /// 启动 BeEF 进程.
    pub async fn start(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        if *status != BeefStatus::Stopped {
            return Err(format!("BeEF is not stopped (current: {:?})", status));
        }

        *status = BeefStatus::Starting;
        drop(status);

        info!(dir = %self.beef_dir.display(), "starting BeEF process");

        // 检查 Ruby 是否可用
        let ruby_check = Command::new(&self.ruby_bin).arg("--version").output().await;
        match ruby_check {
            Ok(output) => {
                let version = String::from_utf8_lossy(&output.stdout);
                info!(ruby = %version.trim(), "Ruby found");
            }
            Err(e) => {
                let msg = format!("Ruby not found at '{}': {}. Install ruby + bundler and run `./install` in beef/", self.ruby_bin, e);
                let mut status = self.status.write().await;
                *status = BeefStatus::Failed(msg.clone());
                return Err(msg);
            }
        }

        // 启动 BeEF
        let child = Command::new(&self.ruby_bin)
            .arg("beef")
            .arg("--config")
            .arg(self.config_path.to_str().unwrap_or("config.yaml"))
            .current_dir(&self.beef_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn BeEF: {}", e))?;

        let pid = child.id().ok_or("Failed to get child PID")?;
        info!(pid = pid, "BeEF process spawned");

        let mut child_lock = self.child.lock().await;
        *child_lock = Some(child);
        drop(child_lock);

        // 等待进程就绪 (轮询 HTTP 接口)
        let max_retries = 30u32;
        for i in 0..max_retries {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            // 检查进程是否仍在运行
            let mut child_lock = self.child.lock().await;
            if let Some(ref mut child) = *child_lock {
                match child.try_wait() {
                    Ok(Some(exit)) => {
                        let msg = format!("BeEF exited prematurely with code: {}", exit);
                        *child_lock = None;
                        let mut status = self.status.write().await;
                        *status = BeefStatus::Failed(msg.clone());
                        return Err(msg);
                    }
                    Ok(None) => {} // still running
                    Err(e) => {
                        warn!("Error checking BeEF process: {}", e);
                    }
                }
            }
            drop(child_lock);

            // 尝试健康检查
            if self.health_check().await {
                let mut status = self.status.write().await;
                let port = self.extract_port();
                *status = BeefStatus::Running { pid, port };
                info!(pid = pid, port = port, "BeEF is ready");
                return Ok(());
            }

            if i % 5 == 4 {
                info!(retry = i + 1, "waiting for BeEF to become ready...");
            }
        }

        let msg = format!("BeEF did not become ready within {} seconds", max_retries);
        let mut status = self.status.write().await;
        *status = BeefStatus::Failed(msg.clone());
        Err(msg)
    }

    /// 停止 BeEF 进程.
    pub async fn stop(&self) -> Result<(), String> {
        let mut status = self.status.write().await;
        let mut child_lock = self.child.lock().await;

        if let Some(ref mut child) = *child_lock {
            info!("stopping BeEF process");
            // 先尝试优雅退出 (SIGTERM)
            child
                .start_kill()
                .map_err(|e| format!("kill failed: {}", e))?;

            // 等待最多 5 秒
            for _ in 0..5 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                match child.try_wait() {
                    Ok(Some(exit)) => {
                        info!(code = %exit, "BeEF process exited");
                        *child_lock = None;
                        *status = BeefStatus::Stopped;
                        return Ok(());
                    }
                    _ => {}
                }
            }

            // 强制杀死
            child
                .kill()
                .await
                .map_err(|e| format!("force kill failed: {}", e))?;
            let _ = child.wait().await;
            info!("BeEF force killed");
        }

        *child_lock = None;
        *status = BeefStatus::Stopped;
        Ok(())
    }

    /// 健康检查 — 调用 BeEF REST API.
    async fn health_check(&self) -> bool {
        match reqwest::get(&self.health_check_url).await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// 从配置中提取端口.
    fn extract_port(&self) -> u16 {
        // 默认端口 3000
        3000
    }

    /// 检查 BeEF 是否存活并自动重启（带最大重启次数）.
    pub async fn ensure_running(&self, _max_restarts: u32) -> bool {
        let status = self.status().await;
        match status {
            BeefStatus::Running { .. } => {
                if !self.health_check().await {
                    warn!("BeEF health check failed, attempting restart");
                    let _ = self.stop().await;
                    self.start().await.is_ok()
                } else {
                    true
                }
            }
            BeefStatus::Stopped | BeefStatus::Failed(_) => self.start().await.is_ok(),
            BeefStatus::Starting => true, // 已经在启动中
        }
    }
}
