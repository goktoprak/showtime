//! Everything that reads or writes tracked Shows, Seasons, Episodes and
//! Category. Takes a pool, opens its own transactions, and knows nothing
//! about HTTP or TMDB.
//!
//! Not calling TMDB is the load-bearing constraint: it means every operation
//! here is exercisable against an in-memory database, which is what makes the
//! tests below possible at all.

mod episodes;
mod settings;
mod shows;
mod status;

#[cfg(test)]
mod tests;

pub use episodes::{mark_season, mark_show, toggle_episode};
pub use settings::{api_key, clear_api_key, set_api_key, snapshot};
pub use shows::{apply_refresh, delete, detail, insert_show, is_tracked, list, tmdb_id_of};

/// A catalog failure, in the catalog's own terms. `AppError` owns the
/// translation to a status code.
#[derive(Debug)]
pub enum CatalogError {
    NotFound(&'static str),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for CatalogError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::NotFound(m) => f.write_str(m),
            CatalogError::Db(e) => write!(f, "{e}"),
        }
    }
}

pub type Result<T> = std::result::Result<T, CatalogError>;

/// Show metadata already fetched from TMDB, in the shape the catalog wants to
/// write. Keeping this separate from `tmdb::TmdbShow` is what stops the
/// catalog depending on the wire format of a third party.
#[derive(Debug, Clone)]
pub struct FetchedShow {
    pub tmdb_id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub tmdb_status: Option<String>,
    pub seasons: Vec<FetchedSeason>,
}

#[derive(Debug, Clone)]
pub struct FetchedSeason {
    pub number: i64,
    pub name: Option<String>,
    pub episode_count: i64,
    pub episodes: Vec<FetchedEpisode>,
}

#[derive(Debug, Clone)]
pub struct FetchedEpisode {
    pub number: i64,
    pub name: Option<String>,
    pub air_date: Option<String>,
}
