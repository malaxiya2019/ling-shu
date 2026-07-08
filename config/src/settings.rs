use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::env::{current_environment, Environment};

/// LLM 提供商类型.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LlmProvider {
    #[serde(rename = "openai")]
    #[default]
    Openai,
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "groq")]
    Groq,
    #[serde(rename = "mock")]
    Mock,
    #[serde(rename = "llmkit")]
    Llmkit,
}

impl LlmProvider {
    /// 从环境变量 `LLM_PROVIDER` 或 `LS_LLM_PROVIDER` 读取提供商类型.
    pub fn from_env() -> Self {
        std::env::var("LLM_PROVIDER")
            .or_else(|_| std::env::var("LS_LLM_PROVIDER"))
            .ok()
            .and_then(|s| match s.to_lowercase().as_str() {
                "openai" => Some(Self::Openai),
                "anthropic" => Some(Self::Anthropic),
                "groq" => Some(Self::Groq),
                "mock" => Some(Self::Mock),
                "llmkit" => Some(Self::Llmkit),
                _ => None,
            })
            .unwrap_or(Self::Openai)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Openai => "openai",
            Self::Anthropic => "anthropic",
            Self::Groq => "groq",
            Self::Mock => "mock",
            Self::Llmkit => "llmkit",
        }
    }
}

impl std::fmt::Display for LlmProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 顶层配置结构 — 与 config/{dev,test,prod}.yaml 字段一一对应.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LsConfig {
    pub runtime: RuntimeConfig,
    pub eventbus: EventBusConfig,
    pub security: SecurityConfig,
    pub llm: LlmConfig,
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
}

// ── 各模块配置 ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    pub max_concurrent_tasks: usize,
    pub session_ttl_seconds: u64,
    pub enable_snapshot: bool,
    pub federation_enabled: bool,
    pub federation_port: u16,
    pub cluster_name: String,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 64,
            session_ttl_seconds: 3600,
            enable_snapshot: true,
            federation_enabled: false,
            federation_port: 9550,
            cluster_name: "lingshu-default".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EventBusConfig {
    pub max_retries: u32,
    pub retention_days: u64,
    pub audit_retention_days: u64,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retention_days: 7,
            audit_retention_days: 180,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub default_isolation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwt_secret: Option<String>,
    pub enable_audit: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            default_isolation: "session".into(),
            jwt_secret: None,
            enable_audit: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// 提供商类型 (openai / anthropic / groq / mock / llmkit).
    pub provider: LlmProvider,
    /// llmkit 内部提供商名 (仅 provider=llmkit 时生效).
    /// 可选: anthropic, openai, google, mistral, groq, deepseek, ollama, 等 27+.
    pub llmkit_provider: String,
    /// 默认模型名称 (各提供商不同).
    pub default_model: String,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    /// 环境变量中读取的 API 密钥 (运行时注入, 不序列化到 YAML).
    #[serde(skip)]
    pub api_key: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Openai,
            llmkit_provider: "anthropic".into(),
            default_model: "gpt-4o".into(),
            max_tokens: 4096,
            timeout_seconds: 120,
            api_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub provider: String,
    pub bucket: String,
    pub region: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            provider: "local".into(),
            bucket: "lingshu-data".into(),
            region: "us-east-1".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: None,
            max_connections: 10,
        }
    }
}

// ── 配置加载器 ──────────────────────────────────────

/// 配置加载器 — 支持 默认值 → YAML → 环境变量 三层叠加.
pub struct ConfigLoader {
    search_paths: Vec<PathBuf>,
}

