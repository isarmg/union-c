//! 运行配置与 Sunshine 主机持久化。

use sqlx_core::{query::query, row::Row};
use sqlx_postgres::PgConnection;

use crate::app_config::Settings;

use super::DbPool;

const APP_SETTINGS_KEY: &str = "unionc.runtime_settings";

pub async fn load_or_seed_app_settings(
    pool: &DbPool,
    bootstrap: &Settings,
) -> anyhow::Result<Settings> {
    if let Some(stored) = get_setting(pool, APP_SETTINGS_KEY).await? {
        if !crate::secrets::is_encrypted(&stored) {
            anyhow::bail!("unencrypted app runtime settings are not supported");
        }
        let raw = crate::secrets::decrypt(&stored)?;
        let mut settings: Settings = serde_json::from_str(&raw)
            .map_err(|error| anyhow::anyhow!("invalid unionc runtime settings: {error}"))?;
        settings.database = bootstrap.database.clone();
        load_sunshine_hosts(pool, &mut settings).await?;
        return Ok(settings);
    }
    save_app_settings(pool, bootstrap).await?;
    Ok(bootstrap.clone())
}

pub async fn save_app_settings(pool: &DbPool, settings: &Settings) -> anyhow::Result<()> {
    let mut base_settings = settings.clone();
    let hosts = std::mem::take(&mut base_settings.sunshine.hosts);
    let value = crate::secrets::encrypt(&serde_json::to_string(&base_settings)?)?;
    let mut tx = pool.begin().await?;
    upsert_setting(&mut tx, APP_SETTINGS_KEY, &value).await?;
    query("DELETE FROM external_hosts WHERE kind = 'sunshine'")
        .execute(&mut *tx)
        .await?;
    for mut host in hosts {
        let address = std::mem::take(&mut host.host);
        let password = std::mem::take(&mut host.password);
        insert_external_host(
            &mut tx,
            &host.id,
            &address,
            &serde_json::to_string(&host)?,
            &password,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn insert_external_host(
    connection: &mut PgConnection,
    id: &str,
    address: &str,
    config: &str,
    secret: &str,
) -> anyhow::Result<()> {
    let secret = (!secret.is_empty())
        .then(|| crate::secrets::encrypt(secret))
        .transpose()?;
    query("INSERT INTO external_hosts(kind,host_id,address,config,secret) VALUES('sunshine',$1,$2,$3,$4)")
        .bind(id).bind(address).bind(config).bind(secret).execute(connection).await?;
    Ok(())
}

async fn load_sunshine_hosts(pool: &DbPool, settings: &mut Settings) -> anyhow::Result<()> {
    let rows = query("SELECT address,config,secret FROM external_hosts WHERE kind='sunshine' ORDER BY created_at,host_id")
        .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(());
    }
    settings.sunshine.hosts.clear();
    for row in rows {
        let mut host: crate::app_config::SunshineHostConfig =
            serde_json::from_str(&row.try_get::<String, _>("config")?)?;
        host.host = row.try_get("address")?;
        host.password = row
            .try_get::<Option<String>, _>("secret")?
            .map(|value| crate::secrets::decrypt(&value))
            .transpose()?
            .unwrap_or_default();
        settings.sunshine.hosts.push(host);
    }
    Ok(())
}

pub async fn get_setting(pool: &DbPool, key: &str) -> anyhow::Result<Option<String>> {
    let row = query("SELECT value FROM settings WHERE setting_key=$1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    row.map(|row| row.try_get("value").map_err(Into::into))
        .transpose()
}

pub async fn set_setting(pool: &DbPool, key: &str, value: &str) -> anyhow::Result<()> {
    let mut connection = pool.acquire().await?;
    upsert_setting(&mut connection, key, value).await
}

async fn upsert_setting(
    connection: &mut PgConnection,
    key: &str,
    value: &str,
) -> anyhow::Result<()> {
    query("INSERT INTO settings(setting_key,value,updated_at) VALUES($1,$2,NOW()) ON CONFLICT(setting_key) DO UPDATE SET value=EXCLUDED.value,updated_at=NOW()")
        .bind(key).bind(value).execute(connection).await?;
    Ok(())
}
