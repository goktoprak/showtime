use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Show {
    pub id: i64,
    pub tmdb_id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub tmdb_status: Option<String>,
    pub status: String,
    pub added_at: String,
    pub last_refreshed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Season {
    pub id: i64,
    pub show_id: i64,
    pub tmdb_season_number: i64,
    pub name: Option<String>,
    pub episode_count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Episode {
    pub id: i64,
    pub season_id: i64,
    pub tmdb_episode_number: i64,
    pub name: Option<String>,
    pub air_date: Option<String>,
    pub watched: bool,
    pub watched_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SeasonWithEpisodes {
    pub season: Season,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Serialize)]
pub struct ShowDetail {
    #[serde(flatten)]
    pub show: Show,
    pub seasons: Vec<SeasonWithEpisodes>,
}

#[derive(Debug, Deserialize)]
pub struct AddShowRequest {
    pub tmdb_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct SetApiKeyRequest {
    pub api_key: String,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub has_api_key: bool,
}

/// Maps a raw TMDB show status string into whether the show is still
/// actively producing new content ("airing") or is done.
pub fn tmdb_status_is_airing(tmdb_status: &str) -> bool {
    matches!(
        tmdb_status,
        "Returning Series" | "In Production" | "Planned" | "Pilot"
    )
}
