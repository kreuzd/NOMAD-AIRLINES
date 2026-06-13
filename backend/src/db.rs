//! SQLite persistence layer (rusqlite, bundled).
//!
//! A single connection is shared behind a mutex. SQLite serialises writes
//! anyway, and this app's concurrency is modest (one desktop user, or a small
//! Docker deployment), so a connection pool would be over-engineering. All
//! methods are synchronous and short-lived; handlers never hold the lock
//! across an `.await`.

use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};

use crate::config::now_unix;
use crate::error::{AppError, AppResult};
use crate::models::{ImageMeta, ImageRecord, User, UserWithHash};

/// Cloneable handle to the database.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Open (or create) a file-backed database and run migrations.
    pub fn open(path: &str) -> AppResult<Self> {
        let conn = Connection::open(path)
            .map_err(|e| AppError::Internal(format!("cannot open database {path}: {e}")))?;
        Self::init(conn)
    }

    /// Open an in-memory database (used by tests).
    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::Internal(format!("cannot open in-memory db: {e}")))?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> AppResult<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| AppError::Internal(format!("pragma setup failed: {e}")))?;
        let db = Db {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate()?;
        Ok(db)
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::Internal("database mutex poisoned".into()))
    }

    /// Create the schema if it does not already exist.
    fn migrate(&self) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
                email         TEXT UNIQUE COLLATE NOCASE,
                password_hash TEXT NOT NULL,
                created_at    INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS images (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                name       TEXT NOT NULL,
                mime       TEXT NOT NULL DEFAULT 'image/png',
                width      INTEGER,
                height     INTEGER,
                data       BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_images_user ON images(user_id);

            CREATE TABLE IF NOT EXISTS app_state (
                user_id    INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
                state_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| AppError::Internal(format!("migration failed: {e}")))?;
        Ok(())
    }

    // --- users -------------------------------------------------------------

    /// Insert a new user. Returns `Conflict` if the username/email is taken.
    pub fn create_user(
        &self,
        username: &str,
        email: Option<&str>,
        password_hash: &str,
    ) -> AppResult<User> {
        let now = now_unix();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO users (username, email, password_hash, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![username, email, password_hash, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(User {
            id,
            username: username.to_string(),
            email: email.map(|s| s.to_string()),
            created_at: now,
        })
    }

    /// Look up a user (with password hash) by username, case-insensitively.
    pub fn find_user_by_username(&self, username: &str) -> AppResult<Option<UserWithHash>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, username, email, password_hash, created_at
                 FROM users WHERE username = ?1 COLLATE NOCASE",
                params![username],
                |r| {
                    Ok(UserWithHash {
                        id: r.get(0)?,
                        username: r.get(1)?,
                        email: r.get(2)?,
                        password_hash: r.get(3)?,
                        created_at: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Fetch a public user record by id.
    pub fn find_user_by_id(&self, id: i64) -> AppResult<Option<User>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, username, email, created_at FROM users WHERE id = ?1",
                params![id],
                |r| {
                    Ok(User {
                        id: r.get(0)?,
                        username: r.get(1)?,
                        email: r.get(2)?,
                        created_at: r.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    // --- images ------------------------------------------------------------

    /// List a user's images, newest first (metadata only).
    pub fn list_images(&self, user_id: i64) -> AppResult<Vec<ImageMeta>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, mime, width, height, length(data), created_at, updated_at
             FROM images WHERE user_id = ?1 ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt
            .query_map(params![user_id], row_to_meta)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Insert a new image, returning its metadata.
    pub fn create_image(
        &self,
        user_id: i64,
        name: &str,
        mime: &str,
        width: Option<i64>,
        height: Option<i64>,
        data: &[u8],
    ) -> AppResult<ImageMeta> {
        let now = now_unix();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO images (user_id, name, mime, width, height, data, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![user_id, name, mime, width, height, data, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(ImageMeta {
            id,
            name: name.to_string(),
            mime: mime.to_string(),
            width,
            height,
            size: data.len() as i64,
            created_at: now,
            updated_at: now,
        })
    }

    /// Fetch a single image (with binary payload), scoped to the owner.
    pub fn get_image(&self, user_id: i64, id: i64) -> AppResult<Option<ImageRecord>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT id, name, mime, width, height, length(data), created_at, updated_at, data
                 FROM images WHERE id = ?1 AND user_id = ?2",
                params![id, user_id],
                |r| {
                    let meta = row_to_meta(r)?;
                    let data: Vec<u8> = r.get(8)?;
                    Ok(ImageRecord { meta, data })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// Update an image's name and/or payload. Returns `None` if not found /
    /// not owned by the user. Fields left `None` are unchanged.
    // Each parameter is a distinct, clearly-named optional column to patch;
    // grouping them into a struct would add ceremony without clarity here.
    #[allow(clippy::too_many_arguments)]
    pub fn update_image(
        &self,
        user_id: i64,
        id: i64,
        name: Option<&str>,
        mime: Option<&str>,
        width: Option<i64>,
        height: Option<i64>,
        data: Option<&[u8]>,
    ) -> AppResult<Option<ImageMeta>> {
        let now = now_unix();
        let conn = self.lock()?;
        let affected = conn.execute(
            "UPDATE images SET
                name       = COALESCE(?3, name),
                mime       = COALESCE(?4, mime),
                width      = COALESCE(?5, width),
                height     = COALESCE(?6, height),
                data       = COALESCE(?7, data),
                updated_at = ?8
             WHERE id = ?1 AND user_id = ?2",
            params![id, user_id, name, mime, width, height, data, now],
        )?;
        if affected == 0 {
            return Ok(None);
        }
        // Re-read metadata so callers get authoritative size/timestamps.
        drop(conn);
        Ok(self.get_image(user_id, id)?.map(|r| r.meta))
    }

    /// Delete an image. Returns `true` if a row was removed.
    pub fn delete_image(&self, user_id: i64, id: i64) -> AppResult<bool> {
        let conn = self.lock()?;
        let affected = conn.execute(
            "DELETE FROM images WHERE id = ?1 AND user_id = ?2",
            params![id, user_id],
        )?;
        Ok(affected > 0)
    }

    // --- app state ---------------------------------------------------------

    /// Read a user's saved editor state, if any.
    pub fn get_state(&self, user_id: i64) -> AppResult<Option<String>> {
        let conn = self.lock()?;
        let row = conn
            .query_row(
                "SELECT state_json FROM app_state WHERE user_id = ?1",
                params![user_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        Ok(row)
    }

    /// Upsert a user's editor state.
    pub fn set_state(&self, user_id: i64, state_json: &str) -> AppResult<()> {
        let now = now_unix();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO app_state (user_id, state_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET state_json = ?2, updated_at = ?3",
            params![user_id, state_json, now],
        )?;
        Ok(())
    }
}

/// Map a row of the standard metadata column order into [`ImageMeta`].
fn row_to_meta(r: &rusqlite::Row<'_>) -> rusqlite::Result<ImageMeta> {
    Ok(ImageMeta {
        id: r.get(0)?,
        name: r.get(1)?,
        mime: r.get(2)?,
        width: r.get(3)?,
        height: r.get(4)?,
        size: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn create_and_find_user() {
        let db = db();
        let u = db.create_user("Alice", Some("a@x.io"), "hash").unwrap();
        assert_eq!(u.username, "Alice");

        // username lookup is case-insensitive
        let found = db.find_user_by_username("alice").unwrap().unwrap();
        assert_eq!(found.id, u.id);
        assert_eq!(found.password_hash, "hash");

        let by_id = db.find_user_by_id(u.id).unwrap().unwrap();
        assert_eq!(by_id.username, "Alice");
    }

    #[test]
    fn duplicate_username_conflicts() {
        let db = db();
        db.create_user("bob", None, "h1").unwrap();
        let err = db.create_user("BOB", None, "h2").unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[test]
    fn image_crud_is_user_scoped() {
        let db = db();
        let alice = db.create_user("alice", None, "h").unwrap();
        let bob = db.create_user("bob", None, "h").unwrap();

        let img = db
            .create_image(
                alice.id,
                "drawing",
                "image/png",
                Some(10),
                Some(20),
                b"PNGDATA",
            )
            .unwrap();
        assert_eq!(img.size, 7);

        // alice can read it; bob cannot
        assert!(db.get_image(alice.id, img.id).unwrap().is_some());
        assert!(db.get_image(bob.id, img.id).unwrap().is_none());

        // listing is scoped
        assert_eq!(db.list_images(alice.id).unwrap().len(), 1);
        assert_eq!(db.list_images(bob.id).unwrap().len(), 0);

        // bob cannot delete alice's image
        assert!(!db.delete_image(bob.id, img.id).unwrap());
        assert!(db.delete_image(alice.id, img.id).unwrap());
        assert!(db.get_image(alice.id, img.id).unwrap().is_none());
    }

    #[test]
    fn update_image_partial_fields() {
        let db = db();
        let alice = db.create_user("alice", None, "h").unwrap();
        let img = db
            .create_image(alice.id, "old", "image/png", None, None, b"AAA")
            .unwrap();

        // rename only
        let updated = db
            .update_image(alice.id, img.id, Some("new"), None, None, None, None)
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "new");
        assert_eq!(updated.size, 3, "data unchanged when not provided");

        // replace data only
        let updated = db
            .update_image(
                alice.id,
                img.id,
                None,
                None,
                Some(4),
                Some(4),
                Some(b"BBBBBB"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "new", "name preserved");
        assert_eq!(updated.size, 6);
        assert_eq!(updated.width, Some(4));

        // updating a non-owned image returns None
        let bob = db.create_user("bob", None, "h").unwrap();
        assert!(db
            .update_image(bob.id, img.id, Some("hax"), None, None, None, None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn state_round_trip_and_upsert() {
        let db = db();
        let alice = db.create_user("alice", None, "h").unwrap();
        assert!(db.get_state(alice.id).unwrap().is_none());

        db.set_state(alice.id, r#"{"open":1}"#).unwrap();
        assert_eq!(db.get_state(alice.id).unwrap().unwrap(), r#"{"open":1}"#);

        // upsert overwrites
        db.set_state(alice.id, r#"{"open":2}"#).unwrap();
        assert_eq!(db.get_state(alice.id).unwrap().unwrap(), r#"{"open":2}"#);
    }

    #[test]
    fn deleting_user_cascades_to_images_and_state() {
        let db = db();
        let alice = db.create_user("alice", None, "h").unwrap();
        db.create_image(alice.id, "d", "image/png", None, None, b"X")
            .unwrap();
        db.set_state(alice.id, "{}").unwrap();

        db.lock()
            .unwrap()
            .execute("DELETE FROM users WHERE id = ?1", params![alice.id])
            .unwrap();

        assert_eq!(db.list_images(alice.id).unwrap().len(), 0);
        assert!(db.get_state(alice.id).unwrap().is_none());
    }
}
