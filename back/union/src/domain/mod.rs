//! HTTP API 请求与响应模型。
//!
//! 按业务域拆分后在此统一重导出，调用方继续通过 `crate::domain::*` 使用，避免
//! HTTP 合同的物理组织泄漏到业务代码。

mod auth;
mod blog;
mod proxmox;
mod ram;
mod sunshine;
mod system;

pub use auth::*;
pub use blog::*;
pub use proxmox::*;
pub use ram::*;
pub use sunshine::*;
pub use system::*;
