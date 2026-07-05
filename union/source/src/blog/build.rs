//! Astro 静态站构建调度。

use super::{storage::*, *};

/// 在后台触发一次博客构建，构建成功后向 SSE 通道广播通知。
///
/// 构建进行中时只设置 dirty；当前构建结束后会再跑一轮，确保更新不会丢失。
pub async fn trigger_background_build(state: AppState) {
    let mut schedule = state.blog.build.lock().await;
    schedule.dirty = true;
    if schedule.running {
        return;
    }
    schedule.running = true;
    drop(schedule);

    tokio::spawn(async move {
        loop {
            state.blog.build.lock().await.dirty = false;
            match build_blog(&state).await {
                Ok(result) if result.success => {
                    tracing::info!("background blog build completed successfully");
                }
                Ok(_) => tracing::warn!("background blog build exited unsuccessfully"),
                Err(err) => tracing::warn!("background blog build failed: {err}"),
            }

            let mut schedule = state.blog.build.lock().await;
            if schedule.dirty {
                drop(schedule);
                continue;
            }
            schedule.running = false;
            break;
        }
    });
}

/// 运行博客静态构建命令。所有入口都在这里取得信号量，手动与后台构建不会并发。
pub async fn build_blog(state: &AppState) -> AppResult<BlogBuildResponse> {
    let _permit = BUILD_SEMAPHORE
        .acquire()
        .await
        .map_err(|_| AppError::Process("blog build scheduler is closed".to_string()))?;
    build_blog_once(state).await
}

async fn build_blog_once(state: &AppState) -> AppResult<BlogBuildResponse> {
    let settings = &state.settings.blog;

    if !settings.work_dir.exists() {
        return Err(AppError::BadRequest(format!(
            "blog work directory does not exist: {}",
            settings.work_dir.display()
        )));
    }

    ensure_blog_seeded(state).await?;
    let content_guard = state.blog.content_lock.lock().await;
    export_blog_content(state).await?;
    drop(content_guard);

    let job_id = Uuid::new_v4().to_string();
    database::create_job(state.db().as_ref(), &job_id, "blog_build").await?;
    let log_path = settings
        .build_log_dir
        .join(format!("blog-build-{job_id}.log"));

    let started = Instant::now();
    let mut command = Command::new(&settings.build_command);
    command
        .args(&settings.build_args)
        .current_dir(&settings.work_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // timeout 会 drop Child；显式开启后可保证超时不留下孤儿构建进程。
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let message = format!(
                "failed to run blog build command '{}': {err}",
                settings.build_command
            );
            let duration_ms = started.elapsed().as_millis() as i64;
            atomic_write_file(&log_path, format!("{message}\n").as_bytes())?;
            database::finish_job(
                state.db().as_ref(),
                &job_id,
                "failed",
                None,
                duration_ms,
                Some(&log_path.to_string_lossy()),
            )
            .await?;
            return Err(AppError::Process(message));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Process("blog build stdout pipe is unavailable".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Process("blog build stderr pipe is unavailable".to_string()))?;
    let stdout_task = tokio::spawn(drain_capped(stdout, MAX_BUILD_OUTPUT_BYTES));
    let stderr_task = tokio::spawn(drain_capped(stderr, MAX_BUILD_OUTPUT_BYTES));
    let status = match tokio::time::timeout(std::time::Duration::from_secs(600), child.wait()).await
    {
        Ok(Ok(status)) => status,
        result => {
            let message = match result {
                Err(_) => {
                    let _ = child.kill().await;
                    format!(
                        "blog build command '{}' timed out after 600s",
                        settings.build_command
                    )
                }
                Ok(Err(err)) => format!(
                    "failed to run blog build command '{}': {err}",
                    settings.build_command
                ),
                Ok(Ok(_)) => unreachable!(),
            };
            let duration_ms = started.elapsed().as_millis() as i64;
            atomic_write_file(&log_path, format!("{message}\n").as_bytes())?;
            database::finish_job(
                state.db().as_ref(),
                &job_id,
                "failed",
                None,
                duration_ms,
                Some(&log_path.to_string_lossy()),
            )
            .await?;
            let _ = database::insert_audit(
                state.db().as_ref(),
                "blog.build",
                "blog",
                Some(&format!("success=false error={message}")),
            )
            .await;
            return Err(AppError::Process(message));
        }
    };

    let stdout = stdout_task
        .await
        .map_err(|err| AppError::Anyhow(anyhow::anyhow!("stdout task failed: {err}")))??;
    let stderr = stderr_task
        .await
        .map_err(|err| AppError::Anyhow(anyhow::anyhow!("stderr task failed: {err}")))??;
    let duration_ms = started.elapsed().as_millis() as u64;
    let success = status.success();
    let exit_code = status.code();
    let mut combined = String::with_capacity(stdout.len() + stderr.len() + 128);
    combined.push_str(&String::from_utf8_lossy(&stdout));
    combined.push_str(&String::from_utf8_lossy(&stderr));

    atomic_write_file(&log_path, combined.as_bytes())?;

    database::finish_job(
        state.db().as_ref(),
        &job_id,
        if success { "succeeded" } else { "failed" },
        exit_code,
        duration_ms as i64,
        Some(&log_path.to_string_lossy()),
    )
    .await?;
    database::insert_audit(
        state.db().as_ref(),
        "blog.build",
        "blog",
        Some(&format!(
            "command={} args={:?} success={success}",
            settings.build_command, settings.build_args
        )),
    )
    .await?;

    Ok(BlogBuildResponse {
        job_id,
        success,
        exit_code,
        duration_ms,
        log_path: log_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("")
            .to_string(),
        log_tail: tail_lines(&log_path, 80)?,
        adopted_as_drafts: 0,
    })
}

/// 持续排空子进程输出以避免管道阻塞，但只保留有限字节，防止异常构建耗尽内存。
async fn drain_capped<R>(mut reader: R, limit: usize) -> std::io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut retained = Vec::with_capacity(limit.min(64 * 1024));
    let mut chunk = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        retained.extend_from_slice(&chunk[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    if truncated {
        retained.extend_from_slice(b"\n[output truncated]\n");
    }
    Ok(retained)
}
