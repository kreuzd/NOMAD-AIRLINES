//! Image gallery routes. All routes are scoped to the authenticated user.

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use base64::Engine;
use serde_json::json;

use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::models::{CreateImageRequest, ImageMeta, UpdateImageRequest};
use crate::util::parse_data_url;
use crate::AppState;

/// `GET /api/images` — list the user's images (metadata only).
pub async fn list_images(
    user: AuthUser,
    State(state): State<AppState>,
) -> AppResult<Json<Vec<ImageMeta>>> {
    Ok(Json(state.db.list_images(user.id)?))
}

/// `POST /api/images` — create/import an image from a data URL.
pub async fn create_image(
    user: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateImageRequest>,
) -> AppResult<(StatusCode, Json<ImageMeta>)> {
    let name = clean_name(&req.name)?;
    let (mime, bytes) = parse_data_url(&req.data_url)?;
    enforce_size(&state, &bytes)?;

    let meta = state
        .db
        .create_image(user.id, &name, &mime, req.width, req.height, &bytes)?;
    Ok((StatusCode::CREATED, Json(meta)))
}

/// `GET /api/images/:id` — fetch one image as metadata + base64 data URL
/// (convenient for "export to device").
pub async fn get_image(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<serde_json::Value>> {
    let record = state
        .db
        .get_image(user.id, id)?
        .ok_or_else(|| AppError::NotFound("image not found".into()))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&record.data);
    let data_url = format!("data:{};base64,{}", record.meta.mime, b64);
    let mut value = serde_json::to_value(&record.meta)
        .map_err(|e| AppError::Internal(format!("serialize failed: {e}")))?;
    value["data_url"] = json!(data_url);
    Ok(Json(value))
}

/// `GET /api/images/:id/raw` — fetch the raw image bytes with its content type.
/// Used by the gallery to render `<img>` thumbnails via authenticated fetch.
pub async fn get_image_raw(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    let record = state
        .db
        .get_image(user.id, id)?
        .ok_or_else(|| AppError::NotFound("image not found".into()))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, record.meta.mime.clone()),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        record.data,
    ))
}

/// `PUT /api/images/:id` — rename and/or replace the payload.
pub async fn update_image(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateImageRequest>,
) -> AppResult<Json<ImageMeta>> {
    let name = match req.name {
        Some(n) => Some(clean_name(&n)?),
        None => None,
    };

    let decoded = match req.data_url {
        Some(ref d) => Some(parse_data_url(d)?),
        None => None,
    };
    if let Some((_, ref bytes)) = decoded {
        enforce_size(&state, bytes)?;
    }
    let (mime, bytes) = match decoded {
        Some((m, b)) => (Some(m), Some(b)),
        None => (None, None),
    };

    let meta = state
        .db
        .update_image(
            user.id,
            id,
            name.as_deref(),
            mime.as_deref(),
            req.width,
            req.height,
            bytes.as_deref(),
        )?
        .ok_or_else(|| AppError::NotFound("image not found".into()))?;
    Ok(Json(meta))
}

/// `DELETE /api/images/:id` — remove an image.
pub async fn delete_image(
    user: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<StatusCode> {
    if state.db.delete_image(user.id, id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("image not found".into()))
    }
}

// --- helpers ---------------------------------------------------------------

fn clean_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("image name is required".into()));
    }
    if name.chars().count() > 128 {
        return Err(AppError::BadRequest("image name too long (max 128)".into()));
    }
    Ok(name.to_string())
}

fn enforce_size(state: &AppState, bytes: &[u8]) -> AppResult<()> {
    if bytes.len() > state.max_image_bytes {
        return Err(AppError::BadRequest(format!(
            "image exceeds maximum size of {} bytes",
            state.max_image_bytes
        )));
    }
    Ok(())
}
