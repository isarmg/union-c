//! 本机 RAM 子进程生命周期管理。

use std::{
    fs::{self, OpenOptions},
    process::{ExitStatus, Stdio},
    time::Duration,
};

use chrono::Utc;
use tokio::{process::Command, time::timeout};

use crate::{
    database,
    domain::{ActionResponse, ServiceStatus},
    error::{AppError, AppResult},
    state::{AppState, ManagedProcess},
};

use super::{
    client::{ram_base_url, ram_health},
    config::ram_command_spec,
};

/// 启动由union托管的 ram 子进程。
///
/// 整体步骤：
/// 1. 检查union是否已经托管了一个 ram 子进程，避免重复启动；
/// 2. 若端口已被外部进程占用则安全拒绝；
/// 3. 生成仅引用私有配置文件的启动命令；
/// 4. spawn 子进程并记录到 AppState；
/// 5. 写入服务事件和审计日志。
pub async fn start_ram(state: &AppState) -> AppResult<ActionResponse> {
    let _operation = state.ram.operation.lock().await;
    start_ram_locked(state).await
}

async fn start_ram_locked(state: &AppState) -> AppResult<ActionResponse> {
    // ── 步骤 1：检查是否已有托管进程 ─────────────────────────────────────────
    // 用大括号限定 guard 的生命周期：大括号结束时 guard 自动释放锁。
    // 如果不这样做，guard 会一直持有锁直到函数结束，导致后面的异步操作（网络请求等）
    // 也需要持锁，极易造成死锁或长时间阻塞其他请求。
    {
        let mut guard = state.ram.child.lock().await;

        // `try_wait()` 是"非阻塞"检查子进程状态：
        //   - 返回 Ok(None)  → 子进程还在运行（还没退出）
        //   - 返回 Ok(Some(status)) → 子进程已经退出，status 是退出码
        //   - 返回 Err → 系统调用失败
        // 之所以用 try_wait 而不是 wait，是因为 wait 会阻塞直到进程退出，
        // 而这里只需要"瞬间判断"当前状态，不想等待。
        if let Some(process) = guard.as_mut() {
            if process.child.try_wait()?.is_none() {
                // 进程还活着，拒绝重复启动
                return Err(AppError::Conflict("ram is already running".to_string()));
            }
            // 进程已退出但槽位还残留着旧句柄，清理掉
            *guard = None;
        }
    } // ← guard 在此处释放锁，后续代码不再持锁

    if ram_health(state).await.reachable {
        return Err(AppError::Conflict(
            "ram is already reachable on the configured port; refusing to terminate an unmanaged process"
                .to_string(),
        ));
    }

    // ── 步骤 3：准备日志文件 ──────────────────────────────────────────────────
    // ram 子进程的 stdout 和 stderr 都重定向到同一个日志文件，
    // 这样运维人员只需看一个文件就能看到所有输出。
    // `try_clone()` 是系统层面复制文件句柄（类似 Unix 的 dup），
    // 使 stdout 和 stderr 可以各自持有一个句柄，但实际写入同一个文件。
    if let Some(parent) = state.settings.ram.process_log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let stdout = OpenOptions::new()
        .create(true) // 文件不存在时创建
        .append(true) // 追加写入，不清空已有日志
        .open(&state.settings.ram.process_log_path)?;
    let stderr = stdout.try_clone()?; // 复制文件句柄，stdout 和 stderr 写同一个文件

    // ── 步骤 4：spawn 子进程 ──────────────────────────────────────────────────
    // `Command::new` + `spawn()` 是 Tokio 异步版本的子进程启动。
    // `kill_on_drop(false)` 表示：当 Rust 这边的 `Child` 句柄被 drop 时，
    // 不自动发 SIGKILL 杀掉子进程。设为 false 是因为union重启后
    // 仍希望 ram 继续运行，不受union自身生命周期影响。
    let command_spec = ram_command_spec(state).await?;
    let mut command = Command::new(&command_spec.program);
    command
        .args(&command_spec.args)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .kill_on_drop(false); // ram 的生命周期独立于union进程

    let child = command.spawn().map_err(|err| {
        AppError::Process(format!(
            "failed to start ram with command '{}': {err}",
            command_spec.program
        ))
    })?;
    let pid = child.id();

    // ── 步骤 5：记录到 AppState ───────────────────────────────────────────────
    // 重新获取锁，把新启动的子进程句柄存入槽位。
    // `drop(guard)` 提前释放锁，让后续的 `ram_status` 调用可以再次获取锁。
    let mut guard = state.ram.child.lock().await;
    *guard = Some(ManagedProcess {
        child,
        pid,
        started_at: Utc::now(),
    });
    drop(guard); // 显式释放，避免锁一直持有到函数末尾

    database::set_service_desired_state(state.db().as_ref(), "ram", "running").await?;
    database::service_event(
        state.db().as_ref(),
        "ram",
        "start",
        Some("started from union"),
    )
    .await?;
    database::insert_audit(
        state.db().as_ref(),
        "ram.start",
        "ram",
        Some("process spawned"),
    )
    .await?;

    let service = ram_status(state).await?;
    Ok(ActionResponse {
        ok: true,
        message: "ram started".to_string(),
        service: Some(service),
    })
}

