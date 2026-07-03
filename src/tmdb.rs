use serde::Deserialize;

const BASE_URL: &str = "https://api.themoviedb.org/3";

// Image base URL is used on the frontend (static/app.js) to build poster
// and backdrop URLs directly from the path fragments TMDB returns, so it's
// not needed here on the backend.

#[derive(Debug, thiserror::Error)]
pub enum TmdbError {
    #[error("TMDB request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("TMDB returned an error status: {0}")]
    Status(String),
}

#[derive(Debug, Deserialize)]
pub struct TmdbShow {
    pub id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub status: Option<String>,
    pub seasons: Vec<TmdbSeasonSummary>,
}

#[derive(Debug, Deserialize)]
pub struct TmdbSeasonSummary {
    pub season_number: i64,
    pub name: Option<String>,
    pub episode_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct TmdbSeasonDetail {
    pub episodes: Vec<TmdbEpisode>,
}

#[derive(Debug, Deserialize)]
pub struct TmdbEpisode {
    pub episode_number: i64,
    pub name: Option<String>,
    pub air_date: Option<String>,
}

pub struct TmdbClient {
    client: reqwest::Client,
}

impl TmdbClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        api_key: &str,
    ) -> Result<T, TmdbError> {
        let url = format!("{BASE_URL}{path}");
        let resp = self
            .client
            .get(&url)
            .query(&[("api_key", api_key)])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(TmdbError::Status(format!("{status}: {body}")));
        }

        Ok(resp.json::<T>().await?)
    }

    pub async fn get_show(&self, tmdb_id: i64, api_key: &str) -> Result<TmdbShow, TmdbError> {
        self.get(&format!("/tv/{tmdb_id}"), api_key).await
    }

    pub async fn get_season(
        &self,
        tmdb_id: i64,
        season_number: i64,
        api_key: &str,
    ) -> Result<TmdbSeasonDetail, TmdbError> {
        self.get(
            &format!("/tv/{tmdb_id}/season/{season_number}"),
            api_key,
        )
        .await
    }
}
