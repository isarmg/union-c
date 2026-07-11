//! UnionC 精简控制台后端库。
//!
//! 二进制入口只负责日志、监听和关停；应用模块放在库中，便于集成测试复用。

#[cfg(not(target_os = "linux"))]
compile_error!("unionc supports Linux only");

pub mod app_config;
pub mod database;
pub mod domain;
pub mod error;
mod http_client;
mod network;
pub mod routes;
mod secrets;
mod service_manager;
pub mod startup;
pub mod state;
mod sunshine;
mod system;
mod wol;
