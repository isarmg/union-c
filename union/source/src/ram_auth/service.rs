//! RAM 账号用例与数据库协调。

use super::{rules::*, *};

/// 首次启动时把配置文件中的 ram auth 规则写入数据库。
///
/// 数据库中已有账号时保持现有配置；仅在账号集合为空时写入默认规则。
pub async fn ensure_seeded(pool: &DbPool, settings: &Settings) -> anyhow::Result<()> {
    if database::count_service_accounts(pool, RAM_SERVICE).await? > 0 {
        return Ok(());
    }

    // 首次启动时用默认 auth 规则初始化 PostgreSQL，之后以数据库为准。
    let mut accounts = parse_existing_rules(&settings.ram.auth)?
        .iter()
        .map(to_account_input)
        .collect::<Vec<_>>();

    // 管理账号优先使用单独配置，缺省时复用第一个具名 ram 账号。
    if let Some((username, password)) =
        parse_management_auth(settings.ram.management_auth.as_deref())
            .or_else(|| first_named_rule(&settings.ram.auth))
    {
        accounts.push(ServiceAccountInput {
            account_key: MANAGEMENT_KEY.to_string(),
            username: Some(username),
            password_secret: Some(password),
            is_anonymous: false,
            is_management: true,
            permissions: Vec::new(),
        });
    }

    database::replace_service_accounts(pool, RAM_SERVICE, &accounts).await?;
    Ok(())
}

/// 读取当前 ram 权限配置，供前端展示。
pub async fn current_auth(state: &AppState) -> AppResult<RamAuthResponse> {
    current_auth_for(state, RAM_SERVICE).await
}

pub async fn current_auth_for(state: &AppState, service: &str) -> AppResult<RamAuthResponse> {
    let accounts = database::service_accounts(state.db().as_ref(), service).await?;
    Ok(response_from_accounts(&state.settings, accounts))
}

/// 生成 ram 启动时需要的 `--auth` 规则列表。
pub async fn auth_rules_for_ram(state: &AppState) -> AppResult<Vec<String>> {
    auth_rules_for(state, RAM_SERVICE).await
}

pub async fn auth_rules_for(state: &AppState, service: &str) -> AppResult<Vec<String>> {
    let accounts = database::service_accounts(state.db().as_ref(), service).await?;
    // 管理员账号同时作为 ram 根路径读写账号，浏览器可用同一组凭据登录实例。
    // 若普通规则复用了管理员用户名，只保留管理员规则，避免同名规则权限冲突。
    let management_username = accounts
        .iter()
        .find(|account| account.is_management)
        .and_then(|account| account.username.as_deref());
    Ok(accounts
        .iter()
        .filter(|account| {
            account.is_management || account.username.as_deref() != management_username
        })
        .map(record_to_rule)
        .collect::<AppResult<Vec<_>>>()?
        .iter()
        .map(format_auth_rule)
        .collect())
}

/// 取出管理中心访问 ram 时使用的账号密码。
///
/// 优先使用管理账号；如果没有，则回退到第一个普通具名账号。
pub async fn management_auth_pair(state: &AppState) -> AppResult<Option<(String, String)>> {
    management_auth_pair_for(state, RAM_SERVICE).await
}

pub async fn management_auth_pair_for(
    state: &AppState,
    service: &str,
) -> AppResult<Option<(String, String)>> {
    let accounts = database::service_accounts(state.db().as_ref(), service).await?;
    if let Some(pair) = accounts
        .iter()
        .find(|account| account.is_management)
        .and_then(record_to_pair)
    {
        return Ok(Some(pair));
    }

    Ok(accounts
        .iter()
        .find(|account| !account.is_management && !account.is_anonymous)
        .and_then(record_to_pair))
}

/// 保存前端提交的权限配置。
///
/// 主要步骤：
/// 1. 读取旧账号，便于空密码时沿用旧密码；
/// 2. 规范化前端规则并校验；
/// 3. 写入数据库事务；
/// 4. 尝试重新加载 ram，让新权限生效。
pub async fn update_auth(
    state: &AppState,
    request: RamAuthUpdateRequest,
) -> AppResult<RamAuthUpdateResponse> {
    update_auth_for(state, request, RAM_SERVICE, true).await
}

