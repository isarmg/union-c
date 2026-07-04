//! PVE 主机查找、校验和通用操作。

use super::*;

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

/// 按 ID 查找 PVE 主机配置。
pub(super) async fn find_host(state: &AppState, id: &str) -> AppResult<ProxmoxHostConfig> {
    let host = state
        .hosts
        .proxmox
        .read()
        .await
        .iter()
        .find(|h| h.id == id)
        .cloned()
        .ok_or_else(|| AppError::BadRequest(format!("PVE 主机 '{id}' 不存在")))?;
    if state.settings.production && !host.verify_tls {
        return Err(AppError::BadRequest(
            "该 PVE 主机已禁用 TLS 验证；请先编辑配置并启用验证".to_string(),
        ));
    }
    Ok(host)
}

async fn settings_with_hosts(
    state: &AppState,
    hosts: &[ProxmoxHostConfig],
) -> crate::app_config::Settings {
    let mut settings = (*state.settings).clone();
    settings.proxmox.hosts = hosts.to_vec();
    settings.sunshine.hosts = state.hosts.sunshine.read().await.clone();
    settings
}

pub(super) async fn persist_registered_host(
    state: &AppState,
    hosts: &[ProxmoxHostConfig],
    host: &ProxmoxHostConfig,
) -> AppResult<()> {
    let settings = settings_with_hosts(state, hosts).await;
    database::save_app_settings_and_register_host(
        state.db().as_ref(),
        &settings,
        "proxmox",
        &host.id,
        &host.host,
    )
    .await?;
    Ok(())
}

pub(super) async fn persist_unregistered_host(
    state: &AppState,
    hosts: &[ProxmoxHostConfig],
    id: &str,
) -> AppResult<()> {
    let settings = settings_with_hosts(state, hosts).await;
    database::save_app_settings_and_unregister_host(state.db().as_ref(), &settings, "proxmox", id)
        .await?;
    Ok(())
}

/// 构建对外展示的主机信息（不含 token_secret 明文）。
pub(super) fn host_info(
    host: &ProxmoxHostConfig,
    connection: Option<&Result<(), String>>,
) -> PveHostInfo {
    PveHostInfo {
        id: host.id.clone(),
        name: host.name.clone(),
        host: host.host.clone(),
        port: host.port,
        token_id: host.token_id.clone(),
        token_secret_set: !host.token_secret.is_empty(),
        verify_tls: host.verify_tls,
        web_url: proxmox::web_url(host),
        connected: connection.is_some_and(Result::is_ok),
        connection_error: connection.and_then(|result| result.as_ref().err().cloned()),
    }
}

/// 验证 host 字段是否为合法的 IP 或域名。
pub(super) fn validate_host(host: &str) -> AppResult<()> {
    let h = host.trim();
    if network::is_valid_host(h) {
        return Ok(());
    }
    Err(AppError::InvalidHost(format!(
        "无效的 host '{h}'，请提供 IPv4、IPv6 或域名"
    )))
}

pub(super) fn validate_host_request(
    req: &PveHostSaveRequest,
    _creating: bool,
    production: bool,
) -> AppResult<()> {
    validate_host(&req.host)?;
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("主机名称不能为空".to_string()));
    }
    if req.port == 0 {
        return Err(AppError::BadRequest("PVE 端口必须大于 0".to_string()));
    }
    if req.token_id.trim().is_empty() {
        return Err(AppError::BadRequest("API Token ID 不能为空".to_string()));
    }
    if production && !req.verify_tls {
        return Err(AppError::BadRequest(
            "生产环境不允许关闭 PVE TLS 证书验证".to_string(),
        ));
    }
    Ok(())
}

// ─── 内部辅助 ─────────────────────────────────────────────────────────────────

/// 统一处理 VM/CT 电源操作（start/stop/shutdown/reboot/suspend/resume/reset）。
pub(super) async fn vm_power(
    state: &AppState,
    host_id: &str,
    node: &str,
    vmid: &str,
    kind: &str,   // "qemu" | "lxc"
    action: &str, // "start" | "stop" | "shutdown" | "reboot" | "suspend" | "resume" | "reset"
) -> AppResult<Json<Value>> {
    let host = find_host(state, host_id).await?;
    let data = proxmox::post(
        &host,
        &format!("nodes/{node}/{kind}/{vmid}/status/{action}"),
        &[],
    )
    .await?;
    Ok(Json(data))
}

/// 统一处理 VM/CT 创建快照。
pub(super) async fn snapshot_create(
    state: &AppState,
    host_id: &str,
    node: &str,
    vmid: &str,
    kind: &str,
    req: &PveSnapshotRequest,
) -> AppResult<Json<Value>> {
    let host = find_host(state, host_id).await?;
    let snapname = req.snapname.trim().to_string();
    if snapname.is_empty() {
        return Err(AppError::BadRequest("snapname 不能为空".to_string()));
    }
    let vmstate_str = if req.vmstate.unwrap_or(false) {
        "1"
    } else {
        "0"
    }
    .to_string();
    let desc = req.description.clone().unwrap_or_default();
    let mut params: Vec<(&str, &str)> = vec![("snapname", &snapname), ("vmstate", &vmstate_str)];
    if !desc.is_empty() {
        params.push(("description", &desc));
    }
    let data = proxmox::post(
        &host,
        &format!("nodes/{node}/{kind}/{vmid}/snapshot"),
        &params,
    )
    .await?;
    Ok(Json(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ip_domain_and_rejects_invalid_host() {
        assert!(validate_host("192.168.1.10").is_ok());
        assert!(validate_host("pve.example.lan").is_ok());
        assert!(validate_host("[2001:db8::1]").is_ok());
        assert!(validate_host("bad host").is_err());
    }
}
