//! Reading a show's metadata out of TMDB and into the shape the catalog
//! writes. The catalog never calls TMDB itself (`docs/adr/0003`), so this is
//! where the two meet.

use futures::stream::{self, StreamExt, TryStreamExt};

use crate::catalog::{FetchedEpisode, FetchedSeason, FetchedShow};
use crate::tmdb::{TmdbClient, TmdbError, TmdbShow};

/// How many season requests may be in flight at once. Bounded rather than
/// unlimited: a twenty-season show firing twenty simultaneous requests is a
/// good way to meet TMDB's rate limiter.
const SEASON_CONCURRENCY: usize = 4;

/// Fetches a show and every one of its seasons. Nothing is written here: the
/// caller hands the whole result to the catalog, which applies it in one
/// transaction. Fetching everything up front is what makes that atomicity
/// possible - a failure part-way through leaves no trace in the database.
pub async fn fetch_show(
    tmdb: &TmdbClient,
    tmdb_id: i64,
    api_key: &str,
) -> Result<FetchedShow, TmdbError> {
    // Destructured rather than borrowed: the season summaries have to be
    // owned by the futures below, or the closure can't satisfy the
    // higher-ranked lifetime `buffered` asks for.
    let TmdbShow {
        id,
        name,
        overview,
        poster_path,
        backdrop_path,
        status,
        seasons,
    } = tmdb.get_show(tmdb_id, api_key).await?;

    // `buffered`, not `buffer_unordered`: the concurrency is the same, and
    // keeping TMDB's season order makes the result deterministic.
    let seasons: Vec<FetchedSeason> = stream::iter(seasons)
        .map(|summary| async move {
            let detail = tmdb
                .get_season(tmdb_id, summary.season_number, api_key)
                .await?;
            Ok::<_, TmdbError>(FetchedSeason {
                number: summary.season_number,
                name: summary.name,
                episode_count: summary.episode_count,
                episodes: detail
                    .episodes
                    .into_iter()
                    .map(|e| FetchedEpisode {
                        number: e.episode_number,
                        name: e.name,
                        air_date: e.air_date,
                    })
                    .collect(),
            })
        })
        .buffered(SEASON_CONCURRENCY)
        .try_collect()
        .await?;

    Ok(FetchedShow {
        tmdb_id: id,
        name,
        overview,
        poster_path,
        backdrop_path,
        tmdb_status: status,
        seasons,
    })
}
