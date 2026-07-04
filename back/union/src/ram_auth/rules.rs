//! RAM 文本权限规则的解析、校验与格式化。

use super::*;

pub(super) fn is_known_weak_password(password: &str) -> bool {
    matches!(
        password.to_ascii_lowercase().as_str(),
        "change-me" | "guest" | "password" | "admin" | "12345678"
    )
}

/// 把数据库账号记录转换成前端响应。
pub(super) fn response_from_accounts(
    settings: &Settings,
    accounts: Vec<ServiceAccountRecord>,
) -> RamAuthResponse {
    let management = accounts
        .iter()
        .find(|account| account.is_management)
        .and_then(record_to_pair);

    let rules = accounts
        .iter()
        .filter(|account| !account.is_management)
        .filter_map(|account| record_to_rule(account).ok())
        .map(|rule| to_response_rule(&rule))
        .collect();

    RamAuthResponse {
        storage: STORAGE_LABEL.to_string(),
        auth_method: settings.ram.auth_method.clone(),
        management_auth_configured: management.is_some(),
        management_username: management.map(|(username, _)| username),
        rules,
    }
}

/// 把数据库账号记录转换成内部 ParsedAuthRule。
pub(super) fn record_to_rule(account: &ServiceAccountRecord) -> AppResult<ParsedAuthRule> {
    Ok(ParsedAuthRule {
        username: if account.is_anonymous {
            None
        } else {
            account.username.clone()
        },
        password: account.password_secret.clone(),
        paths: account
            .permissions
            .iter()
            .map(|permission| RamAuthPath {
                path: permission.resource_path.clone(),
                permission: permission.permission.clone(),
            })
            .collect(),
    })
}

/// 从数据库记录中取出 username/password 二元组。
pub(super) fn record_to_pair(account: &ServiceAccountRecord) -> Option<(String, String)> {
    Some((account.username.clone()?, account.password_secret.clone()?))
}

/// 解析配置文件里已有的原始 auth 规则。
pub(super) fn parse_existing_rules(raw_rules: &[String]) -> AppResult<Vec<ParsedAuthRule>> {
    raw_rules
        .iter()
        .map(|rule| parse_auth_rule(rule))
        .collect::<AppResult<Vec<_>>>()
}

/// 找到第一条具名账号规则，用作管理账号的兜底来源。
pub(super) fn first_named_rule(raw_rules: &[String]) -> Option<(String, String)> {
    raw_rules
        .iter()
        .filter_map(|rule| parse_auth_rule(rule).ok())
        .find_map(|rule| Some((rule.username?, rule.password?)))
}

/// 解析单条 ram auth 规则。
///
/// 支持形如：
/// - `user:pass@/public:ro,/private:rw`
/// - `@/public` 匿名只读规则
pub(super) fn parse_auth_rule(rule: &str) -> AppResult<ParsedAuthRule> {
    // ram 原生格式为 user:password@/path:rw,/other，匿名账号则省略 user:password。
    let (account, paths) = rule
        .rsplit_once('@')
        .ok_or_else(|| AppError::BadRequest(format!("invalid ram auth rule: {rule}")))?;

    let (username, password) = if account.is_empty() {
        (None, None)
    } else {
        let (username, password) = account
            .split_once(':')
            .ok_or_else(|| AppError::BadRequest(format!("invalid ram auth account: {rule}")))?;
        if username.trim().is_empty() || password.is_empty() {
            return Err(AppError::BadRequest(format!(
                "invalid ram auth account: {rule}"
            )));
        }
        (
            Some(username.trim().to_string()),
            Some(password.to_string()),
        )
    };

    Ok(ParsedAuthRule {
        username,
        password,
        paths: parse_paths(paths)?,
    })
}

/// 解析 auth 规则中的路径列表。
pub(super) fn parse_paths(paths: &str) -> AppResult<Vec<RamAuthPath>> {
    let mut output = Vec::new();
    for item in paths
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let (path, permission) = match item.rsplit_once(':') {
            Some((path, permission)) if permission.eq_ignore_ascii_case("rw") => {
                (path, "rw".to_string())
            }
            Some((path, permission)) if permission.eq_ignore_ascii_case("ro") => {
                (path, "ro".to_string())
            }
            _ => (item, "ro".to_string()),
        };
        output.push(RamAuthPath {
            path: normalize_path(path)?,
            permission,
        });
    }

    if output.is_empty() {
        return Err(AppError::BadRequest(
            "each ram auth rule needs at least one path".to_string(),
        ));
    }
    Ok(output)
}

