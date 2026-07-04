//! 服务管理门面。
//!
//! 对调用方保持稳定 API，内部按进程生命周期、配置、协议客户端、状态和日志拆分。

mod client;
mod config;
mod lifecycle;
mod logs;
mod status;

pub use client::{ram_entry, ram_health};
pub(crate) use config::redact_auth_rule;
pub use config::{ram_command, ram_config};
pub use lifecycle::{reload_managed_ram, restart_ram, start_ram, stop_ram};
pub use logs::tail_lines;
pub use status::{all_services, sunshine_host_status};
