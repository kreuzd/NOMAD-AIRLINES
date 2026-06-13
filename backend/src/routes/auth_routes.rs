//! Authentication routes: register, login (OAuth2 password grant), and `me`.

use axum::extract::State;
use axum::Json;

use crate::auth::{hash_password, verify_password, AuthUser};
use crate::error::{AppError, AppResult};
use crate::models::{LoginRequest, RegisterRequest, TokenResponse, User};
use crate::AppState;

/// `POST /api/auth/register` — create a local account and return a token.
pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Json<TokenResponse>> {
    validate_username(&req.username)?;
    validate_password(&req.password)?;
    let email = normalize_email(req.email)?;

    let hash = hash_password(&req.password)?;
    let user = state
        .db
        .create_user(&req.username, email.as_deref(), &hash)
        .map_err(|e| match e {
            // Make the conflict message specific to registration.
            AppError::Conflict(_) => AppError::Conflict("username or email already taken".into()),
            other => other,
        })?;

    issue(&state, user)
}

/// `POST /api/auth/token` (and `/login`) — OAuth2 "password" grant.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<TokenResponse>> {
    if req.grant_type != "password" {
        return Err(AppError::BadRequest(
            "unsupported grant_type (only 'password')".into(),
        ));
    }
    let found = state.db.find_user_by_username(&req.username)?;
    // Always run verification (even on missing user) to keep timing uniform.
    let user = match found {
        Some(u) if verify_password(&req.password, &u.password_hash) => u,
        Some(_) | None => {
            return Err(AppError::Unauthorized(
                "invalid username or password".into(),
            ))
        }
    };
    issue(&state, user.into())
}

/// `GET /api/auth/me` — return the authenticated user.
pub async fn me(user: AuthUser, State(state): State<AppState>) -> AppResult<Json<User>> {
    let u = state
        .db
        .find_user_by_id(user.id)?
        .ok_or_else(|| AppError::NotFound("user no longer exists".into()))?;
    Ok(Json(u))
}

fn issue(state: &AppState, user: User) -> AppResult<Json<TokenResponse>> {
    let access_token = state.tokens.issue(user.id, &user.username)?;
    Ok(Json(TokenResponse {
        access_token,
        token_type: "Bearer".to_string(),
        expires_in: state.tokens.expiry_secs(),
        user,
    }))
}

// --- validation ------------------------------------------------------------

fn validate_username(username: &str) -> AppResult<()> {
    let len = username.chars().count();
    if !(3..=32).contains(&len) {
        return Err(AppError::BadRequest(
            "username must be 3-32 characters".into(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        return Err(AppError::BadRequest(
            "username may contain only letters, digits, '_', '.', '-'".into(),
        ));
    }
    Ok(())
}

fn validate_password(password: &str) -> AppResult<()> {
    if password.chars().count() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    Ok(())
}

fn normalize_email(email: Option<String>) -> AppResult<Option<String>> {
    match email {
        None => Ok(None),
        Some(e) if e.trim().is_empty() => Ok(None),
        Some(e) => {
            let e = e.trim();
            // Deliberately lenient: a single '@' with non-empty parts.
            let ok = e
                .split_once('@')
                .map(|(a, b)| !a.is_empty() && b.contains('.') && !b.starts_with('.'))
                .unwrap_or(false);
            if !ok {
                return Err(AppError::BadRequest("invalid email address".into()));
            }
            Ok(Some(e.to_string()))
        }
    }
}