/// 规范化前端提交的单条规则。
///
/// 前端为了避免回传明文旧密码，可能把 password 留空；这里会尝试从旧记录中补回。
pub(super) fn normalize_input_rule(
    rule: RamAuthRuleInput,
    existing_passwords: &HashMap<String, String>,
) -> AppResult<ParsedAuthRule> {
    let username = rule
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let password = match &username {
        Some(username) => Some(resolve_password(
            username,
            rule.password,
            existing_passwords,
        )?),
        None => None,
    };

    let paths = rule
        .paths
        .into_iter()
        .map(|path| {
            let permission = path.permission.trim().to_ascii_lowercase();
            if !matches!(permission.as_str(), "ro" | "rw") {
                return Err(AppError::BadRequest(format!(
                    "invalid ram permission '{}', use ro or rw",
                    path.permission
                )));
            }
            Ok(RamAuthPath {
                path: normalize_path(&path.path)?,
                permission,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;

    if paths.is_empty() {
        return Err(AppError::BadRequest(
            "each ram auth rule needs at least one path".to_string(),
        ));
    }

    validate_account(&username, password.as_deref())?;
    Ok(ParsedAuthRule {
        username,
        password,
        paths,
    })
}

/// 解析最终要保存的密码。
pub(super) fn resolve_password(
    username: &str,
    requested: Option<String>,
    existing_passwords: &HashMap<String, String>,
) -> AppResult<String> {
    if let Some(password) = requested
        && !password.is_empty()
    {
        return Ok(password);
    }

    existing_passwords.get(username).cloned().ok_or_else(|| {
        AppError::BadRequest(format!(
            "password is required for new ram user '{username}'"
        ))
    })
}

/// 校验账号名和密码。
pub(super) fn validate_account(username: &Option<String>, password: Option<&str>) -> AppResult<()> {
    let Some(username) = username else {
        return Ok(());
    };
    if username.contains([':', '@', '\n', '\r']) {
        return Err(AppError::BadRequest(format!(
            "invalid ram username '{username}'"
        )));
    }
    let Some(password) = password else {
        return Err(AppError::BadRequest(format!(
            "password is required for ram user '{username}'"
        )));
    };
    if password.is_empty() || password.contains(['\n', '\r']) {
        return Err(AppError::BadRequest(format!(
            "invalid ram password for user '{username}'"
        )));
    }
    Ok(())
}

/// 校验整组权限规则。
///
/// 同一个用户名不能重复；匿名规则也最多只能有一条。
pub(super) fn validate_rules(rules: &[ParsedAuthRule]) -> AppResult<()> {
    let mut anonymous_seen = false;
    let mut usernames = HashSet::new();

    // ram 规则不能出现重复用户名，匿名规则也只能有一条，否则权限语义会变得不确定。
    for rule in rules {
        if let Some(username) = &rule.username {
            if !usernames.insert(username.clone()) {
                return Err(AppError::BadRequest(format!(
                    "duplicate ram user '{username}'"
                )));
            }
        } else if anonymous_seen {
            return Err(AppError::BadRequest(
                "only one anonymous ram auth rule is allowed".to_string(),
            ));
        } else {
            anonymous_seen = true;
        }
    }

    Ok(())
}

/// 规范化 ram 路径。
///
/// ram 权限路径必须以 `/` 开头，且不能包含 `..` 这种越界片段。
pub(super) fn normalize_path(path: &str) -> AppResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(AppError::BadRequest(
            "ram auth path cannot be empty".to_string(),
        ));
    }
    if path.contains([',', '\\', '\n', '\r', '\0', '@', '?', '#']) {
        return Err(AppError::BadRequest(format!(
            "invalid ram auth path '{path}'"
        )));
    }

    let mut segments = Vec::new();
    for segment in path.trim_matches('/').split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || segment.contains(':') {
            return Err(AppError::BadRequest(format!(
                "invalid ram auth path '{path}'"
            )));
        }
        segments.push(segment);
    }

    Ok(if segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segments.join("/"))
    })
}

