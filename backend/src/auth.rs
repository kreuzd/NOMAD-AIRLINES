//! Authentication: argon2 password hashing and JWT (OAuth2 "password" grant).
//!
//! Accounts are local (username + password). On register/login the backend
//! issues a signed JWT access token; protected routes require it via the
//! `Authorization: Bearer <token>` header, extracted by [`AuthUser`].

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::config::now_unix;
use crate::error::AppError;
use crate::AppState;

/// Hash a plaintext password with argon2id. Returns the PHC string to store.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let mut salt_bytes = [0u8; 16];
    getrandom::fill(&mut salt_bytes)
        .map_err(|e| AppError::Internal(format!("rng unavailable: {e}")))?;
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| AppError::Internal(format!("salt encoding failed: {e}")))?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
}

/// Verify a plaintext password against a stored PHC hash. Returns `false` for
/// any mismatch or malformed hash rather than erroring, to avoid leaking which
/// half failed.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// JWT claims. `sub` holds the user id.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: i64,
    pub username: String,
    pub iat: i64,
    pub exp: i64,
}

/// Signs and verifies access tokens.
#[derive(Clone)]
pub struct TokenManager {
    encoding: EncodingKey,
    decoding: DecodingKey,
    expiry_secs: i64,
}

impl TokenManager {
    pub fn new(secret: &[u8], expiry_secs: i64) -> Self {
        TokenManager {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            expiry_secs,
        }
    }

    pub fn expiry_secs(&self) -> i64 {
        self.expiry_secs
    }

    /// Issue a signed token for a user.
    pub fn issue(&self, user_id: i64, username: &str) -> Result<String, AppError> {
        let iat = now_unix();
        let claims = Claims {
            sub: user_id,
            username: username.to_string(),
            iat,
            exp: iat + self.expiry_secs,
        };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(|e| AppError::Internal(format!("token signing failed: {e}")))
    }

    /// Verify a token's signature and expiry, returning its claims.
    pub fn verify(&self, token: &str) -> Result<Claims, AppError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        decode::<Claims>(token, &self.decoding, &validation)
            .map(|data| data.claims)
            .map_err(|_| AppError::Unauthorized("invalid or expired token".into()))
    }
}

/// Axum extractor that resolves the authenticated user from the
/// `Authorization: Bearer <token>` header. Reject with 401 if absent/invalid.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing Authorization header".into()))?;

        let token = header
            .strip_prefix("Bearer ")
            .or_else(|| header.strip_prefix("bearer "))
            .ok_or_else(|| AppError::Unauthorized("expected Bearer token".into()))?
            .trim();

        let claims = state.tokens.verify(token)?;
        Ok(AuthUser {
            id: claims.sub,
            username: claims.username,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_round_trip() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn distinct_salts_produce_distinct_hashes() {
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert_ne!(a, b, "argon2 should salt each hash uniquely");
        assert!(verify_password("same", &a));
        assert!(verify_password("same", &b));
    }

    #[test]
    fn malformed_hash_is_rejected_not_panicking() {
        assert!(!verify_password("anything", "not-a-phc-string"));
    }

    #[test]
    fn token_issue_and_verify() {
        let tm = TokenManager::new(b"test-secret", 3600);
        let token = tm.issue(42, "alice").unwrap();
        let claims = tm.verify(&token).unwrap();
        assert_eq!(claims.sub, 42);
        assert_eq!(claims.username, "alice");
    }

    #[test]
    fn token_signed_with_other_secret_is_rejected() {
        let issuer = TokenManager::new(b"secret-A", 3600);
        let verifier = TokenManager::new(b"secret-B", 3600);
        let token = issuer.issue(1, "bob").unwrap();
        assert!(verifier.verify(&token).is_err());
    }

    #[test]
    fn expired_token_is_rejected() {
        // Expire well beyond jsonwebtoken's default 60s clock-skew leeway.
        let tm = TokenManager::new(b"secret", -3600);
        let token = tm.issue(1, "carol").unwrap();
        assert!(tm.verify(&token).is_err());
    }
}
