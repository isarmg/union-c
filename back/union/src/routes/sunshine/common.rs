//! Sunshine 主机校验、查找和持久化。

use super::*;

// ─── 请求校验 ─────────────────────────────────────────────────────────────────

/// 验证 host 字段是有效的 IPv4、IPv6 或域名。
pub(super) fn validate_host(host: &str) -> AppResult<()> {
    let h = host.trim();
    if network::is_valid_host(h) {
        return Ok(());
    }
    Err(AppError::InvalidHost(format!(
        "无效的 host 值 '{h}'，请提供有效的 IPv4、IPv6 或域名"
    )))
}

/// 验证 MAC 地址格式（可选字段，空字符串直接放行）。
pub(super) fn validate_mac(mac: &Option<String>) -> AppResult<()> {
    let Some(m) = mac else {
        return Ok(());
    };
    let m = m.trim();
    if m.is_empty() {
        return Ok(());
    }
    // 允许 AA:BB:CC:DD:EE:FF 或 AA-BB-CC-DD-EE-FF
    let sep = if m.contains(':') { ':' } else { '-' };
    let parts: Vec<&str> = m.split(sep).collect();
    let valid = parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()));
    if !valid {
        return Err(AppError::BadRequest(format!(
            "无效的 MAC 地址 '{m}'，格式应为 AA:BB:CC:DD:EE:FF"
        )));
    }
    Ok(())
}

pub(super) fn validate_host_request(
    req: &SunshineHostSaveRequest,
    production: bool,
) -> AppResult<()> {
    validate_host(&req.host)?;
    validate_mac(&req.mac_address)?;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("主机名称不能为空".to_string()));
    }
    if req.username.trim().is_empty() {
        return Err(AppError::BadRequest("管理用户名不能为空".to_string()));
    }
    if req.web_port == 0 {
        return Err(AppError::BadRequest("API 端口必须大于 0".to_string()));
    }
    if let Some(address) = req.broadcast_addr.as_deref()
        && address.parse::<std::net::SocketAddr>().is_err()
    {
        return Err(AppError::BadRequest("WOL 广播地址格式无效".to_string()));
    }
    if production && !req.verify_tls {
        return Err(AppError::BadRequest(
            "生产环境不允许关闭 Sunshine TLS 证书验证".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_proxy_json_object(
    label: &str,
    value: &Value,
    max_bytes: usize,
) -> AppResult<()> {
    if !value.is_object() {
        return Err(AppError::BadRequest(format!(
            "{label} must be a JSON object"
        )));
    }
    let size = serde_json::to_vec(value)
        .map_err(|err| AppError::BadRequest(format!("invalid {label}: {err}")))?
        .len();
    if size > max_bytes {
        return Err(AppError::BadRequest(format!(
            "{label} exceeds the maximum size of {} KiB",
            max_bytes / 1024
        )));
    }
    Ok(())
}

pub(super) fn validate_client_id(value: &str) -> AppResult<&str> {
    validate_opaque_string("client uuid", value, 128)
}

pub(super) fn validate_pin_request(pin: &str, name: &str) -> AppResult<(String, String)> {
    let pin = pin.trim();
    if !(4..=8).contains(&pin.len()) || !pin.chars().all(|value| value.is_ascii_digit()) {
        return Err(AppError::BadRequest(
            "Sunshine PIN must be 4 to 8 digits".to_string(),
        ));
    }
    let name = validate_opaque_string("client name", name, 80)?.to_string();
    Ok((pin.to_string(), name))
}

pub(super) fn validate_cover_upload(key: &str, value: &str) -> AppResult<(String, String)> {
    let key = validate_opaque_string("cover key", key, 512)?.to_string();
    let value = value.trim();
    if value.len() > 2048 {
        return Err(AppError::BadRequest(
            "cover URL cannot be longer than 2048 characters".to_string(),
        ));
    }
    let parsed = url::Url::parse(value)
        .map_err(|_| AppError::BadRequest("cover URL must be an absolute URL".to_string()))?;
    match parsed.scheme() {
        "http" | "https" if parsed.host_str().is_some() => Ok((key, parsed.to_string())),
        _ => Err(AppError::BadRequest(
            "cover URL must use http or https".to_string(),
        )),
    }
}

fn validate_opaque_string<'a>(label: &str, value: &'a str, max_chars: usize) -> AppResult<&'a str> {
    if value.chars().any(char::is_control) {
        return Err(AppError::BadRequest(format!(
            "{label} cannot contain control characters"
        )));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::BadRequest(format!("{label} cannot be empty")));
    }
    if value.chars().count() > max_chars {
        return Err(AppError::BadRequest(format!(
            "{label} cannot be longer than {max_chars} characters"
        )));
    }
    Ok(value)
}

