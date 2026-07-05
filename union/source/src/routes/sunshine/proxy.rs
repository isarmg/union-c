//! Sunshine Web API 代理端点。

use super::{common::*, *};

// ─── Sunshine API 代理 ────────────────────────────────────────────────────────
// 以下 handler 都是简单的代理：找到主机 → 调用 sunshine 模块的对应函数 → 返回结果。
// 所有实际的 HTTP 通信逻辑都封装在 `sunshine.rs` 中。

pub(super) async fn apps_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        sunshine::apps_list(&find_host(&state, &id).await?).await?,
    ))
}

pub(super) async fn apps_save(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    validate_proxy_json_object("Sunshine app payload", &body, 256 * 1024)?;
    let detail = body
        .get("name")
        .and_then(Value::as_str)
        .map(|name| format!("name={}", name.trim()))
        .unwrap_or_else(|| "app payload saved".to_string());
    let host = find_host(&state, &id).await?;
    let response = sunshine::apps_save(&host, body).await?;
    audit(&state, "sunshine.app.save", &id, Some(&detail)).await?;
    Ok(Json(response))
}

pub(super) async fn apps_close(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let response = sunshine::apps_close(&find_host(&state, &id).await?).await?;
    audit(&state, "sunshine.app.close", &id, None).await?;
    Ok(Json(response))
}

pub(super) async fn apps_delete(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, u32)>,
) -> AppResult<Json<Value>> {
    validate_index(index)?;
    // `Path<(String, u32)>` 提取两个路径参数 `/hosts/{id}/apps/{index}`
    let response = sunshine::apps_delete(&find_host(&state, &id).await?, index).await?;
    audit(
        &state,
        "sunshine.app.delete",
        &id,
        Some(&format!("index={index}")),
    )
    .await?;
    Ok(Json(response))
}

pub(super) async fn clients_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        sunshine::clients_list(&find_host(&state, &id).await?).await?,
    ))
}

pub(super) async fn clients_unpair(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<SunshineUnpairRequest>,
) -> AppResult<Json<Value>> {
    let client_id = validate_client_id(&p.uuid)?;
    let response = sunshine::clients_unpair(&find_host(&state, &id).await?, client_id).await?;
    audit(
        &state,
        "sunshine.client.unpair",
        &id,
        Some(&format!("client={client_id}")),
    )
    .await?;
    Ok(Json(response))
}

pub(super) async fn clients_unpair_all(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let response = sunshine::clients_unpair_all(&find_host(&state, &id).await?).await?;
    audit(&state, "sunshine.client.unpair_all", &id, None).await?;
    Ok(Json(response))
}

pub(super) async fn clients_update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<SunshineClientUpdateRequest>,
) -> AppResult<Json<Value>> {
    let client_id = validate_client_id(&p.uuid)?;
    let response =
        sunshine::clients_update(&find_host(&state, &id).await?, client_id, p.enabled).await?;
    audit(
        &state,
        "sunshine.client.update",
        &id,
        Some(&format!("client={client_id} enabled={}", p.enabled)),
    )
    .await?;
    Ok(Json(response))
}

pub(super) async fn config_get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        sunshine::config_get(&find_host(&state, &id).await?).await?,
    ))
}

pub(super) async fn config_save(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    validate_proxy_json_object("Sunshine config payload", &body, 1024 * 1024)?;
    let response = sunshine::config_save(&find_host(&state, &id).await?, body).await?;
    audit(&state, "sunshine.config.save", &id, Some("config updated")).await?;
    Ok(Json(response))
}

pub(super) async fn config_locale(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        sunshine::config_locale(&find_host(&state, &id).await?).await?,
    ))
}

pub(super) async fn api_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    Ok(Json(
        sunshine::api_logs(&find_host(&state, &id).await?).await?,
    ))
}

pub(super) async fn pin(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<SunshinePinRequest>,
) -> AppResult<Json<Value>> {
    let (pin, name) = validate_pin_request(&p.pin, &p.name)?;
    let response = sunshine::pin_pair(&find_host(&state, &id).await?, &pin, &name).await?;
    audit(
        &state,
        "sunshine.client.pair",
        &id,
        Some(&format!("name={name}")),
    )
    .await?;
    Ok(Json(response))
}

pub(super) async fn password(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> AppResult<Json<Value>> {
    validate_proxy_json_object("Sunshine password payload", &body, 64 * 1024)?;
    let response = sunshine::password_update(&find_host(&state, &id).await?, body).await?;
    audit(
        &state,
        "sunshine.password.update",
        &id,
        Some("password updated"),
    )
    .await?;
    Ok(Json(response))
}

pub(super) async fn restart(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let response = sunshine::restart(&find_host(&state, &id).await?).await?;
    audit(&state, "sunshine.system.restart", &id, None).await?;
    Ok(Json(response))
}

pub(super) async fn reset_display(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let response = sunshine::reset_display_device(&find_host(&state, &id).await?).await?;
    audit(&state, "sunshine.display.reset", &id, None).await?;
    Ok(Json(response))
}

/// 获取并转发游戏封面图片（二进制响应，需要特殊处理）。
///
/// 与其他接口不同，这个接口返回的是二进制图片数据（非 JSON），
/// 所以需要手动构建响应并设置正确的 Content-Type 头（如 image/jpeg）。
/// `IntoResponse` trait 让 `Vec<u8>` 可以直接转为 HTTP 响应体。
pub(super) async fn cover_get(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, u32)>,
) -> Result<Response, AppError> {
    validate_index(index)?;
    let host = find_host(&state, &id).await?;
    let (content_type, bytes) = sunshine::cover_get(&host, index).await?;
    // 将 content_type 字符串转为 HTTP 头值（HeaderValue），无效时退回 image/jpeg
    let header_val = HeaderValue::from_str(&content_type)
        .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg"));
    let mut resp = bytes.into_response(); // Vec<u8> 转为 HTTP 响应（自动设置 Content-Length）
    resp.headers_mut().insert(header::CONTENT_TYPE, header_val); // 覆盖 Content-Type
    Ok(resp)
}

/// 上传游戏封面图片（通过 URL，让 Sunshine 服务器端下载）。
pub(super) async fn cover_upload(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<SunshineCoverUploadRequest>,
) -> AppResult<Json<Value>> {
    let (key, url) = validate_cover_upload(&p.key, &p.url)?;
    let response = sunshine::cover_upload(&find_host(&state, &id).await?, &key, &url).await?;
    audit(
        &state,
        "sunshine.cover.upload",
        &id,
        Some(&format!("key={key}")),
    )
    .await?;
    Ok(Json(response))
}

fn validate_index(index: u32) -> AppResult<()> {
    if index > 10_000 {
        return Err(AppError::BadRequest(
            "Sunshine app index is out of range".to_string(),
        ));
    }
    Ok(())
}

async fn audit(
    state: &AppState,
    action: &str,
    target: &str,
    detail: Option<&str>,
) -> AppResult<()> {
    database::insert_audit(state.db().as_ref(), action, target, detail).await?;
    Ok(())
}
