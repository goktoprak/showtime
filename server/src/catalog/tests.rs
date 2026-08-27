use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use super::*;
use shared::ShowCategory;

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

async fn seed_show(pool: &SqlitePool, tmdb_status: Option<&str>, category: ShowCategory) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO shows (tmdb_id, name, tmdb_status, category, added_at)
         VALUES (1, 'Test Show', ?, ?, 'now') RETURNING id",
    )
    .bind(tmdb_status)
    .bind(category)
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
    sqlx::query(
        "UPDATE episodes SET watched = 1 WHERE season_id = ? AND tmdb_episode_number <= ?",
    )
    .bind(season_id)
    .bind(count)
    .execute(pool)
    .await
    .unwrap();
}

async fn category_of(pool: &SqlitePool, show_id: i64) -> ShowCategory {
    sqlx::query_scalar("SELECT category FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The recompute runs against a connection so it can share a transaction with
/// its caller; these tests don't need one, so they lend it a pooled one.
async fn recompute(pool: &SqlitePool, show_id: i64) {
    let mut conn = pool.acquire().await.unwrap();
    status::recompute(&mut conn, show_id).await.unwrap();
}

async fn episode_ids(pool: &SqlitePool, season_id: i64) -> Vec<i64> {
    sqlx::query_scalar("SELECT id FROM episodes WHERE season_id = ? ORDER BY tmdb_episode_number")
        .bind(season_id)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn watched_at_of(pool: &SqlitePool, episode_id: i64) -> Option<String> {
    sqlx::query_scalar("SELECT watched_at FROM episodes WHERE id = ?")
        .bind(episode_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn fetched(tmdb_status: &str, seasons: Vec<FetchedSeason>) -> FetchedShow {
    FetchedShow {
        tmdb_id: 42,
        name: "Fetched Show".to_string(),
        overview: None,
        poster_path: None,
        backdrop_path: None,
        tmdb_status: Some(tmdb_status.to_string()),
        seasons,
    }
}

fn fetched_season(number: i64, episodes: i64) -> FetchedSeason {
    FetchedSeason {
        number,
        name: Some(format!("Season {number}")),
        episode_count: episodes,
        episodes: (1..=episodes)
            .map(|n| FetchedEpisode {
                number: n,
                name: Some(format!("E{n}")),
                air_date: None,
            })
            .collect(),
    }
}

// ---------- category derivation ----------

#[tokio::test]
async fn no_episodes_at_all_leaves_the_status_alone() {
    // Guards the deliberate no-op: a refresh that hasn't repopulated
    // seasons yet must not reclassify the show off missing data.
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Ended"), ShowCategory::Finished).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Finished);
}

#[tokio::test]
async fn episodes_but_none_watched_is_watchlist() {
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Ended"), ShowCategory::Watching).await;
    add_season(&pool, show, 1, 3).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Watchlist);
}

#[tokio::test]
async fn some_but_not_all_watched_is_watching() {
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    let s1 = add_season(&pool, show, 1, 3).await;
    watch(&pool, s1, 1).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Watching);
}

#[tokio::test]
async fn all_watched_while_still_airing_is_ongoing() {
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Returning Series"), ShowCategory::Watching).await;
    let s1 = add_season(&pool, show, 1, 3).await;
    watch(&pool, s1, 3).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Ongoing);
}

#[tokio::test]
async fn all_watched_once_ended_is_finished() {
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Ended"), ShowCategory::Watching).await;
    let s1 = add_season(&pool, show, 1, 3).await;
    watch(&pool, s1, 3).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Finished);
}

#[tokio::test]
async fn all_watched_with_unknown_tmdb_status_is_finished() {
    // tmdb_status is nullable; a missing status must not be treated as
    // still airing.
    let pool = test_pool().await;
    let show = seed_show(&pool, None, ShowCategory::Watching).await;
    let s1 = add_season(&pool, show, 1, 2).await;
    watch(&pool, s1, 2).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Finished);
}

#[tokio::test]
async fn unwatched_specials_do_not_hold_a_show_back() {
    // Season 0 is excluded from the counts, so every regular episode
    // being watched is enough to finish the show.
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Ended"), ShowCategory::Watching).await;
    add_season(&pool, show, 0, 5).await;
    let s1 = add_season(&pool, show, 1, 3).await;
    watch(&pool, s1, 3).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Finished);
}

