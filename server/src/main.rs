mod db;
mod handlers;
mod status;
mod tmdb;

use axum::{
    http::{header, HeaderValue, Response},
    routing::{any, get, post},
    Router,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub tmdb: Arc<tmdb::TmdbClient>,
}

/// Everything trunk emits into dist/ except index.html carries a content
/// hash in its filename, so it can be cached indefinitely - the hash is the
/// cache-buster. index.html itself must never be cached, or a client would
/// go on loading the previous build's asset names. Keying off content type
/// rather than filename also covers the SPA fallback, which serves
/// index.html under an arbitrary path.
fn cache_control_for_asset<B>(res: &Response<B>) -> Option<HeaderValue> {
    let is_html = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));

    Some(HeaderValue::from_static(if is_html {
        "no-store"
    } else {
        "public, max-age=31536000, immutable"
    }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let db_path = std::env::var("SHOWTIME_DB").unwrap_or_else(|_| "showtime.db".to_string());
    let pool = db::init_pool(&db_path).await?;

    let state = AppState {
        pool,
        tmdb: Arc::new(tmdb::TmdbClient::new()),
    };

    let api_routes = Router::new()
        .route("/settings", get(handlers::get_settings))
        .route(
            "/settings/apikey",
            post(handlers::set_api_key).delete(handlers::delete_api_key),
        )
        .route("/export", get(handlers::export_data))
        .route("/shows", get(handlers::list_shows).post(handlers::add_show))
        .route(
            "/shows/:id",
            get(handlers::get_show_detail).delete(handlers::delete_show),
        )
        .route("/shows/:id/refresh", post(handlers::refresh_show))
        .route("/shows/refresh-all", post(handlers::refresh_all_shows))
        .route("/shows/:id/mark-watched", post(handlers::mark_show_watched))
        .route(
            "/seasons/:id/mark-watched",
            post(handlers::mark_season_watched),
        )
        .route(
            "/episodes/:id/toggle",
            post(handlers::toggle_episode_watched),
        )
        .fallback(handlers::api_not_found)
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state);

    // Unknown paths fall back to index.html so the client-side router can
    // resolve them; everything else in dist/ is served directly. Wrapped in
    // its own Router purely so the cache-control layer applies to the static
    // assets without also reaching the API.
    let static_routes = Router::new()
        // `fallback`, not `not_found_service`: the latter wraps the file
        // service in SetStatus and forces a 404, which would mean every
        // valid client route answered index.html with a 404 status.
        .fallback_service(ServeDir::new("dist").fallback(ServeFile::new("dist/index.html")))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            cache_control_for_asset,
        ));

    let app = Router::new()
        .nest("/api", api_routes)
        // Axum's `nest` does not route the exact path "/api/" into the
        // nested router, so without this it would reach the SPA fallback
        // and answer a bad API path with HTML and a 200.
        .route("/api/", any(handlers::api_not_found))
        .fallback_service(static_routes)
        .layer(TraceLayer::new_for_http());

    let bind_addr = std::env::var("SHOWTIME_BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("ShowTime running at http://{bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
