//! NOMAD Airlines backend library.
//!
//! A single axum application that provides the gallery API (local accounts +
//! JWT auth, SQLite-backed image CRUD, per-user editor state) and serves the
//! static frontend (vendored jspaint + the NOMAD gallery). The exact same
//! binary backs all three targets:
//!
//! * **Docker / web** — run `nomad-backend` directly; it binds a TCP port and
//!   serves the frontend.
//! * **Desktop / Android** — the Tauri shell ([`../src-tauri`]) spawns
//!   [`serve`] on a localhost port in-process and points its webview at it.
//!
//! Keeping one HTTP API path (rather than mixing Tauri IPC and HTTP) means the
//! frontend code is identical everywhere.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod util;

use std::path::Path;

use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::auth::TokenManager;
use crate::config::Config;
use crate::db::Db;
use crate::error::AppResult;

/// Shared, cloneable application state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub tokens: TokenManager,
    pub max_image_bytes: usize,
}

impl AppState {
    /// Build state from configuration, opening the database.
    pub fn from_config(config: &Config) -> AppResult<Self> {
        let db = Db::open(&config.database_path)?;
        Ok(AppState {
            db,
            tokens: TokenManager::new(&config.jwt_secret, config.jwt_expiry_secs),
            max_image_bytes: config.max_image_bytes,
        })
    }

    /// Build state with an in-memory database (handy for tests).
    pub fn in_memory(jwt_secret: &[u8]) -> AppResult<Self> {
        Ok(AppState {
            db: Db::open_in_memory()?,
            tokens: TokenManager::new(jwt_secret, 3600),
            max_image_bytes: 16 * 1024 * 1024,
        })
    }
}

/// Build the full application: `/api/*` routes plus static serving of the
/// frontend directory (with SPA-style fallback to `index.html`).
pub fn build_router(state: AppState, frontend_dir: &str) -> Router {
    let index = Path::new(frontend_dir).join("index.html");
    let static_service = ServeDir::new(frontend_dir).fallback(ServeFile::new(index));

    Router::new()
        .nest("/api", routes::api_router())
        .fallback_service(static_service)
        .layer(TraceLayer::new_for_http())
        // Permissive CORS lets the frontend be served from a separate origin
        // during development; in the packaged app it is same-origin anyway.
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Boot the server with the given configuration. Blocks until shutdown.
pub async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = AppState::from_config(&config)?;
    let app = build_router(state, &config.frontend_dir);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    let addr = listener.local_addr()?;
    tracing::info!(
        "NOMAD backend listening on http://{addr} (frontend: {})",
        config.frontend_dir
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Convenience entry point used by the `nomad-backend` binary: reads config
/// from the environment and serves.
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    serve(Config::from_env()).await
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}
