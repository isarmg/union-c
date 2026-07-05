//! Proxmox VE 管理 handler。
//!
//! 路由结构（均挂载在 /api/pve/ 前缀下）：
//!
//! 主机 CRUD：
//!   GET    /hosts                        列出所有 PVE 主机
//!   POST   /hosts                        新建主机
//!   PUT    /hosts/:id                    更新主机
//!   DELETE /hosts/:id                    删除主机
//!
//! 集群概览：
//!   GET    /hosts/:id/resources          cluster/resources（所有 VM/CT/节点）
//!   GET    /hosts/:id/nodes              节点列表
//!   GET    /hosts/:id/tasks              最近任务
//!
//! 节点详情：
//!   GET    /hosts/:id/nodes/:node/status                  节点状态
//!   GET    /hosts/:id/nodes/:node/storage                 节点存储列表
//!   GET    /hosts/:id/nodes/:node/storage/:storage/content 存储内容
//!   GET    /hosts/:id/nodes/:node/tasks                   节点任务
//!
//! VM (QEMU) 操作：
//!   GET    /hosts/:id/nodes/:node/vms                     列出 VM
//!   GET    /hosts/:id/nodes/:node/vms/:vmid/status        VM 状态
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/start         启动
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/stop          强制停止
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/shutdown      ACPI 关机
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/reboot        重启
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/suspend       挂起
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/resume        恢复
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/reset         重置
//!   GET    /hosts/:id/nodes/:node/vms/:vmid/config        配置
//!   DELETE /hosts/:id/nodes/:node/vms/:vmid               删除 VM
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/migrate       迁移
//!   GET    /hosts/:id/nodes/:node/vms/:vmid/snapshots         快照列表
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/snapshots         创建快照
//!   DELETE /hosts/:id/nodes/:node/vms/:vmid/snapshots/:snap   删除快照
//!   POST   /hosts/:id/nodes/:node/vms/:vmid/snapshots/:snap/rollback 回滚
//!
//! Container (LXC) 操作（同 VM，路径为 containers）：
//!   GET    /hosts/:id/nodes/:node/containers                   列出 CT
//!   GET    /hosts/:id/nodes/:node/containers/:vmid/status      CT 状态
//!   POST   /hosts/:id/nodes/:node/containers/:vmid/start       启动
//!   POST   /hosts/:id/nodes/:node/containers/:vmid/stop        停止
//!   POST   /hosts/:id/nodes/:node/containers/:vmid/shutdown    关机
//!   POST   /hosts/:id/nodes/:node/containers/:vmid/reboot      重启
//!   GET    /hosts/:id/nodes/:node/containers/:vmid/config      配置
//!   DELETE /hosts/:id/nodes/:node/containers/:vmid             删除 CT
//!   GET    /hosts/:id/nodes/:node/containers/:vmid/snapshots       快照列表
//!   POST   /hosts/:id/nodes/:node/containers/:vmid/snapshots       创建快照
//!   DELETE /hosts/:id/nodes/:node/containers/:vmid/snapshots/:snap 删除快照
//!   POST   /hosts/:id/nodes/:node/containers/:vmid/snapshots/:snap/rollback 回滚

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    app_config::ProxmoxHostConfig,
    database,
    domain::{
        PveDeleteQuery, PveHostInfo, PveHostSaveRequest, PveMigrateRequest, PveSnapshotRequest,
    },
    error::{AppError, AppResult},
    network, proxmox,
    state::AppState,
};

mod common;
mod containers;
mod hosts;
mod vms;

use containers::*;
use hosts::*;
use vms::*;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/pve/hosts", get(list_hosts).post(create_host))
        .route("/api/pve/hosts/{id}", put(update_host).delete(delete_host))
        .route("/api/pve/hosts/{id}/resources", get(cluster_resources))
        .route("/api/pve/hosts/{id}/nodes", get(cluster_nodes))
        .route("/api/pve/hosts/{id}/tasks", get(cluster_tasks))
        .route("/api/pve/hosts/{id}/nodes/{node}/status", get(node_status))
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/storage",
            get(node_storage),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/storage/{storage}/content",
            get(storage_content),
        )
        .route("/api/pve/hosts/{id}/nodes/{node}/tasks", get(node_tasks))
        .route("/api/pve/hosts/{id}/nodes/{node}/vms", get(list_vms))
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/status",
            get(vm_status),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/config",
            get(vm_config),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}",
            delete(vm_delete),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/start",
            post(vm_start),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/stop",
            post(vm_stop),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/shutdown",
            post(vm_shutdown),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/reboot",
            post(vm_reboot),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/suspend",
            post(vm_suspend),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/resume",
            post(vm_resume),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/reset",
            post(vm_reset),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/migrate",
            post(vm_migrate),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/snapshots",
            get(vm_snapshots).post(vm_snapshot_create),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/snapshots/{snap}",
            delete(vm_snapshot_delete),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}/snapshots/{snap}/rollback",
            post(vm_snapshot_rollback),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers",
            get(list_containers),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/status",
            get(ct_status),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/config",
            get(ct_config),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}",
            delete(ct_delete),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/start",
            post(ct_start),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/stop",
            post(ct_stop),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/shutdown",
            post(ct_shutdown),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/reboot",
            post(ct_reboot),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/snapshots",
            get(ct_snapshots).post(ct_snapshot_create),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/snapshots/{snap}",
            delete(ct_snapshot_delete),
        )
        .route(
            "/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}/snapshots/{snap}/rollback",
            post(ct_snapshot_rollback),
        )
}

// ─── 路径参数结构体 ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(super) struct HostPath {
    pub id: String,
}

#[derive(Deserialize)]
pub(super) struct NodePath {
    pub id: String,
    pub node: String,
}

#[derive(Deserialize)]
pub(super) struct VmPath {
    pub id: String,
    pub node: String,
    pub vmid: String,
}

#[derive(Deserialize)]
pub(super) struct SnapPath {
    pub id: String,
    pub node: String,
    pub vmid: String,
    pub snap: String,
}

#[derive(Deserialize)]
pub(super) struct StoragePath {
    pub id: String,
    pub node: String,
    pub storage: String,
}
