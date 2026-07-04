//! Proxmox VE REST API 客户端。
//!
//! PVE API 特性：
//! - 基础 URL: `https://{host}:{port}/api2/json/`
//! - 认证: `Authorization: PVEAPIToken={token_id}={token_secret}`
//! - 所有响应包裹在 `{"data": ...}` 信封中
//! - POST/PUT/DELETE 使用 `application/x-www-form-urlencoded` 请求体
//! - 异步任务返回 UPID 字符串（而非立即结果）

use reqwest::Method;
use serde_json::Value;

use crate::{
    app_config::ProxmoxHostConfig,
    error::{AppError, AppResult},
    http_client, network,
};

pub fn web_url(host: &ProxmoxHostConfig) -> String {
    format!("https://{}", network::authority(&host.host, host.port))
}

// ─── 公开请求函数 ─────────────────────────────────────────────────────────────

/// GET 请求，返回 PVE data 字段内容。
pub async fn get(host: &ProxmoxHostConfig, path: &str) -> AppResult<Value> {
    request(host, Method::GET, path, &[]).await
}

/// 验证网络、TLS 和 API Token 均可用。
pub async fn check_connection(host: &ProxmoxHostConfig) -> Result<(), String> {
    get(host, "version")
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// POST 请求（带 form 参数）。
pub async fn post(
    host: &ProxmoxHostConfig,
    path: &str,
    params: &[(&str, &str)],
) -> AppResult<Value> {
    request(host, Method::POST, path, params).await
}

/// DELETE 请求（带可选 form 参数）。
pub async fn delete(
    host: &ProxmoxHostConfig,
    path: &str,
    params: &[(&str, &str)],
) -> AppResult<Value> {
    request(host, Method::DELETE, path, params).await
}

// ─── 内部实现 ─────────────────────────────────────────────────────────────────

async fn request(
    host: &ProxmoxHostConfig,
    method: Method,
    path: &str,
    params: &[(&str, &str)],
) -> AppResult<Value> {
    let path = path.trim_matches('/');
    if path.is_empty()
        || path.split('/').any(|segment| {
            segment.is_empty()
                || matches!(segment, "." | "..")
                || segment.contains(['\\', '?', '#', '\r', '\n', '\0'])
        })
    {
        return Err(AppError::BadRequest("invalid PVE API path".to_string()));
    }
    let url = format!(
        "https://{}:{}/api2/json/{}",
        network::url_host(&host.host),
        host.port,
        path
    );
    let auth = format!("PVEAPIToken={}={}", host.token_id, host.token_secret);

    let client = http_client::for_tls(host.verify_tls)?;
    let mut builder = client
        .request(method.clone(), &url)
        .header("Authorization", &auth);

    if !params.is_empty() {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params.iter().copied())
            .finish();
        builder = builder
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("PVE 连接失败: {e}")))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| AppError::Upstream(format!("读取 PVE 响应失败: {error}")))?;

    if !status.is_success() {
        let msg = extract_error_message(&text, status.as_u16());
        return Err(AppError::Upstream(msg));
    }

    if text.is_empty() {
        return Ok(Value::Null);
    }

    let envelope: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::Upstream(format!("PVE 响应解析失败: {e}")))?;

    // PVE 所有响应都包裹在 {"data": ...} 中
    Ok(envelope.get("data").cloned().unwrap_or(envelope))
}

/// 从 PVE 错误响应中提取可读错误信息。
fn extract_error_message(body: &str, status: u16) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(errors) = v.get("errors") {
            return format!("PVE API 错误 {status}: {errors}");
        }
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            return format!("PVE API 错误 {status}: {msg}");
        }
    }
    format!("PVE API 返回 {status}: {body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_ipv6_in_web_urls() {
        let mut host = ProxmoxHostConfig {
            host: "::1".to_string(),
            ..Default::default()
        };
        assert_eq!(web_url(&host), "https://[::1]:8006");
        host.host = "[::1]".to_string();
        assert_eq!(web_url(&host), "https://[::1]:8006");
    }
}
