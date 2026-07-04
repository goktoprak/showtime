use sqlx::SqlitePool;

use crate::models::tmdb_status_is_airing;

/// Recomputes and persists a show's category based on its episodes' watched
/// state and the show's raw TMDB status. Call this after:
///   - toggling an episode's watched flag
///   - refreshing a show's metadata from TMDB (new episodes may appear)
///
/// Rules:
///   - no episodes watched yet AND show has never had any watched episode
///     -> "watchlist" (only applies to brand new shows; see note below)
///   - some episodes watched, but not all -> "watching"
///   - all episodes watched:
///       - TMDB status still airing/upcoming -> "ongoing"
///       - TMDB status ended/canceled        -> "finished"
///   - special case: a show previously "ongoing" or "finished" that gains
///     new unwatched episodes (e.g. a new season dropped) falls back to
///     "watching" automatically, since the rule above already produces that
///     result (not-all-watched -> "watching"), no extra code needed.
///
/// Note: "Specials" (TMDB season_number 0) are excluded from these counts
/// entirely. They're still stored and shown on the show detail page and can
/// be checked off individually, but whether they're watched has no effect
/// on the show's category - a show with every regular-season episode
/// watched still counts as ongoing/finished even with unwatched specials.
pub async fn recompute_show_status(pool: &SqlitePool, show_id: i64) -> anyhow::Result<()> {
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM episodes e
         JOIN seasons s ON e.season_id = s.id
         WHERE s.show_id = ? AND s.tmdb_season_number != 0",
    )
    .bind(show_id)
    .fetch_one(pool)
    .await?;

    let watched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM episodes e
         JOIN seasons s ON e.season_id = s.id
         WHERE s.show_id = ? AND s.tmdb_season_number != 0 AND e.watched = 1",
    )
    .bind(show_id)
    .fetch_one(pool)
    .await?;

    let tmdb_status: Option<String> =
        sqlx::query_scalar("SELECT tmdb_status FROM shows WHERE id = ?")
            .bind(show_id)
            .fetch_one(pool)
            .await?;

    let current_status: String = sqlx::query_scalar("SELECT status FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_one(pool)
        .await?;

    let new_status = if total == 0 {
        // No episode data at all (e.g. a transient refresh hiccup before
        // season data has been (re)populated). Don't change status based on
        // missing data - just leave it as whatever it already was.
        current_status.clone()
    } else if watched == 0 {
        // Episodes exist but none are watched -> watch list.
        "watchlist".to_string()
    } else if watched < total {
        "watching".to_string()
    } else {
        // all episodes watched
        let airing = tmdb_status
            .as_deref()
            .map(tmdb_status_is_airing)
            .unwrap_or(false);
        if airing {
            "ongoing".to_string()
        } else {
            "finished".to_string()
        }
    };

    sqlx::query("UPDATE shows SET status = ? WHERE id = ?")
        .bind(new_status)
        .bind(show_id)
        .execute(pool)
        .await?;

    Ok(())
}
