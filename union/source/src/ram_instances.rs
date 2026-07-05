use std::time::Duration;

use futures_util::{StreamExt, TryStreamExt, stream};

use crate::{
    database::{self, RamInstanceRecord},
    domain::{RamInstanceInfo, RamInstanceSaveRequest},
    error::{AppError, AppResult},
    http_client, network, ram_auth,
    state::AppState,
};

pub fn service_key(id: &str) -> String {
    format!("ram:{id}")
}

async fn info(state: &AppState, record: RamInstanceRecord) -> AppResult<RamInstanceInfo> {
    let auth = ram_auth::current_auth_for(state, &service_key(&record.id)).await?;
    let scheme = if record.use_tls { "https" } else { "http" };
    let url = format!(
        "{scheme}://{}",
        network::authority(&record.host_address, record.port)
    );
    if remote_transport_error(&record, state.settings.production).is_some() {
        return Ok(RamInstanceInfo {
            url,
            id: record.id,
            name: record.name,
            host: record.host_address,
            port: record.port,
            use_tls: record.use_tls,
            verify_tls: record.verify_tls,
            reachable: false,
            management_username: auth.management_username,
            management_password_set: auth.management_auth_configured,
        });
    }
    let client = http_client::for_tls(!record.use_tls || record.verify_tls)?;
    let mut health_request = client
        .get(format!("{url}/__ram__/health"))
        .timeout(Duration::from_secs(3));
    if let Some((username, password)) =
        ram_auth::management_auth_pair_for(state, &service_key(&record.id)).await?
    {
        health_request = health_request.basic_auth(username, Some(password));
    }
    // 收到任意 HTTP 响应都说明远程 RAM 可达；401 仅表示凭据需调整。
    let reachable = health_request.send().await.is_ok();
    Ok(RamInstanceInfo {
        url,
        id: record.id,
        name: record.name,
        host: record.host_address,
        port: record.port,
        use_tls: record.use_tls,
        verify_tls: record.verify_tls,
        reachable,
        management_username: auth.management_username,
        management_password_set: auth.management_auth_configured,
    })
}

pub async fn list(state: &AppState) -> AppResult<Vec<RamInstanceInfo>> {
    stream::iter(
        database::ram_instances(state.db().as_ref())
            .await?
            .into_iter()
            .map(|record| info(state, record)),
    )
    .buffered(8)
    .try_collect()
    .await
}

pub async fn create(state: &AppState, req: RamInstanceSaveRequest) -> AppResult<RamInstanceInfo> {
    validate(&req, state.settings.production)?;
    let record = RamInstanceRecord {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name.trim().to_string(),
        host_address: normalize_host(&req.host),
        port: req.port,
        use_tls: req.use_tls,
        verify_tls: req.verify_tls,
    };
    database::insert_ram_instance(state.db().as_ref(), &record).await?;
    database::insert_audit(
        state.db().as_ref(),
        "ram.instance.create",
        &record.id,
        Some(&format!(
            "name={} host={} port={} tls={}",
            record.name, record.host_address, record.port, record.use_tls
        )),
    )
    .await?;
    info(state, record).await
}

pub async fn update(
    state: &AppState,
    id: &str,
    req: RamInstanceSaveRequest,
) -> AppResult<RamInstanceInfo> {
    validate(&req, state.settings.production)?;
    let mut record = get(state, id).await?;
    record.name = req.name.trim().to_string();
    record.host_address = normalize_host(&req.host);
    record.port = req.port;
    record.use_tls = req.use_tls;
    record.verify_tls = req.verify_tls;
    database::update_ram_instance(state.db().as_ref(), &record).await?;
    database::insert_audit(
        state.db().as_ref(),
        "ram.instance.update",
        &record.id,
        Some(&format!(
            "name={} host={} port={} tls={}",
            record.name, record.host_address, record.port, record.use_tls
        )),
    )
    .await?;
    info(state, record).await
}

