//! HTTP 访问日志格式化模块。
//!
//! ram 支持类似 Nginx 的日志格式字符串，例如：
//! `$time_iso8601 $log_level - $remote_addr "$request" $status`
//! 本文件负责把格式字符串解析成片段，并在每个请求结束后填入真实值。

use std::{
    collections::HashMap,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{Local, SecondsFormat};

use crate::{auth::get_auth_user, server::Request, utils::decode_uri};

pub const DEFAULT_LOG_FORMAT: &str =
    r#"$time_iso8601 $log_level - $remote_addr "$request" $status"#;

/// 已解析好的 HTTP 日志格式。
#[derive(Debug, Clone, PartialEq)]
pub struct HttpLogger {
    elements: Vec<LogElement>,
}

impl Default for HttpLogger {
    fn default() -> Self {
        DEFAULT_LOG_FORMAT.parse().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum LogElement {
    /// 变量，例如 `$status`、`$request`。
    Variable(String),
    /// 请求头，例如 `$http_user_agent` 会读取 user-agent。
    Header(String),
    /// 普通文本，例如空格、引号、横线。
    Literal(String),
}

impl HttpLogger {
    /// 从请求中提前收集日志变量需要的数据。
    pub fn data(&self, req: &Request) -> HashMap<String, String> {
        // 先从请求中收集格式字符串可能用到的字段，避免 log 阶段再访问 Request。
        let mut data = HashMap::default();
        for element in self.elements.iter() {
            match element {
                LogElement::Variable(name) => match name.as_str() {
                    "request" | "request_method" | "request_uri" => {
                        let uri = req.uri().to_string();
                        let decoded_uri = decode_uri(&uri)
                            .map(|s| sanitize_log_value(&s))
                            .unwrap_or_else(|| uri.clone());
                        data.entry("request".to_string())
                            .or_insert_with(|| format!("{} {decoded_uri}", req.method()));
                        data.entry("request_method".to_string())
                            .or_insert_with(|| req.method().to_string());
                        data.entry("request_uri".to_string())
                            .or_insert_with(|| decoded_uri);
                    }
                    "remote_user" => {
                        if let Some(user) =
                            req.headers().get("authorization").and_then(get_auth_user)
                        {
                            data.insert(name.to_string(), user);
                        }
                    }
                    _ => {}
                },
                LogElement::Header(name) => {
                    if let Some(value) = req.headers().get(name).and_then(|v| v.to_str().ok()) {
                        data.insert(name.to_string(), sanitize_log_value(value));
                    }
                }
                LogElement::Literal(_) => {}
            }
        }
        data
    }

    /// 按配置格式输出一条访问日志。
    pub fn log(&self, data: &HashMap<String, String>, err: Option<String>) {
        if self.elements.is_empty() {
            return;
        }
        // log 阶段补充时间、毫秒时间戳和日志级别等运行时字段。
        let is_error = err.is_some();
        let now = Local::now();
        let time_local = now.to_rfc3339_opts(SecondsFormat::Secs, false);
        let time_iso8601 = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        let msec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| format!("{:.3}", d.as_secs_f64()))
            .unwrap_or_default();
        let log_level = if is_error { "ERROR" } else { "INFO" };

        let mut output = String::new();
        for element in self.elements.iter() {
            // 按解析出来的片段顺序拼接最终日志行。
            match element {
                LogElement::Literal(value) => output.push_str(value.as_str()),
                LogElement::Variable(name) => {
                    let resolved = match name.as_str() {
                        "time_local" => Some(time_local.as_str()),
                        "time_iso8601" => Some(time_iso8601.as_str()),
                        "msec" => Some(msec.as_str()),
                        "log_level" => Some(log_level),
                        _ => None,
                    };
                    let val = resolved
                        .or_else(|| data.get(name.as_str()).map(|v| v.as_str()))
                        .unwrap_or("-");
                    output.push_str(val);
                }
                LogElement::Header(name) => {
                    output.push_str(data.get(name.as_str()).map(|v| v.as_str()).unwrap_or("-"))
                }
            }
        }
        match err {
            Some(err) => emit_http_access(&format!("{output} {err}"), true),
            None => emit_http_access(&output, false),
        }
    }
}

/// 通过 `log` crate 输出访问日志。
///
/// target 使用 `http_access`，这样系统 logger 会尽量原样打印访问日志行，
/// 按消息原样写入，不添加时间戳或级别前缀。
/// 根据是否是错误日志选择 info/error 级别输出。
fn emit_http_access(msg: &str, is_error: bool) {
    let level = if is_error {
        log::Level::Error
    } else {
        log::Level::Info
    };
    log::logger().log(
        &log::Record::builder()
            .args(format_args!("{}", msg))
            .level(level)
            .target("http_access")
            .build(),
    );
}

impl FromStr for HttpLogger {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // 把格式字符串拆成 Literal / Variable / Header。
        // 例如 `$http_user_agent` 会被识别成 Header("user-agent")。
        let mut elements = vec![];
        let mut is_var = false;
        let mut cache = String::new();
        for c in format!("{s} ").chars() {
            if c == '$' {
                if !cache.is_empty() {
                    elements.push(LogElement::Literal(cache.to_string()));
                }
                cache.clear();
                is_var = true;
            } else if is_var && !(c.is_alphanumeric() || c == '_') {
                if let Some(value) = cache.strip_prefix("$http_") {
                    elements.push(LogElement::Header(value.replace('_', "-").to_string()));
                } else if let Some(value) = cache.strip_prefix('$') {
                    elements.push(LogElement::Variable(value.to_string()));
                }
                cache.clear();
                is_var = false;
            }
            cache.push(c);
        }
        let cache = cache.trim();
        if !cache.is_empty() {
            elements.push(LogElement::Literal(cache.to_string()));
        }
        Ok(Self { elements })
    }
}

/// 清理日志值中的换行和控制字符，避免日志注入。
fn sanitize_log_value(s: &str) -> String {
    // 日志值可能来自用户输入，必须转义换行、控制字符和引号，避免伪造日志行。
    s.chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            c if c.is_control() => format!("\\x{:02x}", c as u32).chars().collect::<Vec<_>>(),
            c => vec![c],
        })
        .collect()
}
