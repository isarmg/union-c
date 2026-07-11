use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{Duration, Utc};
use sqlx_core::query::query;
use tower::ServiceExt;
use unionc::{
    app_config::{LocalConfig, Settings},
    database, routes,
    state::{AppState, LocalSession},
};
use uuid::Uuid;

const BOOTSTRAP_TOKEN: &str = "integration-enrollment-token-with-more-than-32-characters";
const ENROLLMENT_SECRET: &str = "host-private-enrollment-proof-with-more-than-32-characters";

#[tokio::test]
async fn enrollment_report_and_read_only_queries_are_end_to_end() {
    let Ok(url) = std::env::var("UNIONC_TEST_DATABASE_URL") else {
        eprintln!("skipped: UNIONC_TEST_DATABASE_URL is not configured");
        return;
    };
    let mut settings = Settings::default();
    settings.database.url = url;
    settings.agents.enrollment_token = BOOTSTRAP_TOKEN.to_string();
    let pool = database::connect(&settings)
        .await
        .expect("connect test database");
    database::migrate(&pool)
        .await
        .expect("migrate test database");

    let state = AppState::new(
        settings,
        pool.clone(),
        "$2b$12$C6UzMDM.H6dfI/f/IKcEe.4n3W4O4L2hS2T/1B1Q6VYF2M9mV0X5K".into(),
        LocalConfig {
            database_url: String::new(),
            admin_username: "admin".into(),
            admin_password_hash: "unused".into(),
        },
    );
    state.auth.sessions.write().await.insert(
        "monitoring-test-session".into(),
        LocalSession {
            username: "admin".into(),
            expires_at: Utc::now() + Duration::minutes(5),
        },
    );
    let app = routes::router(state);
    let host_id = Uuid::new_v4();

    let registration = serde_json::json!({
        "id": host_id,
        "name": "integration-host",
        "os": "linux",
        "os_version": "test",
        "kernel_version": "test",
        "arch": "x86_64",
        "agent_version": "0.1.0",
        "enrollment_secret": ENROLLMENT_SECRET,
    });
    let first = post_json(
        &app,
        "/api/agent/v1/register",
        BOOTSTRAP_TOKEN,
        &registration,
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = response_json(first).await;
    let first_token = first_body["token"].as_str().expect("first host token");

    let mut takeover = registration.clone();
    takeover["enrollment_secret"] =
        serde_json::Value::String("attacker-proof-with-at-least-thirty-two-characters".into());
    let takeover_response =
        post_json(&app, "/api/agent/v1/register", BOOTSTRAP_TOKEN, &takeover).await;
    assert_eq!(takeover_response.status(), StatusCode::CONFLICT);

    let report_id = Uuid::new_v4();
    let report = sample_report(host_id, report_id);
    let accepted = post_json(&app, "/api/agent/v1/report", first_token, &report).await;
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(accepted).await["accepted"], true);

    // A retry with the same private enrollment proof is safe and rotates the
    // per-host token. A party holding only the fleet bootstrap token cannot do it.
    let second = post_json(
        &app,
        "/api/agent/v1/register",
        BOOTSTRAP_TOKEN,
        &registration,
    )
    .await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_body = response_json(second).await;
    let second_token = second_body["token"].as_str().expect("rotated host token");
    assert_ne!(first_token, second_token);

    let rejected_old_token = post_json(&app, "/api/agent/v1/report", first_token, &report).await;
    assert_eq!(rejected_old_token.status(), StatusCode::UNAUTHORIZED);

    let duplicate = post_json(&app, "/api/agent/v1/report", second_token, &report).await;
    assert_eq!(duplicate.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(duplicate).await["accepted"], false);

    let hosts = app
        .clone()
        .oneshot(
            Request::get("/api/monitoring/hosts")
                .header("cookie", "session=monitoring-test-session")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hosts.status(), StatusCode::OK);
    let hosts_body = response_json(hosts).await;
    let host = hosts_body["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|host| host["id"] == host_id.to_string())
        .expect("registered host in list");
    assert_eq!(host["network_received_bytes_per_second"], 1000.0);
    assert_eq!(host["disk_read_bytes_per_second"], 3000.0);

    for path in [
        format!("/api/monitoring/hosts/{host_id}"),
        format!("/api/monitoring/hosts/{host_id}/history?limit=10"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::get(path)
                    .header("cookie", "session=monitoring-test-session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    query("DELETE FROM monitored_hosts WHERE host_id=$1::uuid")
        .bind(host_id.to_string())
        .execute(&pool)
        .await
        .expect("clean monitoring test host");
}

async fn post_json(
    app: &axum::Router,
    path: &str,
    token: &str,
    value: &serde_json::Value,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::post(path)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(value).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}

fn sample_report(host_id: Uuid, report_id: Uuid) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "report_id": report_id,
        "collected_at": Utc::now(),
        "host": {
            "id": host_id,
            "name": "integration-host",
            "os": "linux",
            "os_version": "test",
            "kernel_version": "test",
            "arch": "x86_64",
            "agent_version": "0.1.0"
        },
        "interval_seconds": 10.0,
        "system": {
            "uptime_seconds": 60,
            "cpu": {
                "usage_percent": 25.0,
                "logical_count": 4,
                "physical_count": 2,
                "per_core_percent": [10.0, 20.0, 30.0, 40.0]
            },
            "memory": {
                "total_bytes": 10000,
                "used_bytes": 5000,
                "available_bytes": 5000,
                "swap_total_bytes": 0,
                "swap_used_bytes": 0
            },
            "networks": [
                {
                    "name": "eth0",
                    "received_bytes_total": 10000,
                    "transmitted_bytes_total": 5000,
                    "received_bytes_per_second": 1000,
                    "transmitted_bytes_per_second": 500,
                    "packets_received_total": 100,
                    "packets_transmitted_total": 50,
                    "receive_errors_total": 0,
                    "transmit_errors_total": 0
                },
                {
                    "name": "bridge0",
                    "received_bytes_total": 9000,
                    "transmitted_bytes_total": 4000,
                    "received_bytes_per_second": 900,
                    "transmitted_bytes_per_second": 400,
                    "packets_received_total": 90,
                    "packets_transmitted_total": 40,
                    "receive_errors_total": 0,
                    "transmit_errors_total": 0
                }
            ],
            "disks": [{
                "name": "sda",
                "mount_point": "/",
                "file_system": "ext4",
                "total_bytes": 100000,
                "available_bytes": 50000,
                "read_bytes_total": 30000,
                "written_bytes_total": 20000,
                "read_bytes_per_second": 3000,
                "written_bytes_per_second": 2000,
                "is_read_only": false
            }],
            "temperatures": [{
                "id": "cpu",
                "label": "CPU",
                "celsius": 52.5,
                "max_celsius": null,
                "critical_celsius": 100.0,
                "source": "test"
            }],
            "gpus": []
        },
        "capabilities": [{
            "name": "system.cpu",
            "available": true,
            "source": "test",
            "error_kind": null,
            "message": null
        }],
        "agent": {"spool_pending_batches": 0, "collector_errors": 0}
    })
}
