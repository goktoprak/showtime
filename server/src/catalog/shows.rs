use sqlx::{QueryBuilder, SqlitePool};

use shared::{Episode, Season, SeasonWithEpisodes, Show, ShowCategory, ShowDetail};

use super::{status, CatalogError, FetchedShow, Result};

/// SQLite caps bound variables per statement. Five binds per episode row, so
/// 100 rows a chunk stays far under even the old 999 limit.
const EPISODE_CHUNK: usize = 100;

pub async fn list(pool: &SqlitePool) -> Result<Vec<Show>> {
    Ok(
        sqlx::query_as::<_, Show>("SELECT * FROM shows ORDER BY name COLLATE NOCASE ASC")
            .fetch_all(pool)
            .await?,
    )
}

/// Loads a show row, or `NotFound`. Every operation that returns a show goes
/// through here so the not-found behaviour is identical across all of them.
pub async fn show(pool: &SqlitePool, show_id: i64) -> Result<Show> {
    sqlx::query_as::<_, Show>("SELECT * FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_optional(pool)
        .await?
        .ok_or(CatalogError::NotFound("show not found"))
}

/// The whole show in two queries - one for seasons, one for every episode
/// under it - grouped in memory. Previously this was one query per season.
pub async fn detail(pool: &SqlitePool, show_id: i64) -> Result<ShowDetail> {
    let show = show(pool, show_id).await?;

    let seasons = sqlx::query_as::<_, Season>(
        "SELECT * FROM seasons WHERE show_id = ? ORDER BY tmdb_season_number ASC",
    )
    .bind(show_id)
    .fetch_all(pool)
    .await?;

    let episodes = sqlx::query_as::<_, Episode>(
        "SELECT e.* FROM episodes e
         JOIN seasons s ON e.season_id = s.id
         WHERE s.show_id = ?
         ORDER BY s.tmdb_season_number ASC, e.tmdb_episode_number ASC",
    )
    .bind(show_id)
    .fetch_all(pool)
    .await?;

    // Seasons come back in order and episodes are already grouped by season
    // within that order, so a single pass over each is enough.
    let mut by_season: std::collections::HashMap<i64, Vec<Episode>> =
        std::collections::HashMap::new();
    for episode in episodes {
        by_season.entry(episode.season_id).or_default().push(episode);
    }

    let seasons = seasons
        .into_iter()
        .map(|season| SeasonWithEpisodes {
            episodes: by_season.remove(&season.id).unwrap_or_default(),
            season,
        })
        .collect();

    Ok(ShowDetail { show, seasons })
}

pub async fn is_tracked(pool: &SqlitePool, tmdb_id: i64) -> Result<bool> {
    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM shows WHERE tmdb_id = ?")
        .bind(tmdb_id)
        .fetch_optional(pool)
        .await?;
    Ok(existing.is_some())
}

pub async fn tmdb_id_of(pool: &SqlitePool, show_id: i64) -> Result<i64> {
    sqlx::query_scalar("SELECT tmdb_id FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_optional(pool)
        .await?
        .ok_or(CatalogError::NotFound("show not found"))
}

pub async fn delete(pool: &SqlitePool, show_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM shows WHERE id = ?")
        .bind(show_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Inserts a brand new show together with every season and episode, in one
/// transaction. All-or-nothing is the point: a half-written show would be
/// tracked but empty, and re-adding it would then conflict.
pub async fn insert_show(pool: &SqlitePool, fetched: &FetchedShow) -> Result<Show> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    let show_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO shows (tmdb_id, name, overview, poster_path, backdrop_path, tmdb_status, category, added_at, last_refreshed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id",
    )
    .bind(fetched.tmdb_id)
    .bind(&fetched.name)
    .bind(&fetched.overview)
    .bind(&fetched.poster_path)
    .bind(&fetched.backdrop_path)
    .bind(&fetched.tmdb_status)
    .bind(ShowCategory::Watchlist)
    .bind(&now)
    .bind(&now)
    .fetch_one(&mut *tx)
    .await?;

    write_seasons(&mut tx, show_id, fetched).await?;
    status::recompute(&mut tx, show_id).await?;
    tx.commit().await?;

    show(pool, show_id).await
}

/// Re-writes an existing show's metadata, seasons and episodes in one
/// transaction. Existing `watched` flags survive; new episodes arrive
/// unwatched.
pub async fn apply_refresh(pool: &SqlitePool, show_id: i64, fetched: &FetchedShow) -> Result<Show> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;

    sqlx::query(
        "UPDATE shows SET name = ?, overview = ?, poster_path = ?, backdrop_path = ?, tmdb_status = ?, last_refreshed_at = ?
         WHERE id = ?",
    )
    .bind(&fetched.name)
    .bind(&fetched.overview)
    .bind(&fetched.poster_path)
    .bind(&fetched.backdrop_path)
    .bind(&fetched.tmdb_status)
    .bind(&now)
    .bind(show_id)
    .execute(&mut *tx)
    .await?;

    write_seasons(&mut tx, show_id, fetched).await?;
    status::recompute(&mut tx, show_id).await?;
    tx.commit().await?;

    show(pool, show_id).await
}

/// Upserts every season and its episodes. Specials (season 0) are stored like
/// any other season; they are only excluded when Category is derived.
async fn write_seasons(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    show_id: i64,
    fetched: &FetchedShow,
) -> Result<()> {
    for season in &fetched.seasons {
        let season_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO seasons (show_id, tmdb_season_number, name, episode_count)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(show_id, tmdb_season_number)
             DO UPDATE SET name = excluded.name, episode_count = excluded.episode_count
             RETURNING id",
        )
        .bind(show_id)
        .bind(season.number)
        .bind(&season.name)
        .bind(season.episode_count)
        .fetch_one(&mut **tx)
        .await?;

        // One statement per chunk rather than one per episode: a 200-episode
        // show went from 200 round trips to two.
        for chunk in season.episodes.chunks(EPISODE_CHUNK) {
            let mut qb = QueryBuilder::new(
                "INSERT INTO episodes (season_id, tmdb_episode_number, name, air_date, watched) ",
            );
            qb.push_values(chunk, |mut b, ep| {
                b.push_bind(season_id)
                    .push_bind(ep.number)
                    .push_bind(&ep.name)
                    .push_bind(&ep.air_date)
                    .push_bind(0i64);
            });
            qb.push(
                " ON CONFLICT(season_id, tmdb_episode_number)
                  DO UPDATE SET name = excluded.name, air_date = excluded.air_date",
            );
            qb.build().execute(&mut **tx).await?;
        }
    }

    Ok(())
}
