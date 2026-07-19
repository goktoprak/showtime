use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::json;

use crate::{
    db,
    models::{
        AddShowRequest, Episode, Season, SeasonWithEpisodes, SetApiKeyRequest, SettingsResponse,
        Show, ShowDetail,
    },
    status::recompute_show_status,
    AppState,
};

type ApiResult<T> = Result<T, (StatusCode, Json<serde_json::Value>)>;

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(json!({ "error": msg.into() })))
}

// ---------- settings ----------

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<SettingsResponse>> {
    let key = db::get_api_key(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(SettingsResponse {
        has_api_key: key.is_some(),
        masked_key: key.as_deref().map(crate::models::mask_api_key),
    }))
}

pub async fn set_api_key(
    State(state): State<AppState>,
    Json(req): Json<SetApiKeyRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let key = req.api_key.trim();
    if key.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "API key cannot be empty"));
    }

    state.tmdb.validate_key(key).await.map_err(|e| {
        err(
            StatusCode::BAD_REQUEST,
            format!("TMDB rejected this key: {e}"),
        )
    })?;

    sqlx::query("UPDATE settings SET tmdb_api_key = ? WHERE id = 1")
        .bind(key)
        .execute(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

/// Streams a consistent snapshot of the SQLite database (via `VACUUM INTO`,
/// so it's safe even if WAL has uncheckpointed writes) as a downloadable
/// file, for use as a manual backup.
pub async fn export_data(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let tmp_path = std::env::temp_dir().join(format!(
        "showtime-export-{}.db",
        Utc::now().timestamp_micros()
    ));
    let tmp_path_str = tmp_path.to_string_lossy().to_string();

    sqlx::query("VACUUM INTO ?")
        .bind(&tmp_path_str)
        .execute(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let bytes = tokio::fs::read(&tmp_path)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let _ = tokio::fs::remove_file(&tmp_path).await;

    let filename = format!("showtime-backup-{}.db", Utc::now().format("%Y%m%d-%H%M%S"));
    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream".to_string()),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        ),
    ];

    Ok((headers, bytes))
}

pub async fn delete_api_key(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    sqlx::query("UPDATE settings SET tmdb_api_key = NULL WHERE id = 1")
        .execute(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

// ---------- shows ----------

pub async fn list_shows(State(state): State<AppState>) -> ApiResult<Json<Vec<Show>>> {
    let shows = sqlx::query_as::<_, Show>("SELECT * FROM shows ORDER BY name COLLATE NOCASE ASC")
        .fetch_all(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(shows))
}

pub async fn get_show_detail(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<Json<ShowDetail>> {
    let show = sqlx::query_as::<_, Show>("SELECT * FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "show not found"))?;

    let seasons = sqlx::query_as::<_, Season>(
        "SELECT * FROM seasons WHERE show_id = ? ORDER BY tmdb_season_number ASC",
    )
    .bind(show_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut seasons_with_episodes = Vec::with_capacity(seasons.len());
    for season in seasons {
        let episodes = sqlx::query_as::<_, Episode>(
            "SELECT * FROM episodes WHERE season_id = ? ORDER BY tmdb_episode_number ASC",
        )
        .bind(season.id)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        seasons_with_episodes.push(SeasonWithEpisodes { season, episodes });
    }

    Ok(Json(ShowDetail {
        show,
        seasons: seasons_with_episodes,
    }))
}

pub async fn add_show(
    State(state): State<AppState>,
    Json(req): Json<AddShowRequest>,
) -> ApiResult<Json<Show>> {
    let api_key = db::get_api_key(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::BAD_REQUEST,
                "no TMDB API key set - add one in Settings first",
            )
        })?;

    let existing: Option<i64> = sqlx::query_scalar("SELECT id FROM shows WHERE tmdb_id = ?")
        .bind(req.tmdb_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if existing.is_some() {
        return Err(err(StatusCode::CONFLICT, "show already tracked"));
    }

    let tmdb_show = state
        .tmdb
        .get_show(req.tmdb_id, &api_key)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("TMDB error: {e}")))?;

    let now = Utc::now().to_rfc3339();

    let show_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO shows (tmdb_id, name, overview, poster_path, backdrop_path, tmdb_status, status, added_at, last_refreshed_at)
         VALUES (?, ?, ?, ?, ?, ?, 'watchlist', ?, ?)
         RETURNING id",
    )
    .bind(tmdb_show.id)
    .bind(&tmdb_show.name)
    .bind(&tmdb_show.overview)
    .bind(&tmdb_show.poster_path)
    .bind(&tmdb_show.backdrop_path)
    .bind(&tmdb_show.status)
    .bind(&now)
    .bind(&now)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    fetch_and_store_seasons(&state, show_id, req.tmdb_id, &tmdb_show, &api_key).await?;

    recompute_show_status(&state.pool, show_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let show = sqlx::query_as::<_, Show>("SELECT * FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(show))
}

pub async fn refresh_show(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<Json<Show>> {
    let api_key = db::get_api_key(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "no TMDB API key set"))?;

    let show = refresh_show_by_id(&state, show_id, &api_key).await?;
    Ok(Json(show))
}

pub async fn refresh_all_shows(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let api_key = db::get_api_key(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "no TMDB API key set"))?;

    let show_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM shows")
        .fetch_all(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut refreshed = 0;
    let mut failed = 0;
    for show_id in show_ids {
        match refresh_show_by_id(&state, show_id, &api_key).await {
            Ok(_) => refreshed += 1,
            Err(_) => failed += 1,
        }
    }

    Ok(Json(json!({ "refreshed": refreshed, "failed": failed })))
}

/// Re-fetches a single show (plus all its seasons/episodes) from TMDB and
/// recomputes its category. Shared by both the single-show and
/// refresh-all-shows endpoints.
async fn refresh_show_by_id(state: &AppState, show_id: i64, api_key: &str) -> ApiResult<Show> {
    let tmdb_id: i64 = sqlx::query_scalar("SELECT tmdb_id FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "show not found"))?;

    let tmdb_show = state
        .tmdb
        .get_show(tmdb_id, api_key)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("TMDB error: {e}")))?;

    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE shows SET name = ?, overview = ?, poster_path = ?, backdrop_path = ?, tmdb_status = ?, last_refreshed_at = ?
         WHERE id = ?",
    )
    .bind(&tmdb_show.name)
    .bind(&tmdb_show.overview)
    .bind(&tmdb_show.poster_path)
    .bind(&tmdb_show.backdrop_path)
    .bind(&tmdb_show.status)
    .bind(&now)
    .bind(show_id)
    .execute(&state.pool)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    fetch_and_store_seasons(state, show_id, tmdb_id, &tmdb_show, api_key).await?;

    recompute_show_status(&state.pool, show_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let show = sqlx::query_as::<_, Show>("SELECT * FROM shows WHERE id = ?")
        .bind(show_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(show)
}

pub async fn delete_show(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<StatusCode> {
    sqlx::query("DELETE FROM shows WHERE id = ?")
        .bind(show_id)
        .execute(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Fetches season/episode detail for every season TMDB reports and upserts
/// them into the DB. Preserves existing `watched` flags for episodes that
/// already exist; new episodes default to unwatched.
async fn fetch_and_store_seasons(
    state: &AppState,
    show_id: i64,
    tmdb_id: i64,
    tmdb_show: &crate::tmdb::TmdbShow,
    api_key: &str,
) -> ApiResult<()> {
    for season_summary in &tmdb_show.seasons {
        // TMDB includes "specials" as season_number 0; skip if you don't
        // want those tracked. Here we keep them for completeness.
        let season_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO seasons (show_id, tmdb_season_number, name, episode_count)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(show_id, tmdb_season_number)
             DO UPDATE SET name = excluded.name, episode_count = excluded.episode_count
             RETURNING id",
        )
        .bind(show_id)
        .bind(season_summary.season_number)
        .bind(&season_summary.name)
        .bind(season_summary.episode_count)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let season_detail = state
            .tmdb
            .get_season(tmdb_id, season_summary.season_number, api_key)
            .await
            .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("TMDB error: {e}")))?;

        for ep in &season_detail.episodes {
            sqlx::query(
                "INSERT INTO episodes (season_id, tmdb_episode_number, name, air_date, watched)
                 VALUES (?, ?, ?, ?, 0)
                 ON CONFLICT(season_id, tmdb_episode_number)
                 DO UPDATE SET name = excluded.name, air_date = excluded.air_date",
            )
            .bind(season_id)
            .bind(ep.episode_number)
            .bind(&ep.name)
            .bind(&ep.air_date)
            .execute(&state.pool)
            .await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
    }

    Ok(())
}

// ---------- episodes ----------

pub async fn toggle_episode_watched(
    State(state): State<AppState>,
    Path(episode_id): Path<i64>,
) -> ApiResult<Json<Episode>> {
    let current: bool = sqlx::query_scalar("SELECT watched FROM episodes WHERE id = ?")
        .bind(episode_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "episode not found"))?;

    let new_watched = !current;
    let watched_at = if new_watched {
        Some(Utc::now().to_rfc3339())
    } else {
        None
    };

    sqlx::query("UPDATE episodes SET watched = ?, watched_at = ? WHERE id = ?")
        .bind(new_watched)
        .bind(&watched_at)
        .bind(episode_id)
        .execute(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let show_id: i64 = sqlx::query_scalar(
        "SELECT s.show_id FROM seasons s
         JOIN episodes e ON e.season_id = s.id
         WHERE e.id = ?",
    )
    .bind(episode_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    recompute_show_status(&state.pool, show_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let episode = sqlx::query_as::<_, Episode>("SELECT * FROM episodes WHERE id = ?")
        .bind(episode_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(episode))
}

pub async fn mark_season_watched(
    State(state): State<AppState>,
    Path(season_id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let now = Utc::now().to_rfc3339();

    sqlx::query("UPDATE episodes SET watched = 1, watched_at = ? WHERE season_id = ?")
        .bind(&now)
        .bind(season_id)
        .execute(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let show_id: i64 = sqlx::query_scalar("SELECT show_id FROM seasons WHERE id = ?")
        .bind(season_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    recompute_show_status(&state.pool, show_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn mark_show_watched(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE episodes SET watched = 1, watched_at = ?
         WHERE season_id IN (SELECT id FROM seasons WHERE show_id = ?)",
    )
    .bind(&now)
    .bind(show_id)
    .execute(&state.pool)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    recompute_show_status(&state.pool, show_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "ok": true })))
}
