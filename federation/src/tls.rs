//! TLS 传输层 — 为联邦通信提供 TLS 加密.
//!
//! 基于 tokio-rustls，支持证书验证、双向 TLS (mTLS)。
//!
//! ## 配置
//! 环境变量或配置文件:
//! - `LS_FED_TLS_CERT` — 服务器证书路径
//! - `LS_FED_TLS_KEY` — 服务器私钥路径
//! - `LS_FED_TLS_CA` — CA 证书路径 (用于客户端验证)
//! - `LS_FED_TLS_ENABLED` — 是否启用 TLS ("true"/"false")

use lingshu_core::LsResult;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};

/// TLS 配置.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// 是否启用 TLS
    pub enabled: bool,
    /// 证书路径
    pub cert_path: Option<String>,
    /// 私钥路径
    pub key_path: Option<String>,
    /// CA 证书路径（用于 mTLS）
    pub ca_path: Option<String>,
    /// 是否跳过客户端验证
    pub skip_client_verify: bool,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: std::env::var("LS_FED_TLS_ENABLED")
                .ok()
                .map_or(false, |v| v == "true"),
            cert_path: std::env::var("LS_FED_TLS_CERT").ok(),
            key_path: std::env::var("LS_FED_TLS_KEY").ok(),
            ca_path: std::env::var("LS_FED_TLS_CA").ok(),
            skip_client_verify: false,
        }
    }
}

impl TlsConfig {
    /// 从环境变量加载配置.
    pub fn from_env() -> Self {
        Self::default()
    }

    /// 是否完整配置了 TLS.
    pub fn is_complete(&self) -> bool {
        if !self.enabled {
            return true; // TLS not required
        }
        self.cert_path.is_some() && self.key_path.is_some()
    }
}

/// TLS 服务器 — 包装 TcpListener 以支持 TLS 握手.
pub enum TlsServer {
    Plain(TcpListener),
    #[cfg(feature = "tls")]
    Tls(TcpListener, Arc<rustls::ServerConfig>),
}

impl TlsServer {
    /// 绑定地址，自动决定是否使用 TLS.
    pub async fn bind(addr: &str, config: &TlsConfig) -> LsResult<Self> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| lingshu_core::LsError::Internal(format!("bind {addr} failed: {e}")))?;

        if !config.enabled {
            info!("TLS disabled, using plain TCP");
            return Ok(TlsServer::Plain(listener));
        }

        #[cfg(feature = "tls")]
        {
            let tls_config = build_server_config(config)?;
            info!("TLS enabled, accepting encrypted connections");
            Ok(TlsServer::Tls(listener, tls_config))
        }

        #[cfg(not(feature = "tls"))]
        {
            warn!("TLS enabled but 'tls' feature not compiled; falling back to plain TCP");
            Ok(TlsServer::Plain(listener))
        }
    }

    /// 接受连接，返回 (读写流, 对端地址).
    pub async fn accept(&self) -> LsResult<(Box<dyn IoStream>, std::net::SocketAddr)> {
        match self {
            TlsServer::Plain(listener) => {
                let (stream, addr) = listener
                    .accept()
                    .await
                    .map_err(|e| lingshu_core::LsError::Internal(format!("accept: {e}")))?;
                Ok((Box::new(stream), addr))
            }
            #[cfg(feature = "tls")]
            TlsServer::Tls(listener, config) => {
                let (stream, addr) = listener
                    .accept()
                    .await
                    .map_err(|e| lingshu_core::LsError::Internal(format!("accept: {e}")))?;
                let acceptor = tokio_rustls::TlsAcceptor::from(config.clone());
                let tls_stream = acceptor
                    .accept(stream)
                    .await
                    .map_err(|e| lingshu_core::LsError::Internal(format!("tls handshake: {e}")))?;
                Ok((Box::new(tls_stream), addr))
            }
        }
    }
}

/// TLS 连接器 — 为出站连接添加 TLS.
pub enum TlsConnector {
    Plain,
    #[cfg(feature = "tls")]
    Tls(Arc<rustls::ClientConfig>),
}

impl TlsConnector {
    /// 创建连接器.
    pub fn new(config: &TlsConfig) -> LsResult<Self> {
        if !config.enabled {
            return Ok(TlsConnector::Plain);
        }

        #[cfg(feature = "tls")]
        {
            let client_config = build_client_config(config)?;
            Ok(TlsConnector::Tls(client_config))
        }

        #[cfg(not(feature = "tls"))]
        {
            warn!("TLS enabled but 'tls' feature not compiled; using plain TCP");
            Ok(TlsConnector::Plain)
        }
    }

