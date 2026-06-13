//! Serializable data structures shared between the database layer and the HTTP
//! routes.

use serde::{Deserialize, Serialize};

/// A user account as exposed to clients (never includes the password hash).
#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub created_at: i64,
}

/// Internal row including the password hash; used only inside the auth flow.
#[derive(Debug, Clone)]
pub struct UserWithHash {
    pub id: i64,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub created_at: i64,
}

impl From<UserWithHash> for User {
    fn from(u: UserWithHash) -> Self {
        User {
            id: u.id,
            username: u.username,
            email: u.email,
            created_at: u.created_at,
        }
    }
}

/// Image metadata returned by list endpoints (no binary payload).
#[derive(Debug, Clone, Serialize)]
pub struct ImageMeta {
    pub id: i64,
    pub name: String,
    pub mime: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub size: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Full image record including the binary payload.
#[derive(Debug, Clone)]
pub struct ImageRecord {
    pub meta: ImageMeta,
    pub data: Vec<u8>,
}

// --- request bodies --------------------------------------------------------

/// Registration request.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    #[serde(default)]
    pub email: Option<String>,
    pub password: String,
}

/// OAuth2 "password" grant request. `grant_type` is accepted for spec
/// compatibility but defaults to `password`.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    #[serde(default = "default_grant_type")]
    pub grant_type: String,
    pub username: String,
    pub password: String,
}

fn default_grant_type() -> String {
    "password".to_string()
}

/// Create or import an image. `data_url` is a `data:<mime>;base64,<payload>`
/// string (as produced by `canvas.toDataURL()`), or raw base64.
#[derive(Debug, Deserialize)]
pub struct CreateImageRequest {
    pub name: String,
    pub data_url: String,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
}

/// Update an existing image. Any omitted field is left unchanged.
#[derive(Debug, Deserialize)]
pub struct UpdateImageRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub data_url: Option<String>,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
}

/// Per-user editor state blob (opaque JSON owned by the frontend).
#[derive(Debug, Deserialize, Serialize)]
pub struct StateBody {
    pub state: serde_json::Value,
}

// --- response bodies -------------------------------------------------------

/// OAuth2-style token response returned by register and login.
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: User,
}
