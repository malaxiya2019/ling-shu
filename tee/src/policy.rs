//! TEE 策略引擎 — 哪些操作必须由 TEE 保护.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// TEE 策略引擎.
#[derive(Clone, Debug)]
pub struct TeePolicyEngine {
    /// 需要 TEE 保护的操作列表.
    required_operations: HashSet<String>,
    /// 是否强制要求 TEE (如果为 true, 无 TEE 时拒绝执行)
    pub enforce: bool,
}

impl Default for TeePolicyEngine {
    fn default() -> Self {
        let mut ops = HashSet::new();
        ops.insert("decrypt_api_key".into());
        ops.insert("sign_jwt".into());
        ops.insert("process_private_data".into());
        ops.insert("tee_attestation".into());

        Self {
            required_operations: ops,
            enforce: false, // 默认不强制 (向后兼容)
        }
    }
}

impl TeePolicyEngine {
    /// 注册需要 TEE 保护的操作.
    pub fn register_required(&mut self, operation: &str) {
        self.required_operations.insert(operation.to_string());
    }

    /// 撤销 TEE 保护要求.
    pub fn unregister_required(&mut self, operation: &str) {
        self.required_operations.remove(operation);
    }

    /// 检查操作是否需要在 TEE 内执行.
    pub fn requires_tee(&self, operation: &str) -> bool {
        self.required_operations.contains(operation)
    }

    /// 检查操作是否允许在非 TEE 环境下执行.
    pub fn allowed_without_tee(&self, operation: &str) -> bool {
        !self.requires_tee(operation) || !self.enforce
    }

    /// 设置强制模式.
    pub fn set_enforce(&mut self, enforce: bool) {
        self.enforce = enforce;
    }

    /// 获取策略配置.
    pub fn policy_config(&self) -> TeePolicyConfig {
        TeePolicyConfig {
            required_operations: self.required_operations.iter().cloned().collect(),
            enforce: self.enforce,
        }
    }
}

/// TEE 策略配置 (可序列化, 用于 API).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TeePolicyConfig {
    pub required_operations: Vec<String>,
    pub enforce: bool,
}

impl TeePolicyConfig {
    pub fn default_production() -> Self {
        Self {
            required_operations: vec![
                "decrypt_api_key".into(),
                "sign_jwt".into(),
                "process_private_data".into(),
                "tee_attestation".into(),
            ],
            enforce: true,
        }
    }
}