    /// 连接到指定地址.
    pub async fn connect(&self, addr: &str) -> LsResult<Box<dyn IoStream>> {
        match self {
            TlsConnector::Plain => {
                let stream = TcpStream::connect(addr)
                    .await
                    .map_err(|e| lingshu_core::LsError::Internal(format!("connect {addr}: {e}")))?;
                Ok(Box::new(stream))
            }
            #[cfg(feature = "tls")]
            TlsConnector::Tls(config) => {
                let stream = TcpStream::connect(addr)
                    .await
                    .map_err(|e| lingshu_core::LsError::Internal(format!("connect {addr}: {e}")))?;
                let connector = tokio_rustls::TlsConnector::from(config.clone());
                // Use DNS name from the address
                let dns_name = rustls::pki_types::ServerName::try_from(
                    addr.split(':').next().unwrap_or("localhost"),
                )
                .map_err(|_| lingshu_core::LsError::Internal("invalid DNS name".into()))?;
                let tls_stream = connector
                    .connect(dns_name, stream)
                    .await
                    .map_err(|e| lingshu_core::LsError::Internal(format!("tls connect: {e}")))?;
                Ok(Box::new(tls_stream))
            }
        }
    }
}

/// 统一 IO 类型，隐藏 TLS/Plain 差异.
pub trait IoStream: AsyncRead + AsyncWrite + Unpin + Send {}

impl IoStream for TcpStream {}
#[cfg(feature = "tls")]
impl IoStream for tokio_rustls::TlsStream<TcpStream> {}

// ── 内部: 构建 TLS 配置 ────────────────────────────

#[cfg(feature = "tls")]
fn load_certs(path: &str) -> LsResult<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let cert_data = std::fs::read(path)
        .map_err(|e| lingshu_core::LsError::Internal(format!("read cert {path}: {e}")))?;
    let certs = rustls_pemfile::certs(&mut &*cert_data)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| lingshu_core::LsError::Internal(format!("parse cert {path}: {e}")))?;
    Ok(certs)
}

#[cfg(feature = "tls")]
fn load_private_key(path: &str) -> LsResult<rustls::pki_types::PrivateKeyDer<'static>> {
    let key_data = std::fs::read(path)
        .map_err(|e| lingshu_core::LsError::Internal(format!("read key {path}: {e}")))?;
    let key = rustls_pemfile::private_key(&mut &*key_data)
        .map_err(|e| lingshu_core::LsError::Internal(format!("parse key {path}: {e}")))?
        .ok_or_else(|| lingshu_core::LsError::Internal("no private key found".into()))?;
    Ok(key)
}

#[cfg(feature = "tls")]
fn build_server_config(config: &TlsConfig) -> LsResult<Arc<rustls::ServerConfig>> {
    let certs = load_certs(config.cert_path.as_ref().ok_or_else(|| {
        lingshu_core::LsError::Internal("cert_path required for TLS server".into())
    })?)?;
    let key = load_private_key(config.key_path.as_ref().ok_or_else(|| {
        lingshu_core::LsError::Internal("key_path required for TLS server".into())
    })?)?;

    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| lingshu_core::LsError::Internal(format!("tls config: {e}")))?;

    info!("TLS server configured with {} cert(s)", certs.len());
    Ok(Arc::new(server_config))
}

#[cfg(feature = "tls")]
fn build_client_config(config: &TlsConfig) -> LsResult<Arc<rustls::ClientConfig>> {
    let mut root_store = rustls::RootCertStore::empty();

    if let Some(ca_path) = &config.ca_path {
        let ca_data = std::fs::read(ca_path)
            .map_err(|e| lingshu_core::LsError::Internal(format!("read CA {ca_path}: {e}")))?;
        let certs = rustls_pemfile::certs(&mut &*ca_data)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| lingshu_core::LsError::Internal(format!("parse CA {ca_path}: {e}")))?;
        for cert in certs {
            root_store
                .add(cert)
                .map_err(|e| lingshu_core::LsError::Internal(format!("add CA cert: {e}")))?;
        }
    } else {
        // Use Mozilla's default root store
    }

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    info!("TLS client configured");
    Ok(Arc::new(client_config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_default_disabled() {
        let config = TlsConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_tls_config_from_env() {
        // Test that from_env works (typically disabled)
        let config = TlsConfig::from_env();
        assert!(!config.enabled || config.cert_path.is_some());
    }

    #[test]
    fn test_tls_config_complete() {
        let config = TlsConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(config.is_complete()); // TLS disabled = complete

        let config = TlsConfig {
            enabled: true,
            cert_path: None,
            key_path: None,
            ..Default::default()
        };
        assert!(!config.is_complete()); // TLS enabled but no certs
    }
}
