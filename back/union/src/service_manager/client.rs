//! 访问本机 RAM 内部 HTTP 接口。

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

use crate::{
    domain::{RamEntryResponse, RamHealthResponse},
    error::{AppError, AppResult},
    network, ram_auth,
    state::AppState,
};

/// 探测 ram 健康状态（原始 TCP 方式，不依赖 reqwest HTTP 客户端）。
///
/// 为什么不用 reqwest（常用的 Rust HTTP 客户端库）？
/// 1. 依赖最小化：引入 reqwest 会带来大量间接依赖（TLS、连接池、异步运行时集成等），
///    而健康检查只需要发一个 GET 请求并读响应，自己写 20 行代码就够了；
/// 2. 避免循环依赖：如果union本身用 reqwest 作为服务器同时又用它做健康检查客户端，
///    出错时排查会更复杂；
/// 3. 全部细节可见：原始 TCP 方式让每一步（连接、发送、读取、解析）都显式可见，
///    便于调试超时、字符集等边缘问题。
///
/// 实际发出的请求形如：
/// ```text
/// GET /__ram__/health HTTP/1.1\r\n
/// Host: 127.0.0.1:5000\r\n
/// User-Agent: union\r\n
/// Connection: close\r\n
/// \r\n
/// ```
/// ram 返回 `{"status":"OK"}` 时认为健康。
///
/// 注意：此函数不返回 `AppResult`（不会因错误而中断流程），
/// 因为"不健康"本身就是需要展示给用户的状态。
pub async fn ram_health(state: &AppState) -> RamHealthResponse {
    // 健康检查调用 ram 内置的 /__ram__/health。
    // 这个函数不返回 AppResult，因为健康检查失败本身也是一种正常状态，需要返回给前端展示。
    let path = ram_internal_path(state, "__ram__/health", None);
    let url = ram_url_for_path(state, &path);

    match raw_ram_get(state, &path, false).await {
        Ok((status_code, body)) => {
            let body_json = serde_json::from_str::<Value>(&body).ok();
            let reachable = status_code == 200
                && body_json
                    .as_ref()
                    .and_then(|value| value.get("status"))
                    .and_then(Value::as_str)
                    .is_some_and(|status| status.eq_ignore_ascii_case("OK"));
            RamHealthResponse {
                reachable,
                status_code: Some(status_code),
                url,
                body: body_json.or(Some(Value::String(body))),
                message: if reachable {
                    "ram health endpoint returned OK".to_string()
                } else {
                    format!("ram health endpoint returned HTTP {status_code}")
                },
            }
        }
        Err(err) => RamHealthResponse {
            reachable: false,
            status_code: None,
            url,
            body: None,
            message: err,
        },
    }
}

/// 读取 ram 内部路径的 JSON 入口。
pub async fn ram_entry(state: &AppState, path: Option<String>) -> AppResult<RamEntryResponse> {
    let path = path.unwrap_or_else(|| "/".to_string());
    // 目录探测走 ram 自身 JSON 接口，并带上管理账号以便查看受保护路径。
    let request_path = ram_internal_path(state, &path, Some("json"));
    let url = ram_url_for_path(state, &request_path);
    let (status_code, body) = raw_ram_get(state, &request_path, true)
        .await
        .map_err(AppError::Process)?;
    let body = serde_json::from_str::<Value>(&body).unwrap_or(Value::String(body));

    Ok(RamEntryResponse {
        url,
        path,
        status_code,
        body,
    })
}

fn ram_loopback_host(state: &AppState) -> &'static str {
    let bind = state.settings.ram.bind.trim().trim_matches(['[', ']']);
    if matches!(bind, "::" | "::1") {
        "::1"
    } else {
        "127.0.0.1"
    }
}

/// 把 host 和 port 格式化成合法的 TCP 地址字符串。
/// IPv6 地址需要用方括号包裹，例如 `[::1]:5000`；IPv4 直接 `127.0.0.1:5000`。
fn format_host_port(host: &str, port: u16) -> String {
    network::authority(host, port)
}

pub(super) fn ram_base_url(state: &AppState) -> String {
    if let Some(url) = &state.settings.ram.public_url {
        return format!(
            "{}{}",
            url.trim_end_matches('/'),
            normalized_path_prefix(&state.settings.ram.path_prefix)
        );
    }
    format!(
        "http://{}{}",
        format_host_port(ram_loopback_host(state), state.settings.ram.port),
        normalized_path_prefix(&state.settings.ram.path_prefix)
    )
}

pub(super) fn ram_health_url(state: &AppState) -> String {
    ram_url_for_path(state, &ram_internal_path(state, "__ram__/health", None))
}

fn ram_url_for_path(state: &AppState, path: &str) -> String {
    format!(
        "http://{}{}",
        format_host_port(ram_loopback_host(state), state.settings.ram.port),
        path
    )
}

fn normalized_path_prefix(path_prefix: &str) -> String {
    let trimmed = path_prefix.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("/{trimmed}")
    }
}

fn ram_internal_path(state: &AppState, path: &str, query: Option<&str>) -> String {
    let prefix = normalized_path_prefix(&state.settings.ram.path_prefix);
    let clean_path = path.trim_matches('/');
    // ram 对路径前缀敏感，这里统一拼接前缀并逐段编码，避免中文路径或空格破坏请求。
    let mut output = if prefix.is_empty() {
        if clean_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", encode_path(clean_path))
        }
    } else if clean_path.is_empty() {
        format!("{prefix}/")
    } else {
        format!("{prefix}/{}", encode_path(clean_path))
    };

    if let Some(query) = query {
        output.push('?');
        output.push_str(query);
    }

    output
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_segment(segment: &str) -> String {
    let mut output = String::new();
    for byte in segment.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(*byte as char)
            }
            byte => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

