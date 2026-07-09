//! 远程证明 (Remote Attestation)
//!
//! SGX: EPID/DCAP 远程证明报告验证
//! TDX: Intel TDX 远程证明报告验证

use crate::TeePlatform;
use async_trait::async_trait;
use lingshu_core::LsResult;
use serde::{Deserialize, Serialize};

/// 远程证明报告.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttestationReport {
    pub platform: TeePlatform,
    pub nonce: String,
    /// 证明引用 (hex 编码)
    pub quote_hex: String,
    /// 是否通过验证
    pub is_valid: bool,
    pub timestamp: String,
    pub details: String,
}

/// 远程证明 Trait.
#[async_trait]
pub trait AttestationProvider: Send + Sync {
    /// 执行远程证明, 返回证明报告.
    async fn attest(&self, nonce: &str) -> LsResult<AttestationReport>;
    /// 验证外部证明引用.
    async fn verify_quote(&self, quote_hex: &str, nonce: &str) -> LsResult<bool>;
}

// ── SGX Manager ─────────────────────────────────────

/// Intel SGX Enclave 管理器.
pub struct SgxManager;

impl SgxManager {
    pub fn new() -> LsResult<Self> {
        tracing::info!("SGX manager initialized");
        Ok(Self)
    }

    /// 获取 SGX 平台信息.
    pub fn platform_info(&self) -> SgxPlatformInfo {
        SgxPlatformInfo {
            epc_size_mb: 64,
            supported: true,
        }
    }
}

/// SGX 平台信息.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SgxPlatformInfo {
    pub epc_size_mb: u64,
    pub supported: bool,
}

#[async_trait]
impl AttestationProvider for SgxManager {
    async fn attest(&self, nonce: &str) -> LsResult<AttestationReport> {
        // 实际实现会调用 SGX SDK 生成签名引用
        // 这里提供模拟实现 — 需要 Intel SGX SDK 才能完整编译
        let quote_hex = format!("sgx_quote_mock_{}", nonce);
        tracing::info!(nonce, "SGX attestation (mock)");

        Ok(AttestationReport {
            platform: TeePlatform::Sgx,
            nonce: nonce.to_string(),
            quote_hex,
            is_valid: true,
            timestamp: chrono::Utc::now().to_rfc3339(),
            details: "SGX attestation completed (software mock). Install Intel SGX SDK for production.".into(),
        })
    }

    async fn verify_quote(&self, quote_hex: &str, nonce: &str) -> LsResult<bool> {
        // 模拟验证: 检查格式
        let expected = format!("sgx_quote_mock_{}", nonce);
        Ok(quote_hex == expected)
    }
}

// ── TDX Manager ─────────────────────────────────────

/// Intel TDX 信任域管理器.
pub struct TdxManager;

impl TdxManager {
    pub fn new() -> LsResult<Self> {
        tracing::info!("TDX manager initialized");
        Ok(Self)
    }
}

#[async_trait]
impl AttestationProvider for TdxManager {
    async fn attest(&self, nonce: &str) -> LsResult<AttestationReport> {
        let quote_hex = format!("tdx_quote_mock_{}", nonce);
        tracing::info!(nonce, "TDX attestation (mock)");

        Ok(AttestationReport {
            platform: TeePlatform::Tdx,
            nonce: nonce.to_string(),
            quote_hex,
            is_valid: true,
            timestamp: chrono::Utc::now().to_rfc3339(),
            details: "TDX attestation completed (software mock). Install Intel TDX SDK for production.".into(),
        })
    }

    async fn verify_quote(&self, quote_hex: &str, nonce: &str) -> LsResult<bool> {
        let expected = format!("tdx_quote_mock_{}", nonce);
        Ok(quote_hex == expected)
    }
}