impl ConfigLoader {
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }

    /// 在当前工作目录以及 `./config/` 子目录中搜索.
    pub fn with_cwd() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default();
        let cwd_config = cwd.join("config");
        Self {
            search_paths: vec![cwd, cwd_config],
        }
    }

    /// 加载配置: 默认值 → YAML → 环境变量.
    pub fn load(&self, env: Option<Environment>) -> LsResult<LsConfig> {
        let env = env.unwrap_or_else(current_environment);
        let mut config = LsConfig::default();

        if let Some(yaml_path) = self.resolve_yaml(env) {
            match self.load_yaml(&yaml_path) {
                Ok(file_config) => {
                    tracing::info!(path = %yaml_path.display(), "loaded config");
                    config = merge_configs(config, file_config);
                }
                Err(e) => {
                    tracing::warn!(path = %yaml_path.display(), error = %e, "yaml load failed");
                }
            }
        }

        apply_env_overrides(&mut config);
        Ok(config)
    }

    pub fn load_default() -> LsResult<LsConfig> {
        Self::with_cwd().load(None)
    }

    fn resolve_yaml(&self, env: Environment) -> Option<PathBuf> {
        let filename = format!("{}.yaml", env.as_str());
        for dir in &self.search_paths {
            let path = dir.join(&filename);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    fn load_yaml(&self, path: &Path) -> Result<LsConfig, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
        serde_yaml::from_str(&content).map_err(|e| format!("parse error: {e}"))
    }
}

/// 字段级合并.
fn merge_configs(base: LsConfig, overlay: LsConfig) -> LsConfig {
    let base_val = serde_json::to_value(&base).unwrap_or_default();
    let overlay_val = serde_json::to_value(&overlay).unwrap_or_default();
    serde_json::from_value(deep_merge(base_val, overlay_val)).unwrap_or(base)
}

fn deep_merge(base: serde_json::Value, overlay: serde_json::Value) -> serde_json::Value {
    match (base, overlay) {
        (serde_json::Value::Object(mut base_map), serde_json::Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                if v.is_null() {
                    continue;
                }
                if v.is_object() {
                    let child = base_map
                        .remove(&k)
                        .unwrap_or(serde_json::Value::Object(Default::default()));
                    base_map.insert(k, deep_merge(child, v));
                } else {
                    base_map.insert(k, v);
                }
            }
            serde_json::Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}

/// 环境变量覆写.
fn apply_env_overrides(config: &mut LsConfig) {
    let env_map: HashMap<String, String> = std::env::vars()
        .filter(|(k, _)| k.starts_with("LS_"))
        .map(|(k, v)| (k.trim_start_matches("LS_").to_lowercase(), v))
        .collect();
    if env_map.is_empty() {
        return;
    }

    if let Some(v) = env_map.get("runtime_max_concurrent_tasks") {
        if let Ok(n) = v.parse() {
            config.runtime.max_concurrent_tasks = n;
        }
    }
    if let Some(v) = env_map.get("runtime_session_ttl_seconds") {
        if let Ok(n) = v.parse() {
            config.runtime.session_ttl_seconds = n;
        }
    }
    if let Some(v) = env_map.get("eventbus_max_retries") {
        if let Ok(n) = v.parse() {
            config.eventbus.max_retries = n;
        }
    }
    if let Some(v) = env_map.get("eventbus_retention_days") {
        if let Ok(n) = v.parse() {
            config.eventbus.retention_days = n;
        }
    }
    if let Some(v) = env_map.get("security_jwt_secret") {
        config.security.jwt_secret = Some(v.clone());
    }
    if let Some(v) = env_map.get("security_default_isolation") {
        config.security.default_isolation = v.clone();
    }
    if let Some(v) = env_map.get("llm_provider") {
        match v.to_lowercase().as_str() {
            "openai" => config.llm.provider = LlmProvider::Openai,
            "anthropic" => config.llm.provider = LlmProvider::Anthropic,
            "groq" => config.llm.provider = LlmProvider::Groq,
            "mock" => config.llm.provider = LlmProvider::Mock,
            "llmkit" => config.llm.provider = LlmProvider::Llmkit,
            _ => {}
        }
    }
    // 也支持无 LS_ 前缀的 LLM_PROVIDER
    if let Ok(v) = std::env::var("LLM_PROVIDER") {
        match v.to_lowercase().as_str() {
            "openai" => config.llm.provider = LlmProvider::Openai,
            "anthropic" => config.llm.provider = LlmProvider::Anthropic,
            "groq" => config.llm.provider = LlmProvider::Groq,
            "mock" => config.llm.provider = LlmProvider::Mock,
            "llmkit" => config.llm.provider = LlmProvider::Llmkit,
            _ => {}
        }
    }
    // API 密钥仅通过环境变量注入
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        config.llm.api_key = Some(key);
    }
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        config.llm.api_key = Some(key);
    }
    if let Ok(key) = std::env::var("GROQ_API_KEY") {
        config.llm.api_key = Some(key);
    }
    if let Some(v) = env_map.get("llm_default_model") {
        config.llm.default_model = v.clone();
    }
    if let Some(v) = env_map.get("llm_max_tokens") {
        if let Ok(n) = v.parse() {
            config.llm.max_tokens = n;
        }
    }
    if let Some(v) = env_map.get("storage_provider") {
        config.storage.provider = v.clone();
    }
    if let Some(v) = env_map.get("storage_bucket") {
        config.storage.bucket = v.clone();
    }
    if let Some(v) = env_map.get("database_url") {
        config.database.url = Some(v.clone());
    }
    if let Some(v) = env_map.get("database_max_connections") {
        if let Ok(n) = v.parse() {
            config.database.max_connections = n;
        }
    }
}

