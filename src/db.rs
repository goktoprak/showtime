use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub async fn init_pool(db_path: &str) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{db_path}"))?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    Ok(pool)
}

pub async fn get_api_key(pool: &SqlitePool) -> anyhow::Result<Option<String>> {
    let key: Option<String> =
        sqlx::query_scalar("SELECT tmdb_api_key FROM settings WHERE id = 1")
            .fetch_one(pool)
            .await?;
    Ok(key.filter(|k| !k.is_empty()))
}