pub async fn update_auth_for(
    state: &AppState,
    request: RamAuthUpdateRequest,
    service: &str,
    reload_managed_ram: bool,
) -> AppResult<RamAuthUpdateResponse> {
    let existing_accounts = database::service_accounts(state.db().as_ref(), service).await?;
    let existing_passwords = existing_accounts
        .iter()
        .filter(|account| !account.is_management)
        .filter_map(record_to_pair)
        .collect::<HashMap<_, _>>();
    let current_management = existing_accounts
        .iter()
        .find(|account| account.is_management)
        .and_then(record_to_pair);

    let next_rules = request
        .rules
        .into_iter()
        // 前端提交空密码表示保留旧密码，新账号则必须显式提供密码。
        .map(|rule| normalize_input_rule(rule, &existing_passwords))
        .collect::<AppResult<Vec<_>>>()?;
    validate_rules(&next_rules)?;

    let submitted_passwords = next_rules
        .iter()
        .filter_map(|rule| Some((rule.username.clone()?, rule.password.clone()?)))
        .collect::<HashMap<_, _>>();
    let management_auth = normalize_management_auth(
        &request.management_username,
        &request.management_password,
        current_management.as_ref(),
        &submitted_passwords,
    )?;
    if state.settings.production {
        let weak_rule = next_rules.iter().any(|rule| {
            rule.password
                .as_deref()
                .is_some_and(|password| password.len() < 12 || is_known_weak_password(password))
        });
        let weak_management = management_auth
            .as_ref()
            .is_some_and(|(_, password)| password.len() < 12 || is_known_weak_password(password));
        if weak_rule || weak_management {
            return Err(AppError::BadRequest(
                "生产环境 ram 密码至少需要 12 个字符，且不能使用默认密码".to_string(),
            ));
        }
    }

    let mut account_inputs = next_rules.iter().map(to_account_input).collect::<Vec<_>>();
    if let Some((username, password)) = &management_auth {
        account_inputs.push(ServiceAccountInput {
            account_key: MANAGEMENT_KEY.to_string(),
            username: Some(username.clone()),
            password_secret: Some(password.clone()),
            is_anonymous: false,
            is_management: true,
            permissions: Vec::new(),
        });
    }

    let remote_apply = if let Some(instance_id) = service.strip_prefix("ram:") {
        let credential = current_management
            .as_ref()
            .or(management_auth.as_ref())
            .ok_or_else(|| AppError::BadRequest("远程 RAM 必须先配置管理员账号密码".to_string()))?
            .clone();
        let management_username = management_auth.as_ref().map(|pair| pair.0.as_str());
        let mut remote_rules = next_rules
            .iter()
            .filter(|rule| rule.username.as_deref() != management_username)
            .map(format_auth_rule)
            .collect::<Vec<_>>();
        if let Some((username, password)) = &management_auth {
            remote_rules.insert(0, format!("{username}:{password}@/:rw"));
        }
        Some((instance_id.to_string(), credential, remote_rules))
    } else {
        None
    };

    database::replace_service_accounts(state.db().as_ref(), service, &account_inputs).await?;
    database::insert_audit(
        state.db().as_ref(),
        "ram.auth.update",
        service,
        Some(&format!("rules={}", next_rules.len())),
    )
    .await?;

    let mut applied = false;
    let mut ram_reloaded = false;
    let mut apply_error = None;
    if let Some((instance_id, credential, remote_rules)) = remote_apply {
        match ram_instances::apply_remote_auth(state, &instance_id, &credential, &remote_rules)
            .await
        {
            Ok(()) => applied = true,
            Err(error) => apply_error = Some(error.to_string()),
        }
    } else if reload_managed_ram {
        match service_manager::reload_managed_ram(state).await {
            Ok(reloaded) => {
                ram_reloaded = reloaded;
                applied = reloaded;
            }
            Err(error) => apply_error = Some(error.to_string()),
        }
    } else {
        applied = false;
    }
    let accounts = database::service_accounts(state.db().as_ref(), service).await?;
    let response = response_from_accounts(&state.settings, accounts);
    let message = auth_update_message(service, applied, ram_reloaded, apply_error.as_deref());

    Ok(RamAuthUpdateResponse {
        saved: true,
        applied,
        ram_reloaded,
        storage: STORAGE_LABEL.to_string(),
        management_auth_configured: response.management_auth_configured,
        management_username: response.management_username,
        rules: response.rules,
        message,
    })
}

fn auth_update_message(
    service: &str,
    applied: bool,
    ram_reloaded: bool,
    apply_error: Option<&str>,
) -> String {
    if let Some(error) = apply_error {
        if service.starts_with("ram:") {
            return format!("远程 RAM 账号已保存到 PostgreSQL，但热更新失败：{error}");
        }
        return format!("ram 账号已保存到 PostgreSQL，但未能重载当前运行服务：{error}");
    }
    if ram_reloaded {
        return "ram 账号已保存到 PostgreSQL，并已应用到当前运行服务".to_string();
    }
    if service.starts_with("ram:") && applied {
        return "远程 RAM 账号已保存到 PostgreSQL，并已热更新".to_string();
    }
    "ram 账号已保存到 PostgreSQL；ram 下次启动会直接使用".to_string()
}
