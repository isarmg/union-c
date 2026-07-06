//! Sunshine API 客户端。
//!
//! 所有函数接收 `&SunshineHostConfig` 而非 `&AppState`，使调用方可以按主机 ID
//! 自由选择目标主机，天然支持多主机管理场景。
//!
//! # 代理模式说明
//!
//! 这个模块充当一个 HTTP 代理：前端把请求发给本应用（union），
//! 本应用再用 `reqwest`（Rust 的 HTTP 客户端库）把请求转发给 Sunshine 的 Web API。
//! 这样做的好处是：
//! 1. 前端只需要和 union 通信，不需要直接访问 Sunshine（避免跨域问题）
//! 2. 认证凭据（用户名/密码）保存在服务器端，不暴露给前端
//! 3. 可以统一做错误处理、日志记录等

use std::time::Duration;

use serde_json::Value;

use crate::{
    app_config::SunshineHostConfig,
    error::{AppError, AppResult},
    http_client, network,
};

// ─── 内部工具 ─────────────────────────────────────────────────────────────────

pub fn web_url(host: &SunshineHostConfig) -> String {
    format!("https://{}", network::authority(&host.host, host.web_port))
}

fn api_url(host: &SunshineHostConfig, path: &str) -> String {
    format!("{}{path}", web_url(host))
}

/// 统一处理 Sunshine API 的响应：检查状态码，提取错误信息，解析 JSON。
///
/// # 错误提取逻辑
///
/// 当 HTTP 状态码不是 2xx 时，尝试从响应体提取人类可读的错误描述：
/// 1. 先尝试把响应体解析为 JSON
/// 2. 从 JSON 中找 `"status"` 或 `"error"` 字段（Sunshine 的常见错误格式）
/// 3. 如果找不到，就截取响应体前 200 个字符作为错误信息
///
/// # 空响应处理
///
/// Sunshine 有些 API（如关闭当前应用、重启服务）成功时返回空响应体（HTTP 204 No Content）。
/// 空响应不是 JSON，所以无法直接 `serde_json::from_str`，这里统一返回 `{"ok": true}`。
async fn handle_response(resp: reqwest::Response) -> AppResult<Value> {
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Upstream(format!("读取 Sunshine 响应失败: {e}")))?;

    if !status.is_success() {
        // 尝试从 JSON 响应中提取 "status" 或 "error" 字段作为错误描述
        let detail = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| {
                // `.or_else` 表示：先找 "status" 字段，找不到再找 "error" 字段
                v.get("status")
                    .or_else(|| v.get("error"))
                    .and_then(|f| f.as_str()) // 确保是字符串类型
                    .map(|s| s.to_string())
            })
            // 如果 JSON 解析失败或找不到错误字段，就用响应文本的前 200 个字符
            .unwrap_or_else(|| text.chars().take(200).collect());
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(AppError::Forbidden(format!(
                "Sunshine 认证失败，请检查主机用户名和密码（HTTP {status}: {detail}）"
            )));
        }
        return Err(AppError::Upstream(format!(
            "Sunshine API 返回 HTTP {status}: {detail}"
        )));
    }

    // 空响应体（HTTP 204 等）：Sunshine 某些操作成功后不返回任何内容
    if text.is_empty() {
        return Ok(serde_json::json!({ "ok": true }));
    }

    // 尝试解析 JSON，如果响应不是 JSON（如纯文本），则包装到 content 字段
    Ok(serde_json::from_str::<Value>(&text)
        .unwrap_or_else(|_| serde_json::json!({ "content": text })))
}

/// 向 Sunshine API 发送 GET 请求。
///
/// `basic_auth` 是 HTTP Basic 认证的标准方式：
/// 将用户名和密码以 `username:password` 格式用 Base64 编码，
/// 放在请求头 `Authorization: Basic <编码后的字符串>` 中。
/// Sunshine 的 Web UI 就是用这种方式保护 API 的。
async fn sunshine_get(host: &SunshineHostConfig, path: &str) -> AppResult<Value> {
    let resp = http_client::for_tls(host.verify_tls)?
        .get(api_url(host, path))
        .basic_auth(&host.username, Some(&host.password)) // HTTP Basic 认证：用户名 + 密码
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("连接 Sunshine {path} 失败: {e}")))?;
    handle_response(resp).await
}

