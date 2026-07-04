//! LXC 容器端点。

use super::{common::*, *};

// ─── Container (LXC) ─────────────────────────────────────────────────────────

pub(super) async fn list_containers(
    State(state): State<AppState>,
    Path(p): Path<NodePath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/lxc")).await?;
    Ok(Json(data))
}

pub(super) async fn ct_status(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/lxc/{vmid}/status/current")).await?;
    Ok(Json(data))
}

pub(super) async fn ct_config(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/lxc/{vmid}/config")).await?;
    Ok(Json(data))
}

pub(super) async fn ct_start(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "lxc", "start").await
}

pub(super) async fn ct_stop(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "lxc", "stop").await
}

pub(super) async fn ct_shutdown(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "lxc", "shutdown").await
}

pub(super) async fn ct_reboot(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "lxc", "reboot").await
}

pub(super) async fn ct_delete(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
    Query(q): Query<PveDeleteQuery>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let mut params: Vec<(&str, &str)> = Vec::new();
    let purge_str;
    if let Some(true) = q.purge {
        purge_str = "1".to_string();
        params.push(("purge", &purge_str));
    }
    let data = proxmox::delete(&host, &format!("nodes/{node}/lxc/{vmid}"), &params).await?;
    Ok(Json(data))
}

pub(super) async fn ct_snapshots(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/lxc/{vmid}/snapshot")).await?;
    Ok(Json(data))
}

pub(super) async fn ct_snapshot_create(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
    Json(req): Json<PveSnapshotRequest>,
) -> AppResult<Json<Value>> {
    snapshot_create(&state, &p.id, &p.node, &p.vmid, "lxc", &req).await
}

pub(super) async fn ct_snapshot_delete(
    State(state): State<AppState>,
    Path(p): Path<SnapPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let snap = validate_snapshot(&p.snap)?;
    let data = proxmox::delete(
        &host,
        &format!("nodes/{node}/lxc/{vmid}/snapshot/{snap}"),
        &[],
    )
    .await?;
    Ok(Json(data))
}

pub(super) async fn ct_snapshot_rollback(
    State(state): State<AppState>,
    Path(p): Path<SnapPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let snap = validate_snapshot(&p.snap)?;
    let data = proxmox::post(
        &host,
        &format!("nodes/{node}/lxc/{vmid}/snapshot/{snap}/rollback"),
        &[],
    )
    .await?;
    Ok(Json(data))
}
