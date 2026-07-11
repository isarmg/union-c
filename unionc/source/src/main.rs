//! unionc 后端入口。
//!
//! 程序启动顺序：
//! 1. 初始化日志；
//! 2. 读取 PostgreSQL 启动连接串并创建基础目录；
//! 3. 连接数据库、执行迁移、读取 PostgreSQL 中的运行配置；
//! 4. 构造共享状态和路由；
//! 5. 绑定端口并启动 Axum HTTP 服务。

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use unionc::{routes, startup};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化统一日志。EnvFilter 允许通过 RUST_LOG 覆盖日志级别。
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "unionc=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let initialized = startup::initialize().await?;
    let app = routes::router(initialized.state);

    tracing::info!("unionc listening on http://{}", initialized.addr);
    let listener = tokio::net::TcpListener::bind(initialized.addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!("failed to install Ctrl+C handler: {err}");
        }
    };

    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => tracing::error!("failed to install SIGTERM handler: {err}"),
        }
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received; draining HTTP connections");
}
