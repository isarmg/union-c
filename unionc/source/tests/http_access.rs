use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use unionc::{
    app_config::{LocalConfig, Settings},
    database, routes,
    state::{AppState, LocalSession},
};

fn test_state() -> AppState {
    test_state_with_settings(Settings::default())
}

fn test_state_with_settings(settings: Settings) -> AppState {
    AppState::new(
        settings,
        database::disconnected_pool().expect("disconnected pool"),
        "$2b$12$C6UzMDM.H6dfI/f/IKcEe.4n3W4O4L2hS2T/1B1Q6VYF2M9mV0X5K".to_string(),
        LocalConfig {
            database_url: String::new(),
            admin_username: "admin".to_string(),
            admin_password_hash: "unused".to_string(),
        },
    )
}

async fn insert_expired_session(state: &AppState, token: &str) {
    state.auth.sessions.write().await.insert(
        token.to_string(),
        LocalSession {
            username: "admin".to_string(),
            expires_at: chrono::Utc::now() - chrono::Duration::minutes(5),
        },
    );
}

async fn insert_session(state: &AppState, token: &str) {
    state.auth.sessions.write().await.insert(
        token.to_string(),
        LocalSession {
            username: "admin".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        },
    );
}

#[tokio::test]
async fn health_is_public_but_current_user_requires_authentication() {
    let app = routes::router(test_state());
    let health = app
        .clone()
        .oneshot(Request::get("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);

    let current_user = app
        .oneshot(Request::get("/api/auth/me").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(current_user.status(), StatusCode::UNAUTHORIZED);
    let payload: serde_json::Value =
        serde_json::from_slice(&to_bytes(current_user.into_body(), 64 * 1024).await.unwrap())
            .unwrap();
    assert_eq!(payload["code"], "unauthorized");
}

#[tokio::test]
async fn cookie_authenticated_mutation_requires_csrf_header() {
    let state = test_state();
    insert_session(&state, "test-session").await;
    let response = routes::router(state)
        .oneshot(
            Request::post("/api/auth/logout")
                .header("cookie", "session=test-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let payload: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(payload["code"], "forbidden");
}

#[tokio::test]
async fn cookie_authenticated_mutation_allows_csrf_header() {
    let state = test_state();
    insert_session(&state, "test-session").await;
    let response = routes::router(state)
        .oneshot(
            Request::post("/api/auth/logout")
                .header("cookie", "session=test-session")
                .header("x-csrf-token", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn expired_session_is_rejected_and_pruned() {
    let state = test_state();
    insert_expired_session(&state, "expired-session").await;
    let response = routes::router(state.clone())
        .oneshot(
            Request::get("/api/auth/me")
                .header("cookie", "session=expired-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(state.auth.sessions.read().await.is_empty());
}

#[tokio::test]
async fn production_login_requires_https_reverse_proxy_header() {
    let settings = Settings {
        production: true,
        ..Settings::default()
    };
    let response = routes::router(test_state_with_settings(settings))
        .oneshot(
            Request::post("/api/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"username":"admin","password":"irrelevant"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let payload: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(payload["code"], "forbidden");
}

#[tokio::test]
async fn business_route_reports_stable_database_unavailable_code() {
    let state = test_state();
    insert_session(&state, "test-session").await;
    let response = routes::router(state)
        .oneshot(
            Request::get("/api/services/sunshine/hosts")
                .header("cookie", "session=test-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let payload: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(payload["code"], "database_unavailable");
}

#[tokio::test]
async fn settings_database_route_remains_available_without_database() {
    let state = test_state();
    insert_session(&state, "test-session").await;
    let response = routes::router(state)
        .oneshot(
            Request::get("/api/settings/database")
                .header("cookie", "session=test-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(payload["connected"], false);
    assert_eq!(payload["restart_required"], false);
}

#[tokio::test]
async fn host_session_cookie_is_preferred_in_full_router() {
    let state = test_state();
    insert_session(&state, "secure-session").await;
    let response = routes::router(state)
        .oneshot(
            Request::get("/api/auth/me")
                .header(
                    "cookie",
                    "session=stale-session; __Host-session=secure-session",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let payload: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(payload["username"], "admin");
}

#[tokio::test]
async fn sse_ticket_reports_database_unavailable_during_bootstrap() {
    let state = test_state();
    insert_session(&state, "test-session").await;
    let response = routes::router(state)
        .oneshot(
            Request::post("/api/events/ticket")
                .header("cookie", "session=test-session")
                .header("x-csrf-token", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let payload: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap();
    assert_eq!(payload["code"], "database_unavailable");
}

#[tokio::test]
async fn agent_registration_requires_bootstrap_bearer_token() {
    let mut settings = Settings::default();
    settings.agents.enrollment_token =
        "test-enrollment-token-with-at-least-32-characters".to_string();
    let response = routes::router(test_state_with_settings(settings))
        .oneshot(
            Request::post("/api/agent/v1/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","name":"test","os":"linux","os_version":null,"kernel_version":null,"arch":"x86_64","agent_version":"0.1.0","enrollment_secret":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn monitoring_routes_remain_console_authenticated() {
    let response = routes::router(test_state())
        .oneshot(
            Request::get("/api/monitoring/hosts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
