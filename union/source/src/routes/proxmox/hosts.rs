//! PVE 主机、集群和节点端点。

use futures_util::{StreamExt, stream};

use super::{common::*, *};

// ─── 主机 CRUD ────────────────────────────────────────────────────────────────

pub(super) async fn list_hosts(State(state): State<AppState>) -> AppResult<Json<Vec<PveHostInfo>>> {
    let hosts = state.hosts.proxmox.read().await.clone();
    let production = state.settings.production;
    let infos = stream::iter(hosts.into_iter().map(move |host| async move {
        let connection = if production && !host.verify_tls {
            Err("生产环境不允许关闭 PVE TLS 证书验证".to_string())
        } else {
            proxmox::check_connection(&host).await
        };
        host_info(&host, Some(&connection))
    }))
    .buffered(8)
    .collect()
    .await;
    Ok(Json(infos))
}

pub(super) async fn create_host(
    State(state): State<AppState>,
    Json(req): Json<PveHostSaveRequest>,
) -> AppResult<Json<PveHostInfo>> {
    validate_host_request(&req, true, state.settings.production)?;
    let new_host = ProxmoxHostConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name.trim().to_string(),
        host: network::normalize_host(&req.host),
        port: req.port,
        token_id: req.token_id.trim().to_string(),
        token_secret: req.token_secret.unwrap_or_default().trim().to_string(),
        verify_tls: req.verify_tls,
    };
    let connection = proxmox::check_connection(&new_host).await;
    let info = host_info(&new_host, Some(&connection));
    let _settings_guard = state.hosts.settings_lock.lock().await;
    let mut hosts = state.hosts.proxmox.read().await.clone();
    hosts.push(new_host.clone());
    persist_registered_host(&state, &hosts, &new_host).await?;
    *state.hosts.proxmox.write().await = hosts;
    Ok(Json(info))
}

pub(super) async fn update_host(
    State(state): State<AppState>,
    Path(p): Path<HostPath>,
    Json(req): Json<PveHostSaveRequest>,
) -> AppResult<Json<PveHostInfo>> {
    validate_host_request(&req, false, state.settings.production)?;
    let _settings_guard = state.hosts.settings_lock.lock().await;
    let mut hosts = state.hosts.proxmox.read().await.clone();
    let host = hosts
        .iter_mut()
        .find(|h| h.id == p.id)
        .ok_or_else(|| AppError::BadRequest(format!("PVE 主机 '{}' 不存在", p.id)))?;
    host.name = req.name.trim().to_string();
    host.host = network::normalize_host(&req.host);
    host.port = req.port;
    host.token_id = req.token_id.trim().to_string();
    if let Some(secret) = req.token_secret {
        let s = secret.trim().to_string();
        if !s.is_empty() {
            host.token_secret = s;
        }
    }
    host.verify_tls = req.verify_tls;
    let probe_host = host.clone();
    persist_registered_host(&state, &hosts, &probe_host).await?;
    *state.hosts.proxmox.write().await = hosts;
    let connection = proxmox::check_connection(&probe_host).await;
    Ok(Json(host_info(&probe_host, Some(&connection))))
}

pub(super) async fn delete_host(
    State(state): State<AppState>,
    Path(p): Path<HostPath>,
) -> AppResult<axum::http::StatusCode> {
    let _settings_guard = state.hosts.settings_lock.lock().await;
    let mut hosts = state.hosts.proxmox.read().await.clone();
    let len_before = hosts.len();
    hosts.retain(|h| h.id != p.id);
    if hosts.len() == len_before {
        return Err(AppError::BadRequest(format!("PVE 主机 '{}' 不存在", p.id)));
    }
    persist_unregistered_host(&state, &hosts, &p.id).await?;
    *state.hosts.proxmox.write().await = hosts;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ─── 集群概览 ─────────────────────────────────────────────────────────────────

/// 返回集群所有资源（VM、CT、节点、存储），前端据此渲染总览。
pub(super) async fn cluster_resources(
    State(state): State<AppState>,
    Path(p): Path<HostPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let data = proxmox::get(&host, "cluster/resources").await?;
    Ok(Json(data))
}

pub(super) async fn cluster_nodes(
    State(state): State<AppState>,
    Path(p): Path<HostPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let data = proxmox::get(&host, "nodes").await?;
    Ok(Json(data))
}

pub(super) async fn cluster_tasks(
    State(state): State<AppState>,
    Path(p): Path<HostPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let data = proxmox::get(&host, "cluster/tasks").await?;
    Ok(Json(data))
}

// ─── 节点详情 ─────────────────────────────────────────────────────────────────

pub(super) async fn node_status(
    State(state): State<AppState>,
    Path(p): Path<NodePath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/status")).await?;
    Ok(Json(data))
}

pub(super) async fn node_storage(
    State(state): State<AppState>,
    Path(p): Path<NodePath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/storage")).await?;
    Ok(Json(data))
}

pub(super) async fn storage_content(
    State(state): State<AppState>,
    Path(p): Path<StoragePath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let storage = validate_storage(&p.storage)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/storage/{storage}/content")).await?;
    Ok(Json(data))
}

pub(super) async fn node_tasks(
    State(state): State<AppState>,
    Path(p): Path<NodePath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/tasks")).await?;
    Ok(Json(data))
}
