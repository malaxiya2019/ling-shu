//! LSConfig — 配置体系。
//!
//! 优先级: 配置中心 > 环境变量 > 配置文件 > 全局默认值
//!
//! 配置文件路径: `config/{dev/test/prod}.yaml`
//! 环境变量前缀: `LS_`
//! 敏感配置仅通过环境变量或密钥管理服务注入。

pub mod env;
pub mod settings;

pub use env::*;
pub use settings::*;
