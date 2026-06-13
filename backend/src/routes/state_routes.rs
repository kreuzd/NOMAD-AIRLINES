//! Per-user editor state persistence, so a user returns to their work after
//! quitting the app. The state body is opaque JSON owned by the frontend
//! (e.g. the id of the image currently open, zoom level, a canvas snapshot).

use axum::extract::State;
use axum::Json;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::models::StateBody;
use crate::AppState;

/// `GET /api/state` — fetch the saved editor state, or `{ "state": null }`.
pub async fn get_state(
    user: AuthUser,
    State(state): State<AppState>,
) -> AppResult<Json<StateBody>> {
    let value = match state.db.get_state(user.id)? {
        Some(json) => serde_json::from_str(&json)
            .map_err(|e| AppError::Internal(format!("corrupt stored state: {e}")))?,
        None => serde_json::Value::Null,
    };
    Ok(Json(StateBody { state: value }))
}

/// `PUT /api/state` — replace the saved editor state.
pub async fn put_state(
    user: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<StateBody>,
) -> AppResult<Json<StateBody>> {
    let serialized = serde_json::to_string(&body.state)
        .map_err(|e| AppError::BadRequest(format!("state is not serializable: {e}")))?;
    state.db.set_state(user.id, &serialized)?;
    Ok(Json(body))
}
