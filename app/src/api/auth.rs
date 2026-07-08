//! 认证端点 (Login / Logout / Admin Auth)
//!
//! ✅ 已完成迁移 (从 full.rs)

use crate::api::AppState;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 管理面板登录请求
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 管理面板登录响应
#[derive(Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub token: Option<String>,
    pub message: String,
}

/// POST /v1/login — 管理面板登录
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Json<LoginResponse> {
    // 验证管理员密码
    let config = &state.runtime.config;
    let admin_password = config
        .extra
        .get("admin_password")
        .and_then(|v| v.as_str())
        .unwrap_or("admin");

    if req.password == admin_password && req.username == "admin" {
        // 生成 JWT token
        match state.jwt_service.generate_token(
            &req.username,
            Some(86400), // 24h expiry
        ) {
            Ok(token) => Json(LoginResponse {
                success: true,
                token: Some(token),
                message: "Login successful".into(),
            }),
            Err(e) => Json(LoginResponse {
                success: false,
                token: None,
                message: format!("Token generation failed: {e}"),
            }),
        }
    } else {
        Json(LoginResponse {
            success: false,
            token: None,
            message: "Invalid credentials".into(),
        })
    }
}

/// POST /v1/logout — 管理面板登出
pub async fn logout_handler() -> (StatusCode, [(&'static str, &'static str); 2], &'static str) {
    let headers = [("Set-Cookie", "token=; Max-Age=0; Path=/; HttpOnly"), ("Content-Type", "text/plain")];
    (StatusCode::OK, headers, "Logged out")
}

/// Axum route definition for Auth module
pub fn auth_routes() -> axum::Router<Arc<AppState>> {
    axum::Router::new()
        .route("/v1/login", axum::routing::post(login_handler))
        .route("/v1/logout", axum::routing::post(logout_handler))
}