pub async fn delete(state: &AppState, id: &str) -> AppResult<()> {
    let _ = get(state, id).await?;
    database::delete_ram_instance(state.db().as_ref(), id).await?;
    database::insert_audit(
        state.db().as_ref(),
        "ram.instance.delete",
        id,
        Some("remote RAM instance removed"),
    )
    .await?;
    Ok(())
}

pub async fn apply_remote_auth(
    state: &AppState,
    id: &str,
    credential: &(String, String),
    rules: &[String],
) -> AppResult<()> {
    let record = get(state, id).await?;
    ensure_remote_transport_allowed(&record, state.settings.production)?;
    let scheme = if record.use_tls { "https" } else { "http" };
    let authority = network::authority(&record.host_address, record.port);
    let client = http_client::for_tls(!record.use_tls || record.verify_tls)?;
    let response = client
        .put(format!("{scheme}://{authority}/__ram__/admin/auth"))
        .timeout(Duration::from_secs(10))
        .basic_auth(&credential.0, Some(&credential.1))
        .json(&serde_json::json!({ "rules": rules }))
        .send()
        .await
        .map_err(|error| AppError::Upstream(format!("连接远程 RAM 管理接口失败: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Upstream(format!(
            "远程 RAM 拒绝认证配置更新（HTTP {}）",
            response.status()
        )));
    }
    Ok(())
}

async fn get(state: &AppState, id: &str) -> AppResult<RamInstanceRecord> {
    database::ram_instance(state.db().as_ref(), id)
        .await?
        .ok_or_else(|| AppError::BadRequest("RAM remote host not found".into()))
}

fn normalize_host(value: &str) -> String {
    network::normalize_host(value)
}

fn validate(req: &RamInstanceSaveRequest, production: bool) -> AppResult<()> {
    if req.name.trim().is_empty() || req.host.trim().is_empty() {
        return Err(AppError::BadRequest("name and host are required".into()));
    }
    if req.port == 0 {
        return Err(AppError::BadRequest("port must be greater than 0".into()));
    }
    let host = normalize_host(&req.host);
    if !network::is_valid_host(&host) {
        return Err(AppError::InvalidHost(
            "host must be IPv4, IPv6 or a domain".into(),
        ));
    }
    if production && !req.use_tls {
        return Err(AppError::BadRequest(
            "生产环境远程 RAM 必须使用 HTTPS".into(),
        ));
    }
    if production && !req.verify_tls {
        return Err(AppError::BadRequest("生产环境远程 RAM 必须校验证书".into()));
    }
    Ok(())
}

fn ensure_remote_transport_allowed(record: &RamInstanceRecord, production: bool) -> AppResult<()> {
    if let Some(message) = remote_transport_error(record, production) {
        return Err(AppError::BadRequest(message));
    }
    Ok(())
}

fn remote_transport_error(record: &RamInstanceRecord, production: bool) -> Option<String> {
    if !production {
        return None;
    }
    if !record.use_tls {
        return Some("生产环境远程 RAM 必须使用 HTTPS".to_string());
    }
    if !record.verify_tls {
        return Some("生产环境远程 RAM 必须校验证书".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(use_tls: bool, verify_tls: bool) -> RamInstanceSaveRequest {
        RamInstanceSaveRequest {
            name: "remote".to_string(),
            host: "192.0.2.10".to_string(),
            port: 5000,
            use_tls,
            verify_tls,
        }
    }

    #[test]
    fn production_requires_https_and_certificate_verification() {
        assert!(validate(&request(false, true), true).is_err());
        assert!(validate(&request(true, false), true).is_err());
        assert!(validate(&request(true, true), true).is_ok());
        assert!(validate(&request(false, false), false).is_ok());
    }

    #[test]
    fn rejects_zero_port() {
        let req = RamInstanceSaveRequest {
            port: 0,
            ..request(true, true)
        };

        assert!(validate(&req, false).is_err());
    }
}