/// 停止 ram。
///
/// 只停止union实际持有句柄的子进程；绝不按名称扫描或终止外部进程。
pub async fn stop_ram(state: &AppState) -> AppResult<ActionResponse> {
    let _operation = state.ram.operation.lock().await;
    stop_ram_locked(state).await
}

async fn stop_ram_locked(state: &AppState) -> AppResult<ActionResponse> {
    // 先取出托管进程句柄，释放锁后再等待退出，减少阻塞其他状态读取。
    let managed_process = {
        let mut guard = state.ram.child.lock().await;
        guard.take()
    };

    let mut stopped = false;
    if let Some(mut process) = managed_process
        && process.child.try_wait()?.is_none()
    {
        terminate_managed_process(&mut process).await?;
        stopped = true;
    }

    if !stopped && ram_health(state).await.reachable {
        return Err(AppError::Conflict(
            "an unmanaged ram process is reachable; refusing to terminate it".to_string(),
        ));
    }

    database::set_service_desired_state(state.db().as_ref(), "ram", "stopped").await?;
    // 只在确实停止了进程时写事件和审计，避免产生"什么都没发生"的误导记录。
    if stopped {
        database::service_event(
            state.db().as_ref(),
            "ram",
            "stop",
            Some("stopped from union"),
        )
        .await?;
        database::insert_audit(
            state.db().as_ref(),
            "ram.stop",
            "ram",
            Some("process stopped"),
        )
        .await?;
    }

    Ok(ActionResponse {
        ok: true,
        message: if stopped {
            "ram stopped".to_string()
        } else {
            "ram was not running".to_string()
        },
        service: Some(ram_status(state).await?),
    })
}

/// 重新加载 ram（热重载认证配置）。
///
/// “热重载”在这里的含义：当用户在union修改了 ram 的账号/密码之后，
/// 不需要用户手动操作，union会自动把新配置应用到 ram。
///
/// 实现方式：ram 本身没有”重新读取配置文件”的信号机制（如 SIGHUP），
/// 所以只能通过”停止旧进程 → 用新参数重新启动”来实现配置更新。
/// 从用户角度看这是”无感知的重载”，实际上是一次短暂的重启（毫秒级）。
///
/// 返回值：`Ok(true)` 表示确实停止并重启了，`Ok(false)` 表示 ram 本来就没在运行。
pub async fn reload_managed_ram(state: &AppState) -> AppResult<bool> {
    let _operation = state.ram.operation.lock().await;
    // ram 没有独立热加载账号的接口，这里通过停止并重新启动托管实例应用新配置。
    let managed_process = {
        let mut guard = state.ram.child.lock().await;
        guard.take()
    };

    let mut stopped = false;
    if let Some(mut process) = managed_process
        && process.child.try_wait()?.is_none()
    {
        terminate_managed_process(&mut process).await?;
        stopped = true;
    }

    if !stopped {
        if ram_health(state).await.reachable {
            return Err(AppError::Conflict(
                "an unmanaged ram process is reachable; refusing to reload it".to_string(),
            ));
        }
        return Ok(false);
    }

    database::service_event(
        state.db().as_ref(),
        "ram",
        "reload-auth",
        Some("reloaded after auth update"),
    )
    .await?;
    database::insert_audit(
        state.db().as_ref(),
        "ram.auth.reload",
        "ram",
        Some("process reloaded after auth update"),
    )
    .await?;

    start_ram_locked(state).await?;
    Ok(true)
}

