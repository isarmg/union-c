//! 控制台服务状态聚合。

use chrono::Utc;

use crate::{
    domain::{ServiceStatus, SunshineStatus},
    error::AppResult,
    state::AppState,
};

use super::lifecycle::ram_status;

/// 汇总所有服务状态。
pub async fn all_services(state: &AppState) -> AppResult<Vec<ServiceStatus>> {
    let mut services = vec![ram_status(state).await?];
    // 每台 Sunshine 主机单独一张服务卡片
    let hosts = state.hosts.sunshine.read().await;
    for host in hosts.iter() {
        services.push(sunshine_host_service_status(host).await);
    }
    services.push(blog_service_status(state));
    Ok(services)
}

/// 按主机配置探测 Sunshine 并返回服务卡片状态。
pub async fn sunshine_host_service_status(
    host: &crate::app_config::SunshineHostConfig,
) -> ServiceStatus {
    let reachable = crate::sunshine::check_reachable(host).await;
    ServiceStatus {
        name: format!("sunshine:{}", host.name),
        kind: "streaming-host".to_string(),
        runtime_state: if reachable {
            "reachable".to_string()
        } else {
            "unknown".to_string()
        },
        healthy: reachable,
        address: Some(crate::sunshine::web_url(host)),
        pid: None,
        message: format!(
            "{} — {}",
            host.name,
            if reachable {
                "port reachable"
            } else {
                "unreachable"
            }
        ),
        updated_at: Utc::now().to_rfc3339(),
    }
}

/// 探测单台 Sunshine 主机（供路由层直接调用）。
pub async fn sunshine_host_status(host: &crate::app_config::SunshineHostConfig) -> SunshineStatus {
    let reachable = crate::sunshine::check_reachable(host).await;
    SunshineStatus {
        host: host.host.clone(),
        web_port: host.web_port,
        web_url: crate::sunshine::web_url(host),
        reachable,
        mac_configured: host.mac_address.is_some(),
        message: if reachable {
            "Sunshine Web UI port is reachable".to_string()
        } else {
            "Sunshine Web UI port is not reachable".to_string()
        },
    }
}

/// 博客服务状态。
///
/// 博客不是常驻进程，这里主要展示构建工作目录是否存在。
fn blog_service_status(state: &AppState) -> ServiceStatus {
    let work_dir_exists = state.settings.blog.work_dir.exists();
    let export_dir_exists = state.settings.paths.blog_export_dir.exists();
    let healthy = work_dir_exists && export_dir_exists;

    ServiceStatus {
        name: "blog".to_string(),
        kind: "static-site".to_string(),
        runtime_state: if healthy { "ready" } else { "not-configured" }.to_string(),
        healthy,
        address: Some(state.settings.blog.work_dir.to_string_lossy().to_string()),
        pid: None,
        message: if healthy {
            "blog work directory and export directory exist".to_string()
        } else {
            "blog work directory or export directory is missing".to_string()
        },
        updated_at: Utc::now().to_rfc3339(),
    }
}
