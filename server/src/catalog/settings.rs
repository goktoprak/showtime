use sqlx::SqlitePool;

use super::Result;

pub async fn api_key(pool: &SqlitePool) -> Result<Option<String>> {
    let key: Option<String> = sqlx::query_scalar("SELECT tmdb_api_key FROM settings WHERE id = 1")
        .fetch_one(pool)
        .await?;
    Ok(key.filter(|k| !k.is_empty()))
}

pub async fn set_api_key(pool: &SqlitePool, key: &str) -> Result<()> {
    sqlx::query("UPDATE settings SET tmdb_api_key = ? WHERE id = 1")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn clear_api_key(pool: &SqlitePool) -> Result<()> {
    sqlx::query("UPDATE settings SET tmdb_api_key = NULL WHERE id = 1")
        .execute(pool)
        .await?;
    Ok(())
}

/// Writes a consistent copy of the whole database to `path` via `VACUUM
/// INTO`, which is safe even with uncheckpointed WAL writes outstanding.
pub async fn snapshot(pool: &SqlitePool, path: &str) -> Result<()> {
    sqlx::query("VACUUM INTO ?")
        .bind(path)
        .execute(pool)
        .await?;
    Ok(())
}
