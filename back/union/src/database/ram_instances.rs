use sqlx_core::{query::query, row::Row};

use super::DbPool;

#[derive(Debug, Clone)]
pub struct RamInstanceRecord {
    pub id: String,
    pub name: String,
    pub host_address: String,
    pub port: u16,
    pub use_tls: bool,
    pub verify_tls: bool,
}

fn from_row(row: sqlx_postgres::PgRow) -> anyhow::Result<RamInstanceRecord> {
    Ok(RamInstanceRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        host_address: row.try_get("bind_address")?,
        port: u16::try_from(row.try_get::<i32, _>("port")?)?,
        use_tls: row.try_get("use_tls")?,
        verify_tls: row.try_get("verify_tls")?,
    })
}

pub async fn ram_instances(pool: &DbPool) -> anyhow::Result<Vec<RamInstanceRecord>> {
    query("SELECT id,name,bind_address,port,use_tls,verify_tls FROM ram_instances ORDER BY created_at")
        .fetch_all(pool).await?.into_iter().map(from_row).collect()
}

pub async fn ram_instance(pool: &DbPool, id: &str) -> anyhow::Result<Option<RamInstanceRecord>> {
    query("SELECT id,name,bind_address,port,use_tls,verify_tls FROM ram_instances WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(from_row)
        .transpose()
}

pub async fn insert_ram_instance(pool: &DbPool, record: &RamInstanceRecord) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    query("INSERT INTO ram_instances(id,name,bind_address,port,serve_path,desired_state,use_tls,verify_tls) VALUES($1,$2,$3,$4,'/','stopped',$5,$6)")
        .bind(&record.id).bind(&record.name).bind(&record.host_address).bind(i32::from(record.port))
        .bind(record.use_tls).bind(record.verify_tls).execute(&mut *tx).await?;
    upsert_ram_host_address(&mut tx, record).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn update_ram_instance(pool: &DbPool, record: &RamInstanceRecord) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    query("UPDATE ram_instances SET name=$2,bind_address=$3,port=$4,use_tls=$5,verify_tls=$6,updated_at=NOW() WHERE id=$1")
        .bind(&record.id).bind(&record.name).bind(&record.host_address).bind(i32::from(record.port))
        .bind(record.use_tls).bind(record.verify_tls).execute(&mut *tx).await?;
    upsert_ram_host_address(&mut tx, record).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete_ram_instance(pool: &DbPool, id: &str) -> anyhow::Result<()> {
    let service_name = format!("ram:{id}");
    let mut tx = pool.begin().await?;
    query(
        "DELETE FROM service_account_permissions WHERE account_id IN \
         (SELECT id FROM service_accounts WHERE service_name=$1)",
    )
    .bind(&service_name)
    .execute(&mut *tx)
    .await?;
    query("DELETE FROM service_accounts WHERE service_name=$1")
        .bind(&service_name)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM managed_host_addresses WHERE kind='ram' AND host_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM ram_instances WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn upsert_ram_host_address(
    tx: &mut sqlx_core::transaction::Transaction<'_, sqlx_postgres::Postgres>,
    record: &RamInstanceRecord,
) -> anyhow::Result<()> {
    query(
        "INSERT INTO managed_host_addresses(kind,host_id,address) VALUES('ram',$1,$2) \
         ON CONFLICT(kind,host_id) DO UPDATE SET address=EXCLUDED.address,updated_at=NOW()",
    )
    .bind(&record.id)
    .bind(&record.host_address)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
