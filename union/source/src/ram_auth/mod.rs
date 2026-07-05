//! ram 账号和权限管理。
//!
//! 本模块负责在三种数据形态之间转换：
//! - 数据库中的 `service_accounts/service_permissions` 记录；
//! - 前端表单提交的结构化 JSON；
//! - ram 命令行需要的 `username:password@/path:rw` 文本规则。

use std::collections::{HashMap, HashSet};

use crate::{
    app_config::Settings,
    database::{self, DbPool, ServiceAccountInput, ServiceAccountRecord, ServicePermissionInput},
    domain::{
        RamAuthPath, RamAuthResponse, RamAuthRuleInput, RamAuthRuleResponse, RamAuthUpdateRequest,
        RamAuthUpdateResponse,
    },
    error::{AppError, AppResult},
    ram_instances, service_manager,
    state::AppState,
};

const RAM_SERVICE: &str = "ram";
const STORAGE_LABEL: &str = "postgresql:service_accounts";
const ANONYMOUS_KEY: &str = "__anonymous__";
const MANAGEMENT_KEY: &str = "__management__";

#[derive(Debug, Clone)]
/// 解析后的 ram 认证规则。
///
/// username 为 None 表示匿名规则；password 为 None 通常表示前端保存时沿用旧密码。
struct ParsedAuthRule {
    username: Option<String>,
    password: Option<String>,
    paths: Vec<RamAuthPath>,
}

mod rules;
mod service;

pub use service::{
    auth_rules_for_ram, current_auth, current_auth_for, ensure_seeded, management_auth_pair,
    management_auth_pair_for, update_auth, update_auth_for,
};
