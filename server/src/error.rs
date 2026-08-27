//! The one place a failure becomes an HTTP response.
//!
//! Handlers propagate with `?` and never restate the mapping. The body shape
//! is `{"error": "..."}` because that is the key the frontend reads
//! (`web/src/api.rs::error_from`).

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::catalog::CatalogError;
use crate::tmdb::TmdbError;

#[derive(Debug)]
pub enum AppError {
    Db(sqlx::Error),
    /// A TMDB call that failed on TMDB's side. Distinct from a rejected API
    /// key, which is the user's problem and answers 400 - see `set_api_key`.
    Tmdb(TmdbError),
    NotFound(&'static str),
    BadRequest(String),
    Conflict(String),
    /// A server-side failure that isn't the database's fault - today only the
    /// filesystem step of a backup export.
    Internal(String),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

impl From<CatalogError> for AppError {
    fn from(e: CatalogError) -> Self {
        match e {
            CatalogError::NotFound(m) => Self::NotFound(m),
            CatalogError::Db(e) => Self::Db(e),
        }
    }
}

impl From<TmdbError> for AppError {
    fn from(e: TmdbError) -> Self {
        Self::Tmdb(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Db(e) => {
                // The response text is unchanged from before this type
                // existed; the log line is the new part.
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
            AppError::Tmdb(e) => (StatusCode::BAD_GATEWAY, format!("TMDB error: {e}")),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.to_string()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m),
            AppError::Internal(m) => {
                tracing::error!(error = %m, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, m)
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
