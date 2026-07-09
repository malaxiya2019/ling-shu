//! LSTEE — 机密计算 (Confidential Computing) 支持
//!
//! ## 功能
//! - Intel SGX 远程证明验证
//! - Intel TDX 远程证明验证
//! - 加密内存区域 (Encrypted Memory Region)
//! - 平台检测: 是否支持 TEE
//! - 策略引擎: 哪些操作需要 TEE 保护
//!
//! ## 架构
//! ```text
//! ┌──────────────────────────────────────┐
//! │         TEE Enclave Manager          │
//! │  ┌─────────┐  ┌─────────┐           │
//! │  │ SGX     │  │ TDX     │           │
//! │  │ Manager │  │ Manager │           │
//! │  └─────────┘  └─────────┘           │
//! │  ┌──────────────────────────┐       │
//! │  │ Encrypted Memory Region  │       │
//! │  │ (AES-256-GCM in-memory)  │       │
//! │  └──────────────────────────┘       │
//! │  ┌──────────────────────────┐       │
//! │  │ TEE Policy Engine        │       │
//! │  └──────────────────────────┘       │
//! └──────────────────────────────────────┘
//! ```

mod attestation;
mod encrypted_memory;
mod policy;

pub use attestation::*;
pub use encrypted_memory::*;
pub use policy::*;

use lingshu_core::LsResult;
use std::sync::{Arc, RwLock};

/// TEE 平台类型.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TeePlatform {
    /// Intel Software Guard Extensions
    Sgx,
    /// Intel Trust Domain Extensions
    Tdx,
    /// 无 TEE 支持 (mock/软模式)
    None,
}

impl TeePlatform {
    pub fn is_available(&self) -> bool {
        !matches!(self, TeePlatform::None)
    }

    pub fn label(&self) -> &'static str {
        match self {
            TeePlatform::Sgx => "Intel SGX",
            TeePlatform::Tdx => "Intel TDX",
            TeePlatform::None => "None (Software Mode)",
        }
    }
}

/// TEE 系统主入口.
pub struct TeeSystem {
    pub platform: TeePlatform,
    pub sgx: Option<Arc<SgxManager>>,
    pub tdx: Option<Arc<TdxManager>>,
    pub encrypted_memory: Arc<EncryptedMemoryRegion>,
    pub policy_engine: Arc<RwLock<TeePolicyEngine>>,
}

impl TeeSystem {
    /// 初始化 TEE 系统, 自动检测平台.
    pub async fn initialize() -> LsResult<Self> {
        let platform = detect_platform().await;

        let sgx = if platform == TeePlatform::Sgx {
            Some(Arc::new(SgxManager::new()?))
        } else {
            None
        };

        let tdx = if platform == TeePlatform::Tdx {
            Some(Arc::new(TdxManager::new()?))
        } else {
            None
        };

        let encrypted_memory = Arc::new(EncryptedMemoryRegion::new());
        let policy_engine = Arc::new(RwLock::new(TeePolicyEngine::default()));

        tracing::info!(platform = ?platform, "TEE system initialized");

        Ok(Self {
            platform,
            sgx,
            tdx,
            encrypted_memory,
            policy_engine,
        })
    }

    /// 执行远程证明, 获取证明报告.
    pub async fn attest(&self, nonce: &str) -> LsResult<AttestationReport> {
        match self.platform {
            TeePlatform::Sgx => {
                if let Some(ref sgx) = self.sgx {
                    sgx.attest(nonce).await
                } else {
                    Err(lingshu_core::LsError::Internal("SGX not available".into()))
                }
            }
            TeePlatform::Tdx => {
                if let Some(ref tdx) = self.tdx {
                    tdx.attest(nonce).await
                } else {
                    Err(lingshu_core::LsError::Internal("TDX not available".into()))
                }
            }
            TeePlatform::None => {
                // Software mode: return mock attestation
                Ok(AttestationReport {
                    platform: TeePlatform::None,
                    nonce: nonce.to_string(),
                    quote_hex: "mock_quote_software_mode".into(),
                    is_valid: true,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    details: "Software mode — no hardware TEE".into(),
                })
            }
        }
    }
}

/// 检测当前平台支持的 TEE.
async fn detect_platform() -> TeePlatform {
    // 检查 /dev/sgx_enclave (Linux SGX)
    if std::path::Path::new("/dev/sgx_enclave").exists() {
        return TeePlatform::Sgx;
    }
    // 检查 /dev/tdx-guest (Intel TDX)
    if std::path::Path::new("/dev/tdx-guest").exists() {
        return TeePlatform::Tdx;
    }
    // 如果设置了环境变量 LINGSHU_TEE_FORCE
    if let Ok(force) = std::env::var("LINGSHU_TEE_FORCE") {
        match force.to_lowercase().as_str() {
            "sgx" => return TeePlatform::Sgx,
            "tdx" => return TeePlatform::Tdx,
            _ => {}
        }
    }
    TeePlatform::None
}
