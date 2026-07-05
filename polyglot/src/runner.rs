use crate::config::PolyglotConfig;
use crate::detect::Language;
use crate::error::PolyglotResult;
use async_trait::async_trait;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, error};

/// LanguageRunner trait — each language implements this
#[async_trait]
pub trait LanguageRunner: Send + Sync {
    fn language_name(&self) -> &'static str;
    fn language(&self) -> Language;
    fn file_extension(&self) -> &'static str;
    async fn run(&self, code: &str, config: &PolyglotConfig) -> PolyglotResult<String>;
}

/// Helper: execute a shell command with timeout and output limit
pub async fn execute_command(
    cmd: &mut Command,
    config: &PolyglotConfig,
) -> PolyglotResult<String> {
    let timed = timeout(Duration::from_secs(config.default_timeout), cmd.output());

    match timed.await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if !output.status.success() {
                let msg = if stderr.is_empty() { stdout.clone() } else { stderr };
                return Err(crate::error::PolyglotError::ExecutionFailed(msg));
            }

            if stdout.len() > config.max_output_bytes {
                return Err(crate::error::PolyglotError::OutputLimitExceeded(
                    config.max_output_bytes,
                ));
            }

            debug!("Command succeeded ({} bytes)", stdout.len());
            Ok(stdout)
        }
        Ok(Err(e)) => {
            error!("IO error executing command: {}", e);
            Err(crate::error::PolyglotError::Io(e))
        }
        Err(_) => {
            error!("Command timed out after {}s", config.default_timeout);
            Err(crate::error::PolyglotError::Timeout(config.default_timeout))
        }
    }
}
