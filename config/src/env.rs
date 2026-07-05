/// 环境标识.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Dev,
    Test,
    Prod,
}

impl Environment {
    pub fn as_str(&self) -> &str {
        match self {
            Environment::Dev => "dev",
            Environment::Test => "test",
            Environment::Prod => "prod",
        }
    }

    /// 日志级别.
    pub fn log_level(&self) -> &str {
        match self {
            Environment::Dev => "debug",
            Environment::Test => "info",
            Environment::Prod => "warn",
        }
    }

    /// 是否启用严格权限.
    pub fn strict_permissions(&self) -> bool {
        matches!(self, Environment::Test | Environment::Prod)
    }

    /// 是否启用全程审计.
    pub fn full_audit(&self) -> bool {
        matches!(self, Environment::Prod)
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Environment {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dev" | "development" => Ok(Environment::Dev),
            "test" | "testing" => Ok(Environment::Test),
            "prod" | "production" => Ok(Environment::Prod),
            _ => Err(format!("unknown environment: {s} (expected dev/test/prod)")),
        }
    }
}

/// 读取环境标识，默认 dev.
pub fn current_environment() -> Environment {
    match std::env::var("LS_ENV").as_deref() {
        Ok("prod" | "production") => Environment::Prod,
        Ok("test" | "testing") => Environment::Test,
        _ => Environment::Dev,
    }
}

/// 读取环境变量 (带 LS_ 前缀).
pub fn env_value(key: &str) -> Option<String> {
    let prefixed = format!("LS_{}", key);
    std::env::var(&prefixed).ok()
}

/// 读取环境变量，返回解析值或默认.
pub fn env_value_or<T, F>(key: &str, parser: F, default: T) -> T
where
    F: FnOnce(&str) -> Option<T>,
{
    env_value(key)
        .as_deref()
        .and_then(parser)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_environment_default() {
        std::env::remove_var("LS_ENV");
        assert_eq!(current_environment(), Environment::Dev);
    }

    #[test]
    fn test_parse_from_str() {
        assert_eq!("dev".parse::<Environment>().unwrap(), Environment::Dev);
        assert_eq!("prod".parse::<Environment>().unwrap(), Environment::Prod);
    }

    #[test]
    fn test_env_value() {
        unsafe {
            std::env::set_var("LS_TEST_KEY", "hello");
        }
        // env_value lowercases the key before lookup
        assert_eq!(env_value("TEST_KEY").as_deref(), Some("hello"));
        assert_eq!(env_value("nonexistent"), None);
        unsafe {
            std::env::remove_var("LS_TEST_KEY");
        }
    }
}
