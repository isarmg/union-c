//! RAM 启动配置生成和安全展示。

use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

use crate::{
    domain::{RamCommandResponse, RamConfigResponse, RamFeatures},
    error::{AppError, AppResult},
    ram_auth,
    state::AppState,
};

use super::client::{ram_base_url, ram_health_url};

const RAM_GENERATED_CONFIG: &str = "ram/data/ram.generated.yaml";

pub(super) struct RamCommandSpec {
    pub(super) program: String,
    pub(super) args: Vec<String>,
}

pub(super) async fn ram_start_command_spec(state: &AppState) -> AppResult<RamCommandSpec> {
    let auth_rules = ram_auth::auth_rules_for_ram(state).await?;
    validate_auth_rules_for_start(state, &auth_rules)?;
    write_private_ram_config(state, &auth_rules)?;
    Ok(ram_command_spec(state))
}

fn ram_command_spec(state: &AppState) -> RamCommandSpec {
    let mut args = vec!["--config".to_string(), RAM_GENERATED_CONFIG.to_string()];
    if !state.settings.ram.auth_method.trim().is_empty() {
        args.push("--auth-method".to_string());
        args.push(state.settings.ram.auth_method.clone());
    }
    args.extend(state.settings.ram.extra_args.clone());
    RamCommandSpec {
        program: state.settings.ram.command.clone(),
        args,
    }
}

fn validate_auth_rules_for_start(state: &AppState, auth_rules: &[String]) -> AppResult<()> {
    if state.settings.production && auth_rules.is_empty() {
        return Err(AppError::BadRequest(
            "configure at least one ram account before starting it in production".to_string(),
        ));
    }
    if state.settings.production && auth_rules.iter().any(|rule| weak_auth_rule(rule)) {
        return Err(AppError::BadRequest(
            "replace weak ram credentials before starting it in production".to_string(),
        ));
    }
    Ok(())
}

fn weak_auth_rule(rule: &str) -> bool {
    let Some((account, _)) = rule.rsplit_once('@') else {
        return true;
    };
    if account.is_empty() {
        return false;
    }
    let Some((_, password)) = account.split_once(':') else {
        return true;
    };
    password.len() < 12
        || matches!(
            password.to_ascii_lowercase().as_str(),
            "change-me" | "guest" | "password" | "admin" | "12345678"
        )
}

fn write_private_ram_config(state: &AppState, auth_rules: &[String]) -> AppResult<()> {
    let settings = &state.settings;
    let quoted = |value: &str| serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string());
    let mut lines = vec![
        format!(
            "serve-path: {}",
            quoted(&settings.paths.data_dir.to_string_lossy())
        ),
        format!("bind: {}", quoted(&settings.ram.bind)),
        format!("port: {}", settings.ram.port),
        format!("path-prefix: {}", quoted(&settings.ram.path_prefix)),
        format!("allow-all: {}", settings.ram.allow_all),
        format!("allow-upload: {}", settings.ram.allow_upload),
        format!("allow-delete: {}", settings.ram.allow_delete),
        format!("allow-search: {}", settings.ram.allow_search),
        format!("allow-symlink: {}", settings.ram.allow_symlink),
        format!("allow-archive: {}", settings.ram.allow_archive),
        format!("allow-hash: {}", settings.ram.allow_hash),
        format!("enable-cors: {}", settings.ram.enable_cors),
        format!("render-index: {}", settings.ram.render_index),
        format!("render-try-index: {}", settings.ram.render_try_index),
        format!("render-spa: {}", settings.ram.render_spa),
        format!("compress: {}", quoted(&settings.ram.compress)),
        "hidden:".to_string(),
    ];
    lines.extend(
        settings
            .ram
            .hidden
            .iter()
            .map(|value| format!("  - {}", quoted(value))),
    );
    if auth_rules.is_empty() {
        lines.push("auth: []".to_string());
    } else {
        lines.push("auth:".to_string());
        lines.extend(
            auth_rules
                .iter()
                .map(|value| format!("  - {}", quoted(value))),
        );
    }
    if let Some(value) = &settings.ram.assets {
        lines.push(format!("assets: {}", quoted(&value.to_string_lossy())));
    }
    if let Some(value) = &settings.ram.log_format {
        lines.push(format!("log-format: {}", quoted(value)));
    }
    lines.push(format!(
        "log-file: {}",
        quoted(&settings.ram.log_path.to_string_lossy())
    ));
    if let Some(value) = &settings.ram.tls_cert {
        lines.push(format!("tls-cert: {}", quoted(&value.to_string_lossy())));
    }
    if let Some(value) = &settings.ram.tls_key {
        lines.push(format!("tls-key: {}", quoted(&value.to_string_lossy())));
    }

    let path = PathBuf::from(RAM_GENERATED_CONFIG);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = temporary_config_path(&path)?;
    let content = format!("{}\n", lines.join("\n"));
    let result = write_private_file(&temporary, content.as_bytes()).and_then(|()| {
        fs::rename(&temporary, &path)?;
        Ok(())
    });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(())
}

fn temporary_config_path(path: &Path) -> std::io::Result<PathBuf> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "generated ram config path has no parent directory",
        )
    })?;
    Ok(parent.join(format!(".ram.generated.{}.yaml.tmp", uuid::Uuid::new_v4())))
}

fn write_private_file(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(content)?;
    file.sync_all()
}

