use sqlx::SqlitePool;

use shared::tmdb_status_is_airing;

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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// A migrated, empty database. Capped at one connection because each
    /// connection to `sqlite::memory:` would otherwise get its own private
    /// database; the timeouts are disabled so it can't be recycled out from
    /// under the test.
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../migrations").run(&pool).await.unwrap();
        pool
    }

    async fn seed_show(pool: &SqlitePool, tmdb_status: Option<&str>, status: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO shows (tmdb_id, name, tmdb_status, status, added_at)
             VALUES (1, 'Test Show', ?, ?, 'now') RETURNING id",
        )
        .bind(tmdb_status)
        .bind(status)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Adds a season with `episodes` unwatched episodes, numbered from 1.
    async fn add_season(pool: &SqlitePool, show_id: i64, number: i64, episodes: i64) -> i64 {
        let season_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO seasons (show_id, tmdb_season_number, name, episode_count)
             VALUES (?, ?, 'S', ?) RETURNING id",
        )
        .bind(show_id)
        .bind(number)
        .bind(episodes)
        .fetch_one(pool)
        .await
        .unwrap();

        for n in 1..=episodes {
            sqlx::query(
                "INSERT INTO episodes (season_id, tmdb_episode_number, name, watched)
                 VALUES (?, ?, 'E', 0)",
            )
            .bind(season_id)
            .bind(n)
            .execute(pool)
            .await
            .unwrap();
        }
        season_id
    }

    /// Marks the first `count` episodes of a season watched.
    async fn watch(pool: &SqlitePool, season_id: i64, count: i64) {
        sqlx::query("UPDATE episodes SET watched = 1 WHERE season_id = ? AND tmdb_episode_number <= ?")
            .bind(season_id)
            .bind(count)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn status_of(pool: &SqlitePool, show_id: i64) -> String {
        sqlx::query_scalar("SELECT status FROM shows WHERE id = ?")
            .bind(show_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn no_episodes_at_all_leaves_the_status_alone() {
        // Guards the deliberate no-op: a refresh that hasn't repopulated
        // seasons yet must not reclassify the show off missing data.
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Ended"), "finished").await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "finished");
    }

    #[tokio::test]
    async fn episodes_but_none_watched_is_watchlist() {
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Ended"), "watching").await;
        add_season(&pool, show, 1, 3).await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "watchlist");
    }

    #[tokio::test]
    async fn some_but_not_all_watched_is_watching() {
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Ended"), "watchlist").await;
        let s1 = add_season(&pool, show, 1, 3).await;
        watch(&pool, s1, 1).await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "watching");
    }

    #[tokio::test]
    async fn all_watched_while_still_airing_is_ongoing() {
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Returning Series"), "watching").await;
        let s1 = add_season(&pool, show, 1, 3).await;
        watch(&pool, s1, 3).await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "ongoing");
    }

    #[tokio::test]
    async fn all_watched_once_ended_is_finished() {
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Ended"), "watching").await;
        let s1 = add_season(&pool, show, 1, 3).await;
        watch(&pool, s1, 3).await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "finished");
    }

    #[tokio::test]
    async fn all_watched_with_unknown_tmdb_status_is_finished() {
        // tmdb_status is nullable; a missing status must not be treated as
        // still airing.
        let pool = test_pool().await;
        let show = seed_show(&pool, None, "watching").await;
        let s1 = add_season(&pool, show, 1, 2).await;
        watch(&pool, s1, 2).await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "finished");
    }

    #[tokio::test]
    async fn unwatched_specials_do_not_hold_a_show_back() {
        // Season 0 is excluded from the counts, so every regular episode
        // being watched is enough to finish the show.
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Ended"), "watching").await;
        add_season(&pool, show, 0, 5).await;
        let s1 = add_season(&pool, show, 1, 3).await;
        watch(&pool, s1, 3).await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "finished");
    }

    #[tokio::test]
    async fn watching_only_specials_does_not_start_the_show() {
        // The mirror image: specials count for nothing in either direction.
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Ended"), "watchlist").await;
        let specials = add_season(&pool, show, 0, 5).await;
        add_season(&pool, show, 1, 3).await;
        watch(&pool, specials, 5).await;

        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "watchlist");
    }

    #[tokio::test]
    async fn unwatching_everything_returns_to_watchlist() {
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Ended"), "watchlist").await;
        let s1 = add_season(&pool, show, 1, 2).await;
        watch(&pool, s1, 2).await;
        recompute_show_status(&pool, show).await.unwrap();
        assert_eq!(status_of(&pool, show).await, "finished");

        sqlx::query("UPDATE episodes SET watched = 0")
            .execute(&pool)
            .await
            .unwrap();
        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "watchlist");
    }

    #[tokio::test]
    async fn a_new_episode_drops_an_ongoing_show_back_to_watching() {
        // The behaviour the README promises after a metadata refresh.
        let pool = test_pool().await;
        let show = seed_show(&pool, Some("Returning Series"), "watchlist").await;
        let s1 = add_season(&pool, show, 1, 2).await;
        watch(&pool, s1, 2).await;
        recompute_show_status(&pool, show).await.unwrap();
        assert_eq!(status_of(&pool, show).await, "ongoing");

        add_season(&pool, show, 2, 1).await;
        recompute_show_status(&pool, show).await.unwrap();

        assert_eq!(status_of(&pool, show).await, "watching");
    }
}
