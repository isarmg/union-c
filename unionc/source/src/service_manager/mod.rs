//! Sunshine 状态与日志读取门面。

mod logs;
mod status;

pub use logs::tail_lines;
pub use status::{all_services, sunshine_host_status};