/// 向 Sunshine API 发送带 JSON 请求体的 POST 请求。
///
/// `.json(body)` 会自动把 `Value` 序列化为 JSON 字符串，
/// 并设置 `Content-Type: application/json` 请求头。
async fn sunshine_post_json(
    host: &SunshineHostConfig,
    path: &str,
    body: &Value,
) -> AppResult<Value> {
    let resp = http_client::for_tls(host.verify_tls)?
        .post(api_url(host, path))
        .basic_auth(&host.username, Some(&host.password))
        .json(body)
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("请求 Sunshine {path} 失败: {e}")))?;
    handle_response(resp).await
}

/// 向 Sunshine API 发送无请求体的 POST 请求。
///
/// 为什么需要手动设置 `CONTENT_LENGTH: 0`？
///
/// 某些服务器（包括部分版本的 Sunshine）对 POST 请求有严格要求：
/// 必须明确声明 Content-Length 为 0，否则服务器可能认为请求不完整而挂起等待请求体。
/// reqwest 在没有请求体时不会自动添加这个头，所以需要手动加上。
async fn sunshine_post_empty(host: &SunshineHostConfig, path: &str) -> AppResult<Value> {
    let resp = http_client::for_tls(host.verify_tls)?
        .post(api_url(host, path))
        .basic_auth(&host.username, Some(&host.password))
        .header(reqwest::header::CONTENT_LENGTH, "0") // 明确告知服务器请求体为空，避免服务器等待
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("请求 Sunshine {path} 失败: {e}")))?;
    handle_response(resp).await
}

/// 向 Sunshine API 发送 DELETE 请求。
async fn sunshine_delete(host: &SunshineHostConfig, path: &str) -> AppResult<Value> {
    let resp = http_client::for_tls(host.verify_tls)?
        .delete(api_url(host, path))
        .basic_auth(&host.username, Some(&host.password))
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("请求 Sunshine {path} 失败: {e}")))?;
    handle_response(resp).await
}

// ─── TCP 可达性检测（不需要认证）────────────────────────────────────────────────

/// 检测 Sunshine 主机是否可通过 TCP 连接访问（不涉及认证）。
///
/// # 超时机制
///
/// `tokio::time::timeout` 包裹异步操作，如果在指定时间内没有完成就取消并返回超时错误。
/// 这里设置 500ms 超时，原因：
/// - 局域网内 TCP 连接通常 <50ms，500ms 已经足够宽裕
/// - 如果超时，说明主机已关机或网络不通，不需要等更长时间
///
/// `.is_ok_and(|r| r.is_ok())` 的双重 `is_ok` 含义：
/// - 外层 `is_ok()`：检查 `timeout` 是否没有超时（`Ok` 表示在时间内得到结果）
/// - 内层 `|r| r.is_ok()`：检查 TCP 连接本身是否成功
pub async fn check_reachable(host: &SunshineHostConfig) -> bool {
    use tokio::{net::TcpStream, time::timeout};
    let address = network::normalize_host(&host.host);
    timeout(
        Duration::from_millis(500), // 500 毫秒超时：主机离线时不等太久
        TcpStream::connect((address.as_str(), host.web_port)),
    )
    .await
    .is_ok_and(|r| r.is_ok()) // 超时返回 false，连接失败也返回 false，只有连接成功才返回 true
}

