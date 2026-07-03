mod db;
mod handlers;
mod models;
mod status;
mod tmdb;

use axum::{
    http::{header, HeaderValue},
    routing::{get, post},
    Router,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::{
    cors::CorsLayer, services::ServeDir, set_header::SetResponseHeaderLayer, trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub tmdb: Arc<tmdb::TmdbClient>,
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
        .route("/settings/apikey", post(handlers::set_api_key))
        .route(
            "/shows",
            get(handlers::list_shows).post(handlers::add_show),
        )
        .route(
            "/shows/:id",
            get(handlers::get_show_detail).delete(handlers::delete_show),
        )
        .route("/shows/:id/refresh", post(handlers::refresh_show))
        .route("/shows/:id/mark-watched", post(handlers::mark_show_watched))
        .route("/seasons/:id/mark-watched", post(handlers::mark_season_watched))
        .route("/episodes/:id/toggle", post(handlers::toggle_episode_watched))
        .with_state(state);

    let static_service = ServeDir::new("static");

    let app = Router::new()
        .nest("/api", api_routes)
        .fallback_service(static_service)
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let bind_addr = std::env::var("SHOWTIME_BIND").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("ShowTime running at http://{bind_addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
