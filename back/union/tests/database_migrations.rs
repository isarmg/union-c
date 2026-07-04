use sqlx_core::{query::query, row::Row};
use union::{
    app_config::Settings,
    database::{self, RamInstanceRecord, ServiceAccountInput},
};

/// 使用专用测试数据库运行：
/// `UNION_TEST_DATABASE_URL=postgresql://.../union_test cargo test --test database_migrations`
#[tokio::test]
async fn migrations_are_versioned_and_idempotent() {
    let Ok(url) = std::env::var("UNION_TEST_DATABASE_URL") else {
        eprintln!("skipped: UNION_TEST_DATABASE_URL is not configured");
        return;
    };
    let mut settings = Settings::default();
    settings.database.url = url;
    let pool = database::connect(&settings)
        .await
        .expect("connect test database");

    database::migrate(&pool).await.expect("first migration");
    database::migrate(&pool).await.expect("second migration");

    let row = query(
        "SELECT COUNT(*) AS count, MAX(checksum) AS checksum \
         FROM schema_migrations WHERE version = 1",
    )
    .fetch_one(&pool)
    .await
    .expect("read migration version");
    assert_eq!(row.get::<i64, _>("count"), 1);
    assert_eq!(row.get::<Option<String>, _>("checksum").unwrap().len(), 64);

    // 配置先写、主机地址后写；第二步失败时第一步必须回滚。
    let baseline = Settings::default();
    database::save_app_settings_and_register_host(
        &pool,
        &baseline,
        "integration-test",
        "atomic-host",
        "127.0.0.1",
    )
    .await
    .expect("seed settings and host");
    let mut rejected = baseline.clone();
    rejected.server.port = 6553;
    assert!(
        database::save_app_settings_and_register_host(
            &pool,
            &rejected,
            "integration-test",
            "atomic-host",
            "invalid host with spaces",
        )
        .await
        .is_err()
    );
    let loaded = database::load_or_seed_app_settings(&pool, &Settings::default())
        .await
        .expect("load settings after rollback");
    assert_eq!(loaded.server.port, baseline.server.port);

    // 删除远程 RAM 时，实例、地址和服务账号必须一起删除。
    let record = RamInstanceRecord {
        id: "integration-ram".to_string(),
        name: "Integration RAM".to_string(),
        host_address: "127.0.0.1".to_string(),
        port: 5599,
        use_tls: false,
        verify_tls: true,
    };
    query("DELETE FROM ram_instances WHERE id=$1")
        .bind(&record.id)
        .execute(&pool)
        .await
        .expect("clear prior test instance");
    database::insert_ram_instance(&pool, &record)
        .await
        .expect("insert RAM instance");
    database::replace_service_accounts(
        &pool,
        "ram:integration-ram",
        &[ServiceAccountInput {
            account_key: "admin".to_string(),
            username: Some("admin".to_string()),
            password_secret: Some("integration-password".to_string()),
            is_anonymous: false,
            is_management: true,
            permissions: Vec::new(),
        }],
    )
    .await
    .expect("insert service account");
    database::delete_ram_instance(&pool, &record.id)
        .await
        .expect("delete RAM aggregate");
    for (table, condition) in [
        ("ram_instances", "id='integration-ram'"),
        (
            "managed_host_addresses",
            "kind='ram' AND host_id='integration-ram'",
        ),
        ("service_accounts", "service_name='ram:integration-ram'"),
    ] {
        let sql = format!("SELECT COUNT(*) AS count FROM {table} WHERE {condition}");
        let row = query(&sql).fetch_one(&pool).await.expect("count rows");
        assert_eq!(row.get::<i64, _>("count"), 0, "table {table}");
    }
}
