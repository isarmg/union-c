//! ram 的简易日志实现。
//!
//! 这里实现 `log::Log`，让代码中的 `info!`、`error!` 等宏可以输出到：
//! - 标准输出/标准错误；
//! - 或 `--log-file` 指定的日志文件。

use anyhow::{Context, Result};
use log::{Level, LevelFilter, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

struct SimpleLogger {
    /// 如果指定了日志文件，就把文件句柄放进 Mutex，保证多任务写日志时不会交错写坏。
    file: Option<Mutex<File>>,
}

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let text = record.args().to_string();
            match &self.file {
                Some(file) => {
                    // 写文件时加锁，避免并发请求同时写入同一文件造成内容混杂。
                    if let Ok(mut file) = file.lock() {
                        let _ = writeln!(file, "{text}");
                    }
                }
                None => {
                    // 没有日志文件时，错误走 stderr，普通信息走 stdout。
                    if record.level() < Level::Info {
                        eprintln!("{text}");
                    } else {
                        println!("{text}");
                    }
                }
            }
        }
    }

    fn flush(&self) {}
}

/// 初始化全局日志器。
///
/// `log_file` 为 None 时输出到终端；有路径时追加写入文件。
pub fn init(log_file: Option<PathBuf>) -> Result<()> {
    // 程序启动时调用一次，注册全局 logger。
    let file = match log_file {
        None => None,
        Some(log_file) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
                .with_context(|| {
                    format!("Failed to open the log file at '{}'", log_file.display())
                })?;
            Some(Mutex::new(file))
        }
    };
    let logger = SimpleLogger { file };
    log::set_boxed_logger(Box::new(logger))
        .map(|_| log::set_max_level(LevelFilter::Info))
        .with_context(|| "Failed to init logger")?;
    Ok(())
}
