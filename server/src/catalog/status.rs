//! Deriving a Show's Category. Private to the catalog: a Category is never
//! assigned from outside, only recomputed from Progress and Production
//! Status.

use sqlx::SqliteConnection;

use shared::{tmdb_status_is_airing, ShowCategory};

/// Recomputes and persists a show's Category. Called after:
///   - toggling an episode's watched flag
///   - refreshing a show's metadata from TMDB (new episodes may appear)
///
/// Rules:
///   - no episodes watched yet -> Watchlist
///   - some episodes watched, but not all -> Watching
///   - all episodes watched:
///       - TMDB production status still airing/upcoming -> Ongoing
///       - TMDB production status ended/canceled        -> Finished
///   - a show previously Ongoing or Finished that gains new unwatched
///     episodes (e.g. a new season dropped) falls back to Watching
///     automatically, since the rule above already produces that result.
///
/// Specials (TMDB season_number 0) are excluded from these counts entirely.
/// TMDB files a large and unstable set of shorts, recaps and clips under
/// season 0, so counting them would hold shows in Watching indefinitely and
/// reclassify them whenever TMDB adds one. They are still stored, shown and
/// individually markable - they just don't decide the Category.
///
/// Takes a connection rather than a pool so it can run inside the same
/// transaction as the mutation that triggered it; otherwise the count it
/// reads could be stale by the time it writes.
pub(super) async fn recompute(
    conn: &mut SqliteConnection,
    show_id: i64,
) -> Result<(), sqlx::Error> {
    // One pass for both counts: they differ only by the watched filter.
    let (total, watched): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(e.watched), 0) FROM episodes e
         JOIN seasons s ON e.season_id = s.id
         WHERE s.show_id = ? AND s.tmdb_season_number != 0",
    )
    .bind(show_id)
    .fetch_one(&mut *conn)
    .await?;

    // One read for both columns of the same row.
    let (tmdb_status, current): (Option<String>, ShowCategory) =
        sqlx::query_as("SELECT tmdb_status, category FROM shows WHERE id = ?")
            .bind(show_id)
            .fetch_one(&mut *conn)
            .await?;

    let category = if total == 0 {
        // No episode data at all (e.g. a transient refresh hiccup before
        // season data has been (re)populated). Don't reclassify off missing
        // data - just leave it as whatever it already was.
        current
    } else if watched == 0 {
        ShowCategory::Watchlist
    } else if watched < total {
        ShowCategory::Watching
    } else {
        let airing = tmdb_status
            .as_deref()
            .map(tmdb_status_is_airing)
            .unwrap_or(false);
        if airing {
            ShowCategory::Ongoing
        } else {
            ShowCategory::Finished
        }
    };

    sqlx::query("UPDATE shows SET category = ? WHERE id = ?")
        .bind(category)
        .bind(show_id)
        .execute(&mut *conn)
        .await?;

    Ok(())
}
