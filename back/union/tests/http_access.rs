use std::collections::HashMap;

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use union::{
    app_config::{LocalConfig, Settings},
    database, routes,
    state::{AppState, LocalSession},
};

fn test_state() -> AppState {
    AppState::new(
        Settings::default(),
        database::disconnected_pool().expect("disconnected pool"),
        "$2b$12$C6UzMDM.H6dfI/f/IKcEe.4n3W4O4L2hS2T/1B1Q6VYF2M9mV0X5K".to_string(),
        LocalConfig {
            database_url: String::new(),
            admin_username: "admin".to_string(),
            admin_password_hash: "unused".to_string(),
        },
    )
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
    state.auth.sessions.write().await.extend(HashMap::from([(
        "test-session".to_string(),
        LocalSession {
            username: "admin".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        },
    )]));
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
async fn business_route_reports_stable_database_unavailable_code() {
    let state = test_state();
    state.auth.sessions.write().await.insert(
        "test-session".to_string(),
        LocalSession {
            username: "admin".to_string(),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        },
    );
    let response = routes::router(state)
        .oneshot(
            Request::get("/api/blog/posts")
                .header("authorization", "Bearer test-session")
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
