//! QEMU 虚拟机端点。

use super::{common::*, *};

// ─── VM (QEMU) ───────────────────────────────────────────────────────────────

pub(super) async fn list_vms(
    State(state): State<AppState>,
    Path(p): Path<NodePath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/qemu")).await?;
    Ok(Json(data))
}

pub(super) async fn vm_status(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/qemu/{vmid}/status/current")).await?;
    Ok(Json(data))
}

pub(super) async fn vm_config(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/qemu/{vmid}/config")).await?;
    Ok(Json(data))
}

pub(super) async fn vm_start(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "qemu", "start").await
}

pub(super) async fn vm_stop(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "qemu", "stop").await
}

pub(super) async fn vm_shutdown(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "qemu", "shutdown").await
}

pub(super) async fn vm_reboot(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "qemu", "reboot").await
}

pub(super) async fn vm_suspend(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "qemu", "suspend").await
}

pub(super) async fn vm_resume(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "qemu", "resume").await
}

pub(super) async fn vm_reset(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    vm_power(&state, &p.id, &p.node, &p.vmid, "qemu", "reset").await
}

pub(super) async fn vm_delete(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
    Query(q): Query<PveDeleteQuery>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let mut params: Vec<(&str, &str)> = Vec::new();
    let purge_str;
    let destroy_str;
    if let Some(true) = q.purge {
        purge_str = "1".to_string();
        params.push(("purge", &purge_str));
    }
    if let Some(true) = q.destroy_unreferenced_disks {
        destroy_str = "1".to_string();
        params.push(("destroy-unreferenced-disks", &destroy_str));
    }
    let data = proxmox::delete(&host, &format!("nodes/{node}/qemu/{vmid}"), &params).await?;
    Ok(Json(data))
}

pub(super) async fn vm_migrate(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
    Json(req): Json<PveMigrateRequest>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let target = validate_node(&req.target)?.to_string();
    let online_str = if req.online.unwrap_or(true) { "1" } else { "0" }.to_string();
    let local_disks_str = if req.with_local_disks.unwrap_or(false) {
        "1"
    } else {
        "0"
    }
    .to_string();
    let mut params = vec![("target", target.as_str()), ("online", online_str.as_str())];
    if req.with_local_disks.is_some() {
        params.push(("with-local-disks", local_disks_str.as_str()));
    }
    let data = proxmox::post(&host, &format!("nodes/{node}/qemu/{vmid}/migrate"), &params).await?;
    Ok(Json(data))
}

// VM 快照
pub(super) async fn vm_snapshots(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let data = proxmox::get(&host, &format!("nodes/{node}/qemu/{vmid}/snapshot")).await?;
    Ok(Json(data))
}

pub(super) async fn vm_snapshot_create(
    State(state): State<AppState>,
    Path(p): Path<VmPath>,
    Json(req): Json<PveSnapshotRequest>,
) -> AppResult<Json<Value>> {
    snapshot_create(&state, &p.id, &p.node, &p.vmid, "qemu", &req).await
}

pub(super) async fn vm_snapshot_delete(
    State(state): State<AppState>,
    Path(p): Path<SnapPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let snap = validate_snapshot(&p.snap)?;
    let data = proxmox::delete(
        &host,
        &format!("nodes/{node}/qemu/{vmid}/snapshot/{snap}"),
        &[],
    )
    .await?;
    Ok(Json(data))
}

pub(super) async fn vm_snapshot_rollback(
    State(state): State<AppState>,
    Path(p): Path<SnapPath>,
) -> AppResult<Json<Value>> {
    let host = find_host(&state, &p.id).await?;
    let node = validate_node(&p.node)?;
    let vmid = validate_vmid(&p.vmid)?;
    let snap = validate_snapshot(&p.snap)?;
    let data = proxmox::post(
        &host,
        &format!("nodes/{node}/qemu/{vmid}/snapshot/{snap}/rollback"),
        &[],
    )
    .await?;
    Ok(Json(data))
}
