//! Sunshine 主机 CRUD、状态、WOL 和日志端点。

use super::{common::*, *};

// ─── 主机 CRUD ────────────────────────────────────────────────────────────────

/// 列出所有已配置的 Sunshine 主机，每个主机附带实时 TCP 可达性检测结果。
///
/// 主机探测以最多 8 个并发任务执行，避免离线主机让总延迟线性叠加。
pub(super) async fn list_hosts(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<SunshineHostInfo>>> {
    let hosts = state.hosts.sunshine.read().await.clone(); // 克隆列表后释放读锁
    let production = state.settings.production;
    // 限制并发数，避免离线主机让延迟线性叠加，也避免一次创建过多连接。
    let infos = stream::iter(hosts.into_iter().map(move |host| async move {
        let reachable = sunshine::check_reachable(&host).await;
        let connection = if production && !host.verify_tls {
            Err("生产环境不允许关闭 Sunshine TLS 证书验证".to_string())
        } else if reachable {
            sunshine::check_connection(&host).await
        } else {
            Err("Sunshine Web 端口不可达".to_string())
        };
        host_info(&host, reachable, Some(&connection))
    }))
    .buffered(8)
    .collect()
    .await;
    Ok(Json(infos))
}

/// 新建 Sunshine 主机配置。
///
/// `broadcast_addr` 使用默认值 `"255.255.255.255:9"`，这是 WOL 的标准广播地址，
/// 表示向局域网内所有设备发送，端口 9 是 WOL 惯用端口。
pub(super) async fn create_host(
    State(state): State<AppState>,
    Json(req): Json<SunshineHostSaveRequest>,
) -> AppResult<Json<SunshineHostInfo>> {
    validate_host_request(&req, state.settings.production)?;
    let new_host = SunshineHostConfig {
        id: uuid::Uuid::new_v4().to_string(), // 生成唯一 ID，用于路由中的 {id} 参数
        name: req.name.trim().to_string(),
        host: network::normalize_host(&req.host),
        web_port: req.web_port,
        mac_address: normalize_mac(req.mac_address),
        // 如果请求没有提供广播地址，使用标准的全子网广播地址
        broadcast_addr: normalize_broadcast_addr(req.broadcast_addr)
            .unwrap_or_else(|| "255.255.255.255:9".to_string()),
        log_path: std::path::PathBuf::from("union/data/sunshine/logs/sunshine.log"),
        username: req.username.trim().to_string(),
        password: req.password.unwrap_or_default(), // 密码可选，未提供时为空字符串
        verify_tls: req.verify_tls,
    };
    let reachable = sunshine::check_reachable(&new_host).await;
    let connection = if reachable {
        sunshine::check_connection(&new_host).await
    } else {
        Err("Sunshine Web 端口不可达".to_string())
    };
    let info = host_info(&new_host, reachable, Some(&connection));
    let _settings_guard = state.hosts.settings_lock.lock().await;
    let mut hosts = state.hosts.sunshine.read().await.clone();
    hosts.push(new_host.clone());
    // 数据库成功后才发布内存快照；失败时 API 与运行状态保持一致。
    persist_registered_host(&state, &hosts, &new_host).await?;
    database::insert_audit(
        state.db().as_ref(),
        "sunshine.host.create",
        &new_host.id,
        Some(&format!(
            "name={} host={} port={} verify_tls={}",
            new_host.name, new_host.host, new_host.web_port, new_host.verify_tls
        )),
    )
    .await?;
    *state.hosts.sunshine.write().await = hosts;
    Ok(Json(info))
}

/// 更新主机配置（按 ID）。
///
/// 密码和广播地址都是可选更新：如果请求中没有提供，则保留原来的值。
/// 这样前端不需要每次都传完整配置，可以只更新部分字段。
pub(super) async fn update_host(
    State(state): State<AppState>,
    Path(id): Path<String>, // 从 URL 路径 `/hosts/{id}` 中提取 id 参数
    Json(req): Json<SunshineHostSaveRequest>,
) -> AppResult<Json<SunshineHostInfo>> {
    validate_host_request(&req, state.settings.production)?;
    let _settings_guard = state.hosts.settings_lock.lock().await;
    let mut hosts = state.hosts.sunshine.read().await.clone();
    let host = hosts
        .iter_mut() // `iter_mut` 返回可变引用，允许修改列表中的元素
        .find(|h| h.id == id)
        .ok_or_else(|| AppError::BadRequest(format!("Sunshine 主机 '{id}' 不存在")))?;

    host.name = req.name.trim().to_string();
    host.host = network::normalize_host(&req.host);
    host.web_port = req.web_port;
    // 列表接口不会回传 MAC 明文；未提供时必须保留原值，避免只改名称或地址时
    // 意外清空 Wake-on-LAN 配置。
    if let Some(mac) = req.mac_address {
        host.mac_address = normalize_mac(Some(mac));
    }
    if let Some(b) = req.broadcast_addr
        && let Some(broadcast_addr) = normalize_broadcast_addr(Some(b))
    {
        host.broadcast_addr = broadcast_addr; // 只有明确提供了广播地址才更新
    }
    host.username = req.username.trim().to_string();
    if let Some(pw) = req.password {
        host.password = pw; // 只有明确提供了密码才更新，否则保留原密码
    }
    host.verify_tls = req.verify_tls;
    // 注意：这里先用 `reachable: false` 构建 info，因为还没检测可达性
    let host_clone = host.clone(); // 克隆一份，用于后续的可达性检测（借用检查要求）

    persist_registered_host(&state, &hosts, &host_clone).await?;
    database::insert_audit(
        state.db().as_ref(),
        "sunshine.host.update",
        &host_clone.id,
        Some(&format!(
            "name={} host={} port={} verify_tls={}",
            host_clone.name, host_clone.host, host_clone.web_port, host_clone.verify_tls
        )),
    )
    .await?;
    *state.hosts.sunshine.write().await = hosts;

    // 写锁释放后再执行网络检测（避免持锁时间过长）
    let reachable = sunshine::check_reachable(&host_clone).await;
    let connection = if reachable {
        sunshine::check_connection(&host_clone).await
    } else {
        Err("Sunshine Web 端口不可达".to_string())
    };
    Ok(Json(host_info(&host_clone, reachable, Some(&connection))))
}

/// 删除主机配置（按 ID）。
///
/// `retain` 保留所有不匹配 id 的主机，相当于"过滤掉"指定 id 的主机。
/// 通过比较删除前后的长度来判断是否真的找到并删除了目标主机。
pub(super) async fn delete_host(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<axum::http::StatusCode> {
    let _settings_guard = state.hosts.settings_lock.lock().await;
    let mut hosts = state.hosts.sunshine.read().await.clone();
    let before = hosts.len();
    hosts.retain(|h| h.id != id); // 保留所有 id 不等于目标的主机
    if hosts.len() == before {
        // 如果长度没变，说明没有找到对应 id 的主机
        return Err(AppError::BadRequest(format!("Sunshine 主机 '{id}' 不存在")));
    }
    persist_unregistered_host(&state, &hosts, &id).await?;
    database::insert_audit(
        state.db().as_ref(),
        "sunshine.host.delete",
        &id,
        Some("host removed"),
    )
    .await?;
    *state.hosts.sunshine.write().await = hosts;
    Ok(axum::http::StatusCode::NO_CONTENT) // 删除成功返回 204 No Content
}

// ─── 单主机状态和 WOL ─────────────────────────────────────────────────────────

/// 获取指定 Sunshine 主机的运行状态（进程状态、TCP 可达性等）。
pub(super) async fn host_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<SunshineStatus>> {
    let host = find_host(&state, &id).await?;
    Ok(Json(service_manager::sunshine_host_status(&host).await))
}

/// 向指定主机发送 Wake-on-LAN 魔术包（远程唤醒）。
pub(super) async fn host_wake(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<WakeResponse>> {
    let host = find_host(&state, &id).await?;
    let response = wol::wake_host(&host, state.db().as_ref()).await?;
    database::insert_audit(
        state.db().as_ref(),
        "sunshine.host.wake",
        &id,
        Some(&format!("target={}", response.target)),
    )
    .await?;
    Ok(Json(response))
}

/// 读取指定主机的 Sunshine 本地日志文件（从日志路径直接读取，不经过 Sunshine API）。
pub(super) async fn host_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<LogQuery>,
) -> AppResult<Json<LogsResponse>> {
    let host = find_host(&state, &id).await?;
    let lines = query.lines.unwrap_or(200).min(1000);
    Ok(Json(LogsResponse {
        path: host.log_path.to_string_lossy().to_string(),
        lines: crate::service_manager::tail_lines(&host.log_path, lines)?,
    }))
}