#[tokio::test]
async fn watching_only_specials_does_not_start_the_show() {
    // The mirror image: specials count for nothing in either direction.
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    let specials = add_season(&pool, show, 0, 5).await;
    add_season(&pool, show, 1, 3).await;
    watch(&pool, specials, 5).await;

    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Watchlist);
}

#[tokio::test]
async fn unwatching_everything_returns_to_watchlist() {
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    let s1 = add_season(&pool, show, 1, 2).await;
    watch(&pool, s1, 2).await;
    recompute(&pool, show).await;
    assert_eq!(category_of(&pool, show).await, ShowCategory::Finished);

    sqlx::query("UPDATE episodes SET watched = 0")
        .execute(&pool)
        .await
        .unwrap();
    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Watchlist);
}

#[tokio::test]
async fn a_new_episode_drops_an_ongoing_show_back_to_watching() {
    // The behaviour the README promises after a metadata refresh.
    let pool = test_pool().await;
    let show = seed_show(&pool, Some("Returning Series"), ShowCategory::Watchlist).await;
    let s1 = add_season(&pool, show, 1, 2).await;
    watch(&pool, s1, 2).await;
    recompute(&pool, show).await;
    assert_eq!(category_of(&pool, show).await, ShowCategory::Ongoing);

    add_season(&pool, show, 2, 1).await;
    recompute(&pool, show).await;

    assert_eq!(category_of(&pool, show).await, ShowCategory::Watching);
}

// ---------- marking watched ----------

#[tokio::test]
async fn toggling_an_episode_flips_it_and_recomputes_the_category() {
    let pool = test_pool().await;
    let show_id = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    let season = add_season(&pool, show_id, 1, 2).await;
    let eps = episode_ids(&pool, season).await;

    let show = toggle_episode(&pool, eps[0]).await.unwrap();
    assert_eq!(show.category, ShowCategory::Watching);
    assert!(watched_at_of(&pool, eps[0]).await.is_some());

    let show = toggle_episode(&pool, eps[0]).await.unwrap();
    assert_eq!(show.category, ShowCategory::Watchlist);
    // Unwatching clears the timestamp too.
    assert!(watched_at_of(&pool, eps[0]).await.is_none());
}

#[tokio::test]
async fn toggling_an_unknown_episode_is_not_found() {
    let pool = test_pool().await;
    assert!(matches!(
        toggle_episode(&pool, 999).await,
        Err(CatalogError::NotFound(_))
    ));
}

#[tokio::test]
async fn marking_a_season_again_keeps_the_original_watch_times() {
    // Regression: the update used to rewrite watched_at for every episode in
    // the season, silently destroying when the user actually watched them.
    let pool = test_pool().await;
    let show_id = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    let season = add_season(&pool, show_id, 1, 3).await;
    let eps = episode_ids(&pool, season).await;

    toggle_episode(&pool, eps[0]).await.unwrap();
    let first_watched_at = watched_at_of(&pool, eps[0]).await.unwrap();

    mark_season(&pool, season).await.unwrap();

    assert_eq!(
        watched_at_of(&pool, eps[0]).await.unwrap(),
        first_watched_at,
        "an already-watched episode must keep its original watch time"
    );
    assert!(watched_at_of(&pool, eps[1]).await.is_some());
}

#[tokio::test]
async fn marking_a_show_again_keeps_the_original_watch_times() {
    let pool = test_pool().await;
    let show_id = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    let season = add_season(&pool, show_id, 1, 3).await;
    let eps = episode_ids(&pool, season).await;

    toggle_episode(&pool, eps[0]).await.unwrap();
    let first_watched_at = watched_at_of(&pool, eps[0]).await.unwrap();

    let show = mark_show(&pool, show_id).await.unwrap();
    assert_eq!(show.category, ShowCategory::Finished);

    assert_eq!(
        watched_at_of(&pool, eps[0]).await.unwrap(),
        first_watched_at
    );
}

#[tokio::test]
async fn marking_a_season_leaves_other_seasons_alone() {
    let pool = test_pool().await;
    let show_id = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    let s1 = add_season(&pool, show_id, 1, 2).await;
    add_season(&pool, show_id, 2, 2).await;

    let show = mark_season(&pool, s1).await.unwrap();

    // Season 2 is still unwatched, so the show is only part-way through.
    assert_eq!(show.category, ShowCategory::Watching);
}

#[tokio::test]
async fn marking_an_unknown_season_is_not_found() {
    let pool = test_pool().await;
    assert!(matches!(
        mark_season(&pool, 999).await,
        Err(CatalogError::NotFound(_))
    ));
}

// ---------- reading ----------