/// 用原始 TCP 连接向 ram 发送一个 HTTP/1.1 GET 请求，返回 (状态码, 响应体)。
///
/// HTTP/1.1 协议格式（"\r\n" 是 HTTP 规定的行分隔符，不能用 "\n"）：
/// ```
/// GET /path HTTP/1.1\r\n      ← 请求行
/// Host: 127.0.0.1:5000\r\n   ← 必须有 Host 头，HTTP/1.1 规范要求
/// User-Agent: ...\r\n         ← 可选，但有助于日志识别来源
/// Connection: close\r\n       ← 告诉服务器处理完请求后关闭连接（简化读取逻辑）
/// \r\n                        ← 空行，标志请求头结束
/// ```
///
/// 响应解析：以 "\r\n\r\n" 分割头部和正文；
/// 第一行的第二个空格分隔词就是状态码（如 "HTTP/1.1 200 OK" → 200）。
async fn raw_ram_get(
    state: &AppState,
    path: &str,
    include_management_auth: bool,
) -> Result<(u16, String), String> {
    // 这里用原始 TCP 发 HTTP 请求，避免为本地健康检查再引入完整 HTTP 客户端依赖。
    let host = ram_loopback_host(state);
    let port = state.settings.ram.port;
    // IPv6 地址在 TCP 连接字符串和 HTTP Host 头中都需要用方括号包裹。
    // 例如 IPv4: "127.0.0.1:5000"，IPv6: "[::1]:5000"
    let tcp_addr = format_host_port(host, port);

    // timeout(...) 是 Tokio 提供的异步超时包装器。
    // 如果内部 Future 在指定时间内未完成，返回 Err(Elapsed)。
    // 这里分三个阶段各自设超时：连接 2s、写请求 2s、读响应 5s（响应可能较大）。
    let mut stream = timeout(Duration::from_secs(2), TcpStream::connect(&tcp_addr))
        .await
        .map_err(|_| "ram connection timed out".to_string())?
        .map_err(|err| format!("failed to connect to ram: {err}"))?;

    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {tcp_addr}\r\nUser-Agent: union\r\nConnection: close\r\n"
    );
    if include_management_auth {
        // HTTP Basic Auth：将 "用户名:密码" 用 Base64 编码后放入 Authorization 头。
        // 格式：Authorization: Basic <base64("user:password")>
        // 注意：Base64 不是加密，只是编码；HTTPS 才能保证安全，这里是本地 127.0.0.1 所以可接受。
        if let Some((user, password)) = ram_auth::management_auth_pair(state)
            .await
            .map_err(|err| format!("failed to read ram management auth: {err}"))?
        {
            let encoded = STANDARD.encode(format!("{user}:{password}"));
            request.push_str(&format!("Authorization: Basic {encoded}\r\n"));
        }
    }
    request.push_str("\r\n"); // 空行，标志请求头结束

    timeout(Duration::from_secs(2), stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| "ram request write timed out".to_string())?
        .map_err(|err| format!("failed to write ram request: {err}"))?;

    // `read_to_end` 读取直到连接关闭（因为我们发了 Connection: close）。
    let mut bytes = Vec::new();
    timeout(Duration::from_secs(5), stream.read_to_end(&mut bytes))
        .await
        .map_err(|_| "ram response read timed out".to_string())?
        .map_err(|err| format!("failed to read ram response: {err}"))?;

    // HTTP 响应结构：头部 + "\r\n\r\n" + 正文
    // `split_once` 只在第一次出现时分割，头部本身也包含 "\r\n" 但不影响。
    let response = String::from_utf8_lossy(&bytes);
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "ram returned an invalid HTTP response".to_string())?;
    // 响应首行格式：HTTP/1.1 200 OK
    // split_whitespace().nth(1) 取第二个 token，即状态码字符串 "200"。
    let status_code = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| "ram returned an invalid status line".to_string())?;

    Ok((status_code, body.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_config::{LocalConfig, Settings},
        database,
        state::AppState,
    };

    #[test]
    fn formats_ipv6_and_encodes_path_segments() {
        assert_eq!(format_host_port("::1", 5000), "[::1]:5000");
        assert_eq!(format_host_port("127.0.0.1", 5000), "127.0.0.1:5000");
        assert_eq!(encode_path("目录/a b.md"), "%E7%9B%AE%E5%BD%95/a%20b.md");
    }

    #[tokio::test]
    async fn local_ram_urls_bracket_ipv6_loopback() {
        let mut settings = Settings::default();
        settings.ram.bind = "[::]".to_string();
        settings.ram.port = 5000;
        let state = AppState::new(
            settings,
            database::disconnected_pool().expect("disconnected pool"),
            "unused".to_string(),
            LocalConfig {
                database_url: String::new(),
                admin_username: "admin".to_string(),
                admin_password_hash: "unused".to_string(),
            },
        );

        assert_eq!(ram_base_url(&state), "http://[::1]:5000/files");
        assert_eq!(
            ram_health_url(&state),
            "http://[::1]:5000/files/__ram__/health"
        );
    }

    #[test]
    fn normalizes_optional_path_prefix() {
        assert_eq!(normalized_path_prefix("/files/"), "/files");
        assert_eq!(normalized_path_prefix("/"), "");
    }
}