fn command_line(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_command_args(args: &[String]) -> Vec<String> {
    // 启动命令会展示在管理台；--auth 后面包含密码，必须先脱敏。
    let mut output = Vec::with_capacity(args.len());
    let mut redact_next_auth = false;

    for arg in args {
        if redact_next_auth {
            output.push(redact_auth_rule(arg));
            redact_next_auth = false;
            continue;
        }
        redact_next_auth = arg == "--auth";
        output.push(arg.clone());
    }

    output
}

fn shell_quote(value: &str) -> String {
    let is_safe =
        |ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '/' | ':');
    if value.chars().all(is_safe) {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

/// 返回脱敏后的 ram 启动命令。
pub async fn ram_command(state: &AppState) -> AppResult<RamCommandResponse> {
    let command = ram_command_spec(state);
    let redacted_args = redact_command_args(&command.args);
    Ok(RamCommandResponse {
        command_line: command_line(&command.program, &redacted_args),
        program: command.program,
        args: redacted_args,
    })
}

/// 返回 ram 配置快照。
pub async fn ram_config(state: &AppState) -> AppResult<RamConfigResponse> {
    let settings = &state.settings;
    let auth_rules = ram_auth::auth_rules_for_ram(state).await?;
    Ok(RamConfigResponse {
        serve_path: settings.paths.data_dir.to_string_lossy().to_string(),
        bind: settings.ram.bind.clone(),
        port: settings.ram.port,
        path_prefix: settings.ram.path_prefix.clone(),
        local_url: ram_base_url(state),
        health_url: ram_health_url(state),
        log_path: settings.ram.log_path.to_string_lossy().to_string(),
        process_log_path: settings.ram.process_log_path.to_string_lossy().to_string(),
        hidden: settings.ram.hidden.clone(),
        auth_rules: auth_rules
            .iter()
            .map(|rule| redact_auth_rule(rule))
            .collect(),
        auth_method: settings.ram.auth_method.clone(),
        management_auth_configured: ram_auth::management_auth_pair(state).await?.is_some(),
        features: RamFeatures {
            allow_all: settings.ram.allow_all,
            allow_upload: settings.ram.allow_upload,
            allow_delete: settings.ram.allow_delete,
            allow_search: settings.ram.allow_search,
            allow_symlink: settings.ram.allow_symlink,
            allow_archive: settings.ram.allow_archive,
            allow_hash: settings.ram.allow_hash,
            enable_cors: settings.ram.enable_cors,
            render_index: settings.ram.render_index,
            render_try_index: settings.ram.render_try_index,
            render_spa: settings.ram.render_spa,
            compress: settings.ram.compress.clone(),
            assets: settings
                .ram
                .assets
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            tls_enabled: settings.ram.tls_cert.is_some() && settings.ram.tls_key.is_some(),
        },
    })
}

/// 对 ram 认证规则字符串进行脱敏处理（隐藏密码部分）。
///
/// # 什么是脱敏（Redaction）？
/// 脱敏是指将敏感信息（如密码、Token）替换为占位符（如 `***`），
/// 使日志、界面展示、API 响应中不出现明文密码。
/// 即使攻击者能看到这些输出，也无法获取真实凭据。
///
/// # ram 认证规则格式
/// ram 的 `--auth` 参数格式为：`用户名:密码@路径规则`
/// 例如：`alice:secret123@/:rw|/readonly:r`
///
/// 脱敏后变为：`alice:***@/:rw|/readonly:r`
///
/// # 为什么用 `rsplit_once('@')` 而不是 `split_once('@')`？
/// 密码本身可能包含 `@` 字符（例如 `alice:p@ss@/:rw`）。
/// `rsplit_once` 从字符串末尾开始查找，找到最后一个 `@`，
/// 从而把"账号部分"和"路径规则部分"正确分开，不会被密码中的 `@` 干扰。
pub(crate) fn redact_auth_rule(rule: &str) -> String {
    // rsplit_once 从末尾找 @，保证密码中包含 @ 时仍能正确拆分。
    let Some((account, paths)) = rule.rsplit_once('@') else {
        // 没有 @ 说明格式不符合预期，原样返回（不含密码）
        return rule.to_string();
    };
    if account.is_empty() {
        return format!("@{paths}");
    }
    let Some((user, _)) = account.split_once(':') else {
        // 有 @ 但没有 : 分隔用户名和密码，整个账号部分用 *** 替换
        return format!("***@{paths}");
    };
    // 保留用户名，把密码替换为 ***
    format!("{user}:***@{paths}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app_config::{LocalConfig, Settings},
        database,
        state::AppState,
    };

    fn test_state() -> AppState {
        AppState::new(
            Settings::default(),
            database::disconnected_pool().expect("disconnected pool"),
            "$2b$12$C6UzMDM.H6dfI/f/IKcEe.4n3W4O4L2hS2T/1B1Q6VYF2M9mV0X5K".to_string(),
            LocalConfig {
                database_url: String::new(),
                admin_username: "admin".to_string(),
                admin_password_hash: "unused".to_string(),
            },
        )
    }

    #[test]
    fn redacts_passwords_that_contain_at_signs() {
        assert_eq!(redact_auth_rule("alice:p@ss@/:rw"), "alice:***@/:rw");
        assert_eq!(redact_auth_rule("@/:ro"), "@/:ro");
    }

    #[test]
    fn detects_weak_or_malformed_credentials() {
        assert!(weak_auth_rule("alice:short@/:rw"));
        assert!(weak_auth_rule("malformed"));
        assert!(!weak_auth_rule("alice:long-secure-password@/:rw"));
    }

    #[tokio::test]
    async fn command_preview_does_not_require_database() {
        let response = ram_command(&test_state())
            .await
            .expect("command preview should not query database");

        assert_eq!(response.program, "ram");
        assert_eq!(
            response.args[..2],
            ["--config".to_string(), RAM_GENERATED_CONFIG.to_string()]
        );
    }
}
