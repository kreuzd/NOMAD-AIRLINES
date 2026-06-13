//! HTTP route assembly.

pub mod auth_routes;
pub mod image_routes;
pub mod state_routes;

use axum::routing::{get, post};
use axum::Router;

use crate::AppState;

/// Build the `/api` router (auth + gallery + state + health).
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth_routes::register))
        .route("/auth/token", post(auth_routes::login))
        .route("/auth/login", post(auth_routes::login))
        .route("/auth/me", get(auth_routes::me))
        .route(
            "/images",
            get(image_routes::list_images).post(image_routes::create_image),
        )
        .route(
            "/images/{id}",
            get(image_routes::get_image)
                .put(image_routes::update_image)
                .delete(image_routes::delete_image),
        )
        .route("/images/{id}/raw", get(image_routes::get_image_raw))
        .route(
            "/state",
            get(state_routes::get_state).put(state_routes::put_state),
        )
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok", "service": "nomad-backend" }))
}
