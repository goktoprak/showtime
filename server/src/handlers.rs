//! HTTP adapters over the catalog. Each handler extracts its arguments, calls
//! one catalog operation, and lets `?` turn a failure into a response - the
//! mapping lives in `error.rs`, the SQL lives in `catalog`.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use serde_json::json;

use shared::{
    AddShowRequest, RefreshAllResponse, SetApiKeyRequest, SettingsResponse, Show, ShowDetail,
};

use crate::{catalog, error::AppError, sync, AppState};

type ApiResult<T> = Result<T, AppError>;

/// The key must be present for anything that talks to TMDB.
async fn require_api_key(state: &AppState, missing: &'static str) -> ApiResult<String> {
    catalog::api_key(&state.pool)
        .await?
        .ok_or(AppError::BadRequest(missing.to_string()))
}

// ---------- settings ----------

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<SettingsResponse>> {
    let key = catalog::api_key(&state.pool).await?;
    Ok(Json(SettingsResponse {
        has_api_key: key.is_some(),
        masked_key: key.as_deref().map(shared::mask_api_key),
    }))
}

pub async fn set_api_key(
    State(state): State<AppState>,
    Json(req): Json<SetApiKeyRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let key = req.api_key.trim();
    if key.is_empty() {
        return Err(AppError::BadRequest("API key cannot be empty".into()));
    }

    // A key TMDB won't accept is the user's problem, not a bad gateway, so
    // this one site overrides the default `TmdbError` mapping.
    state
        .tmdb
        .validate_key(key)
        .await
        .map_err(|e| AppError::BadRequest(format!("TMDB rejected this key: {e}")))?;

    catalog::set_api_key(&state.pool, key).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_api_key(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    catalog::clear_api_key(&state.pool).await?;
    Ok(Json(json!({ "ok": true })))
}

/// Serves a consistent snapshot of the database as a downloadable file, for
/// use as a manual backup.
pub async fn export_data(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let tmp_path = std::env::temp_dir().join(format!(
        "showtime-export-{}.db",
        Utc::now().timestamp_micros()
    ));
    let tmp_path_str = tmp_path.to_string_lossy().to_string();

    catalog::snapshot(&state.pool, &tmp_path_str).await?;

    // Remove the snapshot before propagating a read failure, or a failed
    // export would leave the copy behind in the temp dir for good.
    let read = tokio::fs::read(&tmp_path).await;
    let _ = tokio::fs::remove_file(&tmp_path).await;
    let bytes = read.map_err(|e| AppError::Internal(e.to_string()))?;

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

// ---------- shows ----------

pub async fn list_shows(State(state): State<AppState>) -> ApiResult<Json<Vec<Show>>> {
    Ok(Json(catalog::list(&state.pool).await?))
}

pub async fn get_show_detail(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<Json<ShowDetail>> {
    Ok(Json(catalog::detail(&state.pool, show_id).await?))
}

pub async fn add_show(
    State(state): State<AppState>,
    Json(req): Json<AddShowRequest>,
) -> ApiResult<Json<Show>> {
    let api_key =
        require_api_key(&state, "no TMDB API key set - add one in Settings first").await?;

    if catalog::is_tracked(&state.pool, req.tmdb_id).await? {
        return Err(AppError::Conflict("show already tracked".into()));
    }

    // Fetch everything before writing anything: a TMDB failure here must not
    // leave a tracked-but-empty show that can never be re-added.
    let fetched = sync::fetch_show(&state.tmdb, req.tmdb_id, &api_key).await?;

    Ok(Json(catalog::insert_show(&state.pool, &fetched).await?))
}

pub async fn refresh_show(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<Json<Show>> {
    let api_key = require_api_key(&state, "no TMDB API key set").await?;
    Ok(Json(refresh_one(&state, show_id, &api_key).await?))
}

pub async fn refresh_all_shows(
    State(state): State<AppState>,
) -> ApiResult<Json<RefreshAllResponse>> {
    let api_key = require_api_key(&state, "no TMDB API key set").await?;

    let mut refreshed = 0i64;
    let mut failed = 0i64;
    // Sequential across shows on purpose: each refresh already fetches its
    // seasons concurrently, and TMDB should not see the product of the two.
    for show in catalog::list(&state.pool).await? {
        match refresh_one(&state, show.id, &api_key).await {
            Ok(_) => refreshed += 1,
            Err(_) => failed += 1,
        }
    }

    Ok(Json(RefreshAllResponse { refreshed, failed }))
}

/// Shared by the single-show and refresh-all endpoints.
async fn refresh_one(state: &AppState, show_id: i64, api_key: &str) -> ApiResult<Show> {
    let tmdb_id = catalog::tmdb_id_of(&state.pool, show_id).await?;
    let fetched = sync::fetch_show(&state.tmdb, tmdb_id, api_key).await?;
    Ok(catalog::apply_refresh(&state.pool, show_id, &fetched).await?)
}

pub async fn delete_show(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<StatusCode> {
    catalog::delete(&state.pool, show_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------- episodes ----------

// All three mutation endpoints below return the show row rather than the
// thing they mutated. The client updates checkboxes optimistically - it
// already knows what it clicked - but it cannot derive the show's Category
// locally, because that rule excludes specials and depends on the raw TMDB
// production status. Returning the recomputed row keeps the rule in one
// place instead of mirroring it in the frontend.

pub async fn toggle_episode_watched(
    State(state): State<AppState>,
    Path(episode_id): Path<i64>,
) -> ApiResult<Json<Show>> {
    Ok(Json(
        catalog::toggle_episode(&state.pool, episode_id).await?,
    ))
}

pub async fn mark_season_watched(
    State(state): State<AppState>,
    Path(season_id): Path<i64>,
) -> ApiResult<Json<Show>> {
    Ok(Json(catalog::mark_season(&state.pool, season_id).await?))
}

pub async fn mark_show_watched(
    State(state): State<AppState>,
    Path(show_id): Path<i64>,
) -> ApiResult<Json<Show>> {
    Ok(Json(catalog::mark_show(&state.pool, show_id).await?))
}

// ---------- fallback ----------

/// Catch-all for unmatched paths under `/api`. Without this, a typo'd
/// endpoint falls through to the outer static-file fallback, which serves
/// index.html - so a bad API path would answer with HTML and a 200.
pub async fn api_not_found() -> AppError {
    AppError::NotFound("no such API endpoint")
}