impl LsConfig {
    pub fn load() -> LsResult<Self> {
        ConfigLoader::load_default()
    }

    pub fn load_for_env(env: &str) -> LsResult<Self> {
        let env: Environment = env.parse().unwrap_or(Environment::Dev);
        ConfigLoader::with_cwd().load(Some(env))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// 全局锁：确保所有需要干净环境的测试串行执行.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// 在干净的环境中运行闭包（所有 LS_ 环境变量被临时清除并恢复）.
    fn with_clean_env<F>(f: F)
    where
        F: FnOnce(),
    {
        let guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let old: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| k.starts_with("LS_"))
            .collect();
        for (k, _) in &old {
            std::env::remove_var(k);
        }
        // 防止 test_env_overrides 的残留
        f();
        for (k, v) in old {
            std::env::set_var(k, v);
        }
        drop(guard);
    }

    // ── 不依赖环境变量的测试 ──────────────────────────

    #[test]
    fn test_default_config() {
        let cfg = LsConfig::default();
        assert_eq!(cfg.runtime.max_concurrent_tasks, 64);
        assert_eq!(cfg.llm.default_model, "gpt-4o");
        assert_eq!(cfg.eventbus.audit_retention_days, 180);
    }

    #[test]
    fn test_yaml_parsing_directly() {
        let yaml = r#"
runtime:
  max_concurrent_tasks: 16
llm:
  default_model: gpt-4o-mini
storage:
  provider: minio
"#;
        let cfg: LsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.runtime.max_concurrent_tasks, 16);
        assert_eq!(cfg.llm.default_model, "gpt-4o-mini");
        assert_eq!(cfg.storage.provider, "minio");
        assert_eq!(cfg.runtime.session_ttl_seconds, 3600);
        assert_eq!(cfg.storage.bucket, "lingshu-data");
    }

    #[test]
    fn test_deep_merge() {
        let base = serde_json::json!({"a": 1, "b": {"c": 2, "d": 3}, "e": [1, 2]});
        let overlay = serde_json::json!({"b": {"c": 99}, "f": "new"});
        let merged = deep_merge(base, overlay);
        assert_eq!(merged["a"], 1);
        assert_eq!(merged["b"]["c"], 99);
        assert_eq!(merged["b"]["d"], 3);
        assert_eq!(merged["f"], "new");
    }

    #[test]
    fn test_environment_parse() {
        assert_eq!("dev".parse::<Environment>().unwrap(), Environment::Dev);
        assert_eq!("prod".parse::<Environment>().unwrap(), Environment::Prod);
        assert!("unknown".parse::<Environment>().is_err());
    }

    // ── 需要干净环境的测试 ────────────────────────────

    #[test]
    fn test_env_overrides() {
        with_clean_env(|| {
            std::env::set_var("LS_LLM_DEFAULT_MODEL", "o3-mini");
            std::env::set_var("LS_RUNTIME_MAX_CONCURRENT_TASKS", "99");
            let cfg = LsConfig::load().unwrap();
            assert_eq!(cfg.llm.default_model, "o3-mini");
            assert_eq!(cfg.runtime.max_concurrent_tasks, 99);
        });
    }

    #[test]
    fn test_real_yaml_files_in_repo() {
        with_clean_env(|| {
            let loader = ConfigLoader::with_cwd();
            let cfg = loader.load(Some(Environment::Dev)).unwrap();
            assert_eq!(cfg.runtime.max_concurrent_tasks, 16, "dev.yaml has 16");
            assert_eq!(cfg.llm.default_model, "gpt-4o-mini");
        });
    }

    #[test]
    fn test_prod_yaml_loadable() {
        with_clean_env(|| {
            let loader = ConfigLoader::with_cwd();
            let cfg = loader.load(Some(Environment::Prod)).unwrap();
            assert_eq!(cfg.runtime.max_concurrent_tasks, 128, "prod.yaml has 128");
            assert_eq!(cfg.llm.default_model, "gpt-4o");
        });
    }

    #[test]
    fn test_yaml_file_loading() {
        with_clean_env(|| {
            let dir = std::env::temp_dir().join(format!("lscfg_{}", std::process::id()));
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(
                dir.join("dev.yaml"),
                "runtime:\n  max_concurrent_tasks: 16\n",
            )
            .unwrap();
            let loader = ConfigLoader::new(vec![dir.clone()]);
            let cfg = loader.load(Some(Environment::Dev)).unwrap();
            assert_eq!(cfg.runtime.max_concurrent_tasks, 16);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    #[test]
    fn test_yaml_not_found_falls_back_to_defaults() {
        with_clean_env(|| {
            let loader = ConfigLoader::new(vec![PathBuf::from("/nonexistent_path_xyz")]);
            let cfg = loader.load(Some(Environment::Prod)).unwrap();
            assert_eq!(cfg.runtime.max_concurrent_tasks, 64);
            assert_eq!(cfg.llm.default_model, "gpt-4o");
        });
    }
}

// ── Config Hot Reload ────────────────────────────────

/// 配置变更通知.
#[derive(Debug, Clone)]
pub enum ConfigEvent {
    /// 配置已重新加载.
    Reloaded(LsConfig),
    /// 配置加载失败.
    Error(String),
}

/// 配置文件热重载监视器 (轮询模式).
///
/// 使用 `std::thread` 轮询配置文件修改时间，变更时通过 `std::sync::mpsc` 通知。
pub struct ConfigWatcher {
    #[allow(dead_code)]
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl ConfigWatcher {
    /// 启动配置监听 (后台 std::thread)。
    ///
    /// 当配置文件变化时，通过 `tx` 发送 `ConfigEvent`。
    pub fn spawn(env: &str, tx: std::sync::mpsc::Sender<ConfigEvent>) -> Self {
        let env_owned = env.to_string();
        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop_flag.clone();

        std::thread::spawn(move || {
            // 初始加载
            match LsConfig::load_for_env(&env_owned) {
                Ok(cfg) => {
                    let _ = tx.send(ConfigEvent::Reloaded(cfg));
                }
                Err(e) => {
                    let _ = tx.send(ConfigEvent::Error(e.to_string()));
                }
            }

            let mut last_mtime: Option<std::time::SystemTime> = None;
            let config_path = std::path::Path::new("config").join(format!("{}.yaml", env_owned));

            while !flag.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_secs(10));

                if let Ok(meta) = std::fs::metadata(&config_path) {
                    if let Ok(mtime) = meta.modified() {
                        if last_mtime.map(|t| mtime > t).unwrap_or(true) {
                            last_mtime = Some(mtime);
                            match LsConfig::load_for_env(&env_owned) {
                                Ok(cfg) => {
                                    let _ = tx.send(ConfigEvent::Reloaded(cfg));
                                }
                                Err(e) => {
                                    let _ = tx.send(ConfigEvent::Error(e.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        });

        Self { stop_flag }
    }
}

impl std::fmt::Debug for ConfigWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigWatcher").finish()
    }
}
