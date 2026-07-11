use crate::{
    domain::{ServiceStatus, SunshineStatus},
    error::AppResult,
    state::AppState,
};

pub async fn all_services(state: &AppState) -> AppResult<Vec<ServiceStatus>> {
    let hosts = state.hosts.sunshine.read().await.clone();
    let mut services = Vec::with_capacity(hosts.len());
    for host in &hosts {
        services.push(sunshine_host_service_status(host).await);
    }
    Ok(services)
}

async fn sunshine_host_service_status(
    host: &crate::app_config::SunshineHostConfig,
) -> ServiceStatus {
    let reachable = crate::sunshine::check_reachable(host).await;
    ServiceStatus {
        name: format!("sunshine:{}", host.name),
        kind: "streaming-host".to_string(),
        runtime_state: if reachable { "reachable" } else { "unknown" }.to_string(),
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
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

pub async fn sunshine_host_status(host: &crate::app_config::SunshineHostConfig) -> SunshineStatus {
    let reachable = crate::sunshine::check_reachable(host).await;
    SunshineStatus {
        host: host.host.clone(),
        web_port: host.web_port,
        web_url: crate::sunshine::web_url(host),
        reachable,
        mac_configured: host.mac_address.is_some(),
        message: if reachable {
            "Sunshine Web UI port is reachable"
        } else {
            "Sunshine Web UI port is not reachable"
        }
        .to_string(),
    }
}