// ─── 辅助：按 ID 查找主机 ─────────────────────────────────────────────────────

/// 按主机 ID 查找 Sunshine 主机配置，找不到则返回 400 错误。
///
/// `state.hosts.sunshine` 是 `RwLock<Vec<SunshineHostConfig>>`：
/// - `RwLock` 允许多个读者同时访问，但写者独占
/// - `.read().await` 获取读锁（等待写锁释放后才能获取）
/// - 此处只需要读取，所以用读锁（性能更好）
pub(super) async fn find_host(state: &AppState, id: &str) -> AppResult<SunshineHostConfig> {
    let hosts = state.hosts.sunshine.read().await;
    let host = hosts
        .iter()
        .find(|h| h.id == id) // 线性搜索（主机数量通常很少，无需索引）
        .cloned() // `.cloned()` 从引用 `&SunshineHostConfig` 复制出一个新的拥有值
        .ok_or_else(|| AppError::BadRequest(format!("Sunshine 主机 '{id}' 不存在")))?;
    if state.settings.production && !host.verify_tls {
        return Err(AppError::BadRequest(
            "该 Sunshine 主机已禁用 TLS 验证；请先编辑配置并启用验证".to_string(),
        ));
    }
    Ok(host)
}

async fn settings_with_hosts(
    state: &AppState,
    hosts: &[SunshineHostConfig],
) -> crate::app_config::Settings {
    // 两类动态主机都从内存快照写回，避免用启动时的旧配置覆盖另一个模块。
    let mut settings = (*state.settings).clone();
    settings.sunshine.hosts = hosts.to_vec();
    settings.proxmox.hosts = state.hosts.proxmox.read().await.clone();
    settings
}

pub(super) async fn persist_registered_host(
    state: &AppState,
    hosts: &[SunshineHostConfig],
    _host: &SunshineHostConfig,
) -> AppResult<()> {
    let settings = settings_with_hosts(state, hosts).await;
    database::save_app_settings(state.db().as_ref(), &settings).await?;
    Ok(())
}

pub(super) async fn persist_unregistered_host(
    state: &AppState,
    hosts: &[SunshineHostConfig],
    _id: &str,
) -> AppResult<()> {
    let settings = settings_with_hosts(state, hosts).await;
    database::save_app_settings(state.db().as_ref(), &settings).await?;
    Ok(())
}

/// 将主机配置转换为脱敏的展示信息（不包含密码明文）。
///
/// 密码字段转换为布尔值 `password_set`，前端只需要知道"是否已配置密码"，
/// 不需要（也不应该）知道密码内容。这是 API 设计的安全最佳实践。
pub(super) fn host_info(
    host: &SunshineHostConfig,
    reachable: bool,
    connection: Option<&Result<(), String>>,
) -> SunshineHostInfo {
    SunshineHostInfo {
        id: host.id.clone(),
        name: host.name.clone(),
        host: host.host.clone(),
        web_port: host.web_port,
        mac_configured: host.mac_address.is_some(), // MAC 地址是否已配置（用于显示 WOL 按钮）
        broadcast_addr: host.broadcast_addr.clone(),
        username: host.username.clone(),
        password_set: !host.password.is_empty(), // 密码是否已设置（不返回密码本身）
        verify_tls: host.verify_tls,
        web_url: sunshine::web_url(host),
        reachable,
        connected: connection.is_some_and(Result::is_ok),
        connection_error: connection.and_then(|result| result.as_ref().err().cloned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_host_and_mac_inputs() {
        assert!(validate_host("host.example.lan").is_ok());
        assert!(validate_host("[::1]").is_ok());
        assert!(validate_host("bad host").is_err());
        assert!(validate_mac(&Some("AA:BB:CC:DD:EE:FF".to_string())).is_ok());
        assert!(validate_mac(&Some("not-a-mac".to_string())).is_err());
    }

    #[test]
    fn validates_sunshine_proxy_inputs() {
        assert!(validate_client_id("client-1").is_ok());
        assert!(validate_client_id("\nclient").is_err());
        assert!(validate_pin_request("1234", "Moonlight").is_ok());
        assert!(validate_pin_request("12ab", "Moonlight").is_err());
        assert!(validate_cover_upload("box-art", "https://example.test/cover.jpg").is_ok());
        assert!(validate_cover_upload("box-art", "file:///etc/passwd").is_err());
        assert!(
            validate_proxy_json_object(
                "Sunshine config",
                &serde_json::json!({"locale": "en"}),
                1024
            )
            .is_ok()
        );
        assert!(
            validate_proxy_json_object("Sunshine config", &serde_json::json!([]), 1024).is_err()
        );
    }
}
