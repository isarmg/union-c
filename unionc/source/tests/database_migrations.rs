use sqlx_core::{query::query, row::Row};
use unionc::{app_config::Settings, database};

/// 使用 UNIONC_TEST_DATABASE_URL 指定的专用数据库运行。
#[tokio::test]
async fn migrations_are_versioned_and_idempotent() {
    let Ok(url) = std::env::var("UNIONC_TEST_DATABASE_URL") else {
        eprintln!("skipped: UNIONC_TEST_DATABASE_URL is not configured");
        return;
    };
    let mut settings = Settings::default();
    settings.database.url = url;
    let pool = database::connect(&settings)
        .await
        .expect("connect test database");

    database::migrate(&pool).await.expect("first migration");
    database::migrate(&pool).await.expect("second migration");

    let row = query("SELECT COUNT(*) AS count, MAX(checksum) AS checksum FROM schema_migrations")
        .fetch_one(&pool)
        .await
        .expect("read migration version");
    assert_eq!(row.get::<i64, _>("count"), 2);
    assert_eq!(row.get::<Option<String>, _>("checksum").unwrap().len(), 64);

    let baseline = Settings::default();
    database::save_app_settings(&pool, &baseline)
        .await
        .expect("save settings");
    let loaded = database::load_or_seed_app_settings(&pool, &Settings::default())
        .await
        .expect("load settings");
    assert_eq!(loaded.server.port, baseline.server.port);
    assert_eq!(loaded.sunshine.hosts.len(), baseline.sunshine.hosts.len());

    let invalid_external_host = query(
        "INSERT INTO external_hosts(kind,host_id,address,config,secret) VALUES('sunshine','invalid-json','127.0.0.1','[]',NULL)",
    ).execute(&pool).await;
    assert!(
        invalid_external_host.is_err(),
        "external host config must be a JSON object"
    );

    let unsupported_kind = query(
        "INSERT INTO external_hosts(kind,host_id,address,config,secret) VALUES('unsupported','invalid-kind','127.0.0.1','{}',NULL)",
    ).execute(&pool).await;
    assert!(
        unsupported_kind.is_err(),
        "only Sunshine hosts are accepted"
    );

    let monitoring_tables = query(
        "SELECT COUNT(*) AS count FROM information_schema.tables WHERE table_schema='public' AND table_name IN ('monitored_hosts','agent_metric_reports')",
    )
    .fetch_one(&pool)
    .await
    .expect("read monitoring tables");
    assert_eq!(monitoring_tables.get::<i64, _>("count"), 2);
}