/// 验证 Sunshine Web API 及管理凭据，而不只是检查端口是否打开。
pub async fn check_connection(host: &SunshineHostConfig) -> Result<(), String> {
    apps_list(host)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

// ─── 应用管理 ──────────────────────────────────────────────────────────────────

/// 获取 Sunshine 管理的游戏/应用列表。
pub async fn apps_list(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_get(host, "/api/apps").await
}

/// 保存（新增或修改）一个游戏/应用配置。
pub async fn apps_save(host: &SunshineHostConfig, app: Value) -> AppResult<Value> {
    sunshine_post_json(host, "/api/apps", &app).await
}

/// 关闭当前正在运行的游戏/应用。
pub async fn apps_close(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_post_empty(host, "/api/apps/close").await
}

/// 删除指定索引的游戏/应用。
pub async fn apps_delete(host: &SunshineHostConfig, index: u32) -> AppResult<Value> {
    sunshine_delete(host, &format!("/api/apps/{index}")).await
}

// ─── 客户端管理 ────────────────────────────────────────────────────────────────

/// 列出已配对的 Moonlight 客户端。
pub async fn clients_list(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_get(host, "/api/clients/list").await
}

/// 取消与指定 UUID 客户端的配对。
pub async fn clients_unpair(host: &SunshineHostConfig, uuid: &str) -> AppResult<Value> {
    sunshine_post_json(host, "/api/unpair", &serde_json::json!({ "uuid": uuid })).await
}

/// 取消所有已配对客户端。
pub async fn clients_unpair_all(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_post_empty(host, "/api/clients/unpair-all").await
}

/// 更新指定客户端的启用/禁用状态。
pub async fn clients_update(
    host: &SunshineHostConfig,
    uuid: &str,
    enabled: bool,
) -> AppResult<Value> {
    sunshine_post_json(
        host,
        "/api/clients/update",
        &serde_json::json!({ "uuid": uuid, "enabled": enabled }),
    )
    .await
}

// ─── 配置管理 ──────────────────────────────────────────────────────────────────

/// 获取 Sunshine 当前配置。
pub async fn config_get(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_get(host, "/api/config").await
}

/// 保存 Sunshine 配置。
pub async fn config_save(host: &SunshineHostConfig, config: Value) -> AppResult<Value> {
    sunshine_post_json(host, "/api/config", &config).await
}

/// 获取 Sunshine 的本地化配置（不需要认证，所以单独实现）。
pub async fn config_locale(host: &SunshineHostConfig) -> AppResult<Value> {
    // 注意：这个接口不需要 basic_auth，所以没有使用通用的 sunshine_get
    let resp = http_client::for_tls(host.verify_tls)?
        .get(api_url(host, "/api/configLocale"))
        .send()
        .await
        .map_err(|e| AppError::Upstream(format!("连接 Sunshine /api/configLocale 失败: {e}")))?;
    handle_response(resp).await
}

// ─── 日志 ──────────────────────────────────────────────────────────────────────

/// 获取 Sunshine 的运行日志。
pub async fn api_logs(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_get(host, "/api/logs").await
}

// ─── 配对 ──────────────────────────────────────────────────────────────────────

/// 使用 PIN 码与 Moonlight 客户端完成配对。
///
/// Moonlight 配对流程：客户端显示一个 PIN 码，用户在 Sunshine 管理界面输入此 PIN 完成配对。
pub async fn pin_pair(host: &SunshineHostConfig, pin: &str, name: &str) -> AppResult<Value> {
    sunshine_post_json(
        host,
        "/api/pin",
        &serde_json::json!({ "pin": pin, "name": name }),
    )
    .await
}

// ─── 密码管理 ──────────────────────────────────────────────────────────────────

/// 修改 Sunshine Web 界面的登录密码。
pub async fn password_update(host: &SunshineHostConfig, payload: Value) -> AppResult<Value> {
    sunshine_post_json(host, "/api/password", &payload).await
}

// ─── 系统操作 ──────────────────────────────────────────────────────────────────

/// 重启 Sunshine 服务进程。
pub async fn restart(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_post_empty(host, "/api/restart").await
}

/// 重置显示设备持久化配置（用于解决虚拟显示器配置异常问题）。
pub async fn reset_display_device(host: &SunshineHostConfig) -> AppResult<Value> {
    sunshine_post_empty(host, "/api/reset-display-device-persistence").await
}

// ─── 封面图片 ──────────────────────────────────────────────────────────────────

/// 下载指定应用的封面图片，返回 (Content-Type, 图片字节数据) 元组。
///
/// 这里不使用通用的 `handle_response`，因为需要返回二进制数据（图片字节），
/// 而不是 JSON。所以单独处理响应，读取 Content-Type 头和原始字节流。
pub async fn cover_get(host: &SunshineHostConfig, index: u32) -> AppResult<(String, Vec<u8>)> {
    let resp = http_client::for_tls(host.verify_tls)?
        .get(api_url(host, &format!("/api/covers/{index}")))
        .basic_auth(&host.username, Some(&host.password))
        .send()
        .await
        .map_err(|e| AppError::Process(format!("Sunshine cover GET failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::Process(format!(
            "Sunshine cover endpoint returned HTTP {}",
            resp.status()
        )));
    }

    // 从响应头中提取 Content-Type（如 "image/jpeg" 或 "image/png"）
    // 如果响应头不存在或不是有效字符串，默认使用 "image/jpeg"
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    // `.bytes()` 读取响应体的原始字节，`.to_vec()` 将其转换为 `Vec<u8>`
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Process(format!("failed to read cover bytes: {e}")))?
        .to_vec();

    Ok((content_type, bytes))
}

/// 上传游戏封面图片（通过 URL 方式，让 Sunshine 自己去下载图片）。
pub async fn cover_upload(host: &SunshineHostConfig, key: &str, url: &str) -> AppResult<Value> {
    sunshine_post_json(
        host,
        "/api/covers/upload",
        &serde_json::json!({ "key": key, "url": url }),
    )
    .await
}
