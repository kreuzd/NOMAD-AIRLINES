//! Runtime configuration, sourced from environment variables with sensible
//! defaults so the same binary runs unconfigured (desktop/Android via Tauri)
//! or fully configured (Docker / server).

use std::time::{SystemTime, UNIX_EPOCH};

/// Application configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// Address to bind the HTTP server to, e.g. `127.0.0.1:8787`.
    pub bind_addr: String,
    /// Path to the SQLite database file. `:memory:` uses an in-memory DB.
    pub database_path: String,
    /// Directory containing the built frontend (jspaint + gallery).
    pub frontend_dir: String,
    /// Secret used to sign JWTs. Generated randomly if unset (tokens then do
    /// not survive a restart, which is fine for desktop but should be set in
    /// production via `NOMAD_JWT_SECRET`).
    pub jwt_secret: Vec<u8>,
    /// Access-token lifetime in seconds.
    pub jwt_expiry_secs: i64,
    /// Max accepted image payload size in bytes (decoded).
    pub max_image_bytes: usize,
}

impl Config {
    /// Build configuration from the process environment.
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("NOMAD_JWT_SECRET")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.into_bytes())
            .unwrap_or_else(generate_secret);

        Config {
            bind_addr: env_or("NOMAD_BIND_ADDR", "127.0.0.1:8787"),
            database_path: env_or("NOMAD_DB_PATH", "nomad.db"),
            frontend_dir: env_or("NOMAD_FRONTEND_DIR", "../frontend"),
            jwt_secret,
            jwt_expiry_secs: env_or("NOMAD_JWT_EXPIRY_SECS", "86400")
                .parse()
                .unwrap_or(86_400),
            max_image_bytes: env_or("NOMAD_MAX_IMAGE_BYTES", "16777216")
                .parse()
                .unwrap_or(16 * 1024 * 1024),
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Generate a random secret from the OS CSPRNG for dev/desktop use when none
/// is configured. Production deployments should set `NOMAD_JWT_SECRET`
/// explicitly so tokens survive restarts and span multiple instances.
fn generate_secret() -> Vec<u8> {
    let mut buf = vec![0u8; 48];
    if getrandom::fill(&mut buf).is_err() {
        // Extremely unlikely OS RNG failure: fall back to a time-derived seed
        // so the process can still start (single-instance desktop use).
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        buf[..16].copy_from_slice(&nanos.to_le_bytes());
    }
    buf
}

/// Current unix time in seconds.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