#[tokio::test]
async fn detail_groups_every_episode_under_its_own_season() {
    let pool = test_pool().await;
    let show_id = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;
    add_season(&pool, show_id, 1, 3).await;
    add_season(&pool, show_id, 2, 2).await;

    let detail = detail(&pool, show_id).await.unwrap();

    assert_eq!(detail.seasons.len(), 2);
    assert_eq!(detail.seasons[0].season.tmdb_season_number, 1);
    assert_eq!(detail.seasons[0].episodes.len(), 3);
    assert_eq!(detail.seasons[1].episodes.len(), 2);
    // Episodes stay in broadcast order within a season.
    assert_eq!(detail.seasons[0].episodes[0].tmdb_episode_number, 1);
    assert_eq!(detail.seasons[0].episodes[2].tmdb_episode_number, 3);
}

#[tokio::test]
async fn detail_of_a_show_with_no_seasons_is_empty_not_an_error() {
    let pool = test_pool().await;
    let show_id = seed_show(&pool, Some("Ended"), ShowCategory::Watchlist).await;

    let detail = detail(&pool, show_id).await.unwrap();

    assert!(detail.seasons.is_empty());
}

#[tokio::test]
async fn detail_of_an_unknown_show_is_not_found() {
    let pool = test_pool().await;
    assert!(matches!(
        detail(&pool, 999).await,
        Err(CatalogError::NotFound(_))
    ));
}

// ---------- writing fetched metadata ----------

#[tokio::test]
async fn inserting_a_fetched_show_writes_every_season_and_episode() {
    let pool = test_pool().await;

    let show = insert_show(
        &pool,
        &fetched("Returning Series", vec![fetched_season(1, 3), fetched_season(2, 2)]),
    )
    .await
    .unwrap();

    assert_eq!(show.category, ShowCategory::Watchlist);
    let detail = detail(&pool, show.id).await.unwrap();
    assert_eq!(detail.seasons.len(), 2);
    assert_eq!(detail.seasons[0].episodes.len(), 3);
    assert!(detail.seasons[0].episodes.iter().all(|e| !e.watched));
}

#[tokio::test]
async fn a_refresh_preserves_watched_marks_and_adds_new_episodes() {
    let pool = test_pool().await;
    let show = insert_show(&pool, &fetched("Returning Series", vec![fetched_season(1, 2)]))
        .await
        .unwrap();

    let season = detail(&pool, show.id).await.unwrap().seasons[0].season.id;
    let marked = mark_season(&pool, season).await.unwrap();
    assert_eq!(marked.category, ShowCategory::Ongoing);

    // A new episode appears in the season the user had finished.
    let show = apply_refresh(
        &pool,
        show.id,
        &fetched("Returning Series", vec![fetched_season(1, 3)]),
    )
    .await
    .unwrap();

    let detail = detail(&pool, show.id).await.unwrap();
    let episodes = &detail.seasons[0].episodes;
    assert_eq!(episodes.len(), 3);
    assert!(episodes[0].watched, "existing marks must survive a refresh");
    assert!(episodes[1].watched);
    assert!(!episodes[2].watched, "a new episode arrives unwatched");
    assert_eq!(show.category, ShowCategory::Watching);
}

#[tokio::test]
async fn a_refresh_writes_more_episodes_than_one_statement_can_bind() {
    // The episode upsert is chunked; this crosses the chunk boundary.
    let pool = test_pool().await;

    let show = insert_show(&pool, &fetched("Ended", vec![fetched_season(1, 250)]))
        .await
        .unwrap();

    let detail = detail(&pool, show.id).await.unwrap();
    assert_eq!(detail.seasons[0].episodes.len(), 250);
    assert_eq!(detail.seasons[0].episodes[249].tmdb_episode_number, 250);
}

// ---------- settings ----------

#[tokio::test]
async fn an_api_key_round_trips_and_clears() {
    let pool = test_pool().await;
    assert_eq!(api_key(&pool).await.unwrap(), None);

    set_api_key(&pool, "abc123").await.unwrap();
    assert_eq!(api_key(&pool).await.unwrap().as_deref(), Some("abc123"));

    clear_api_key(&pool).await.unwrap();
    assert_eq!(api_key(&pool).await.unwrap(), None);
}

#[tokio::test]
async fn an_empty_stored_key_reads_as_no_key() {
    let pool = test_pool().await;
    set_api_key(&pool, "").await.unwrap();
    assert_eq!(api_key(&pool).await.unwrap(), None);
}