/// 规范化管理账号。
///
/// 管理账号可以单独配置，也可以复用提交的普通账号密码。
pub(super) fn normalize_management_auth(
    username: &Option<String>,
    password: &Option<String>,
    current: Option<&(String, String)>,
    submitted_passwords: &HashMap<String, String>,
) -> AppResult<Option<(String, String)>> {
    let Some(username) = username.as_deref() else {
        return Ok(current.cloned());
    };
    let username = username.trim();
    if username.is_empty() {
        return Ok(None);
    }
    if username.contains([':', '\n', '\r']) {
        return Err(AppError::BadRequest(format!(
            "invalid management username '{username}'"
        )));
    }

    let password = match password {
        Some(password) if !password.is_empty() => password.clone(),
        // 管理账号密码留空时，先复用同名访问账号的新密码，再复用旧管理密码。
        _ => submitted_passwords
            .get(username)
            .cloned()
            .or_else(|| {
                current
                    .filter(|(current_user, _)| current_user == username)
                    .map(|(_, current_password)| current_password.clone())
            })
            .ok_or_else(|| {
                AppError::BadRequest(format!("management password is required for '{username}'"))
            })?,
    };
    if password.contains(['\n', '\r']) {
        return Err(AppError::BadRequest(
            "invalid management password".to_string(),
        ));
    }

    Ok(Some((username.to_string(), password)))
}

/// 解析配置文件中的 `management_auth`。
pub(super) fn parse_management_auth(value: Option<&str>) -> Option<(String, String)> {
    let (username, password) = value?.split_once(':')?;
    if username.is_empty() || password.is_empty() {
        return None;
    }
    Some((username.to_string(), password.to_string()))
}

/// 把内部规则转换成数据库写入结构。
pub(super) fn to_account_input(rule: &ParsedAuthRule) -> ServiceAccountInput {
    ServiceAccountInput {
        account_key: rule
            .username
            .clone()
            .unwrap_or_else(|| ANONYMOUS_KEY.to_string()),
        username: rule.username.clone(),
        password_secret: rule.password.clone(),
        is_anonymous: rule.username.is_none(),
        is_management: false,
        permissions: rule
            .paths
            .iter()
            .map(|path| ServicePermissionInput {
                resource_path: path.path.clone(),
                permission: path.permission.clone(),
            })
            .collect(),
    }
}

/// 把内部规则格式化成 ram 命令行 `--auth` 文本。
pub(super) fn format_auth_rule(rule: &ParsedAuthRule) -> String {
    let paths = rule
        .paths
        .iter()
        .map(|path| {
            if path.permission == "rw" {
                format!("{}:rw", path.path)
            } else {
                path.path.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(",");

    match (&rule.username, &rule.password) {
        (Some(username), Some(password)) => format!("{username}:{password}@{paths}"),
        _ => format!("@{paths}"),
    }
}

/// 把内部规则转换成前端展示结构。
pub(super) fn to_response_rule(rule: &ParsedAuthRule) -> RamAuthRuleResponse {
    RamAuthRuleResponse {
        username: rule.username.clone(),
        anonymous: rule.username.is_none(),
        password_set: rule
            .password
            .as_ref()
            .is_some_and(|password| !password.is_empty()),
        paths: rule.paths.clone(),
        raw: service_manager::redact_auth_rule(&format_auth_rule(rule)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_cleans_relative_input() {
        assert_eq!(normalize_path("public//images").unwrap(), "/public/images");
        assert_eq!(normalize_path("/").unwrap(), "/");
    }

    #[test]
    fn normalize_path_rejects_ambiguous_or_escaping_input() {
        assert!(normalize_path("../private").is_err());
        assert!(normalize_path("/public/../private").is_err());
        assert!(normalize_path("/public?download").is_err());
        assert!(normalize_path("/public:rw").is_err());
        assert!(normalize_path("\\media\\covers").is_err());
    }
}