async fn terminate_managed_process(process: &mut ManagedProcess) -> std::io::Result<()> {
    let graceful_requested = if let Some(pid) = process.pid {
        Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|status| status.success())
    } else {
        false
    };
    if !graceful_requested {
        process.child.start_kill()?;
    }
    if timeout(Duration::from_secs(5), process.child.wait())
        .await
        .is_err()
    {
        process.child.start_kill()?;
        let _ = process.child.wait().await;
    }
    Ok(())
}

/// 在同一操作锁内完成重启，避免 stop/start 之间插入另一个请求。
pub async fn restart_ram(state: &AppState) -> AppResult<ActionResponse> {
    let _operation = state.ram.operation.lock().await;
    stop_ram_locked(state).await?;
    start_ram_locked(state).await
}

/// 获取 ram 当前状态。
pub async fn ram_status(state: &AppState) -> AppResult<ServiceStatus> {
    // 只在持有锁期间做同步操作，不在锁内做异步 I/O：
    // ram_health 的 TCP 连接最长可阻塞 9 秒，在锁内调用会阻塞所有并发的启动/停止操作。
    enum Snapshot {
        Running {
            pid: Option<u32>,
            started_at: chrono::DateTime<Utc>,
        },
        Exited(ExitStatus),
        NotRunning,
    }

    let snapshot = {
        let mut guard = state.ram.child.lock().await;
        if let Some(process) = guard.as_mut() {
            match process.child.try_wait()? {
                Some(status) => {
                    *guard = None;
                    Snapshot::Exited(status)
                }
                None => Snapshot::Running {
                    pid: process.pid,
                    started_at: process.started_at,
                },
            }
        } else {
            Snapshot::NotRunning
        }
        // guard 在此处释放，后续 ram_health 不持锁。
    };

    // 健康检查只调用一次，在锁已释放之后。
    let health = ram_health(state).await;

    let (runtime_state, healthy, pid, message) = match snapshot {
        Snapshot::Running { pid, started_at } => (
            "running".to_string(),
            health.reachable,
            pid,
            if health.reachable {
                format!("running since {}", started_at.to_rfc3339())
            } else {
                format!(
                    "process is running but ram health check failed: {}",
                    health.message
                )
            },
        ),
        Snapshot::Exited(status) if !health.reachable => (
            "stopped".to_string(),
            false,
            None,
            format!("last process exited with {status}"),
        ),
        Snapshot::NotRunning if !health.reachable => (
            "stopped".to_string(),
            false,
            None,
            "not running".to_string(),
        ),
        // 进程已退出或无托管句柄，但端口仍可达：有外部启动的 ram 实例。
        _ => (
            "external".to_string(),
            true,
            None,
            "ram health endpoint is reachable; process is external to this union".to_string(),
        ),
    };

    Ok(ServiceStatus {
        name: "ram".to_string(),
        kind: "file-service".to_string(),
        runtime_state,
        healthy,
        address: Some(ram_base_url(state)),
        pid,
        message,
        updated_at: Utc::now().to_rfc3339(),
    })
}
