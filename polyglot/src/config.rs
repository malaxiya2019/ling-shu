use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyglotConfig {
    /// Default execution timeout in seconds
    pub default_timeout: u64,
    /// Maximum output size in bytes
    pub max_output_bytes: usize,
    /// Whether to enable sandbox (requires appropriate system support)
    pub sandbox_enabled: bool,
    /// Additional environment variables
    pub env_vars: std::collections::HashMap<String, String>,
}

impl Default for PolyglotConfig {
    fn default() -> Self {
        Self {
            default_timeout: 30,
            max_output_bytes: 1_048_576, // 1MB
            sandbox_enabled: false,
            env_vars: std::collections::HashMap::new(),
        }
    }
}
