use serde::{Deserialize, Serialize};

/// 账号密码登录请求。
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// 登录成功响应。
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct UserInfoResponse {
    pub username: String,
}

/// 有效期 60 秒、仅用于 SSE 连接认证的短效票据。
#[derive(Debug, Serialize)]
pub struct SseTicketResponse {
    pub ticket: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}
