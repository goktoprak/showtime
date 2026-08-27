use sqlx::SqlitePool;

use shared::Show;

use super::{shows, status, CatalogError, Result};

/// Flips one episode's watched flag and returns the show with its Category
/// recomputed. The mutation and the recompute share a transaction, so the
/// counts the recompute reads cannot be stale.
pub async fn toggle_episode(pool: &SqlitePool, episode_id: i64) -> Result<Show> {
    let mut tx = pool.begin().await?;

    let current: bool = sqlx::query_scalar("SELECT watched FROM episodes WHERE id = ?")
        .bind(episode_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(CatalogError::NotFound("episode not found"))?;

    let new_watched = !current;
    // Unwatching clears the timestamp: an unwatched episode has no watch time.
    let watched_at = new_watched.then(|| chrono::Utc::now().to_rfc3339());

    sqlx::query("UPDATE episodes SET watched = ?, watched_at = ? WHERE id = ?")
        .bind(new_watched)
        .bind(&watched_at)
        .bind(episode_id)
        .execute(&mut *tx)
        .await?;

    let show_id: i64 = sqlx::query_scalar(
        "SELECT s.show_id FROM seasons s
         JOIN episodes e ON e.season_id = s.id
         WHERE e.id = ?",
    )
    .bind(episode_id)
    .fetch_one(&mut *tx)
    .await?;

    status::recompute(&mut tx, show_id).await?;
    tx.commit().await?;

    shows::show(pool, show_id).await
}

pub async fn mark_season(pool: &SqlitePool, season_id: i64) -> Result<Show> {
    let mut tx = pool.begin().await?;

    // Resolve the season first so an unknown id is NotFound rather than a
    // database error out of the recompute further down.
    let show_id: i64 = sqlx::query_scalar("SELECT show_id FROM seasons WHERE id = ?")
        .bind(season_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(CatalogError::NotFound("season not found"))?;

    // `AND watched = 0` keeps the original watch time on episodes that were
    // already marked: re-marking a season must not rewrite history.
    sqlx::query(
        "UPDATE episodes SET watched = 1, watched_at = ?
         WHERE season_id = ? AND watched = 0",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(season_id)
    .execute(&mut *tx)
    .await?;

    status::recompute(&mut tx, show_id).await?;
    tx.commit().await?;

    shows::show(pool, show_id).await
}

pub async fn mark_show(pool: &SqlitePool, show_id: i64) -> Result<Show> {
    // Same reason as above: NotFound before mutating anything.
    shows::show(pool, show_id).await?;

    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE episodes SET watched = 1, watched_at = ?
         WHERE season_id IN (SELECT id FROM seasons WHERE show_id = ?)
           AND watched = 0",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(show_id)
    .execute(&mut *tx)
    .await?;

    status::recompute(&mut tx, show_id).await?;
    tx.commit().await?;

    shows::show(pool, show_id).await
}
