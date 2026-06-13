# Architecture

## One backend, three targets

```
                         ┌─────────────────────────────────────────┐
                         │              frontend/                    │
                         │  jspaint (MIT)  +  nomad/ gallery iframe  │
                         │            fetch("/api/...")              │
                         └───────────────────┬───────────────────────┘
                                             │ HTTP (same origin)
                         ┌───────────────────▼───────────────────────┐
                         │            backend/ (axum)                 │
                         │  auth (argon2 + JWT) · images · state       │
                         │  SQLite (rusqlite, bundled) · static files  │
                         └───────────────────┬───────────────────────┘
        ┌────────────────────────────────────┼────────────────────────────────┐
        │ Docker/web: run binary               │ Desktop/Android: Tauri shell    │
        │ binds 0.0.0.0:8787, serves frontend  │ spawns backend on 127.0.0.1,    │
        │                                      │ opens webview at it             │
        └──────────────────────────────────────────────────────────────────────┘
```

The frontend never knows which target it's in: it always issues same-origin
`fetch("/api/...")` calls. This avoids maintaining a Tauri-IPC path *and* an
HTTP path.

## Backend

- **axum** router (`backend/src/lib.rs`): `/api/*` nested routes plus a
  `ServeDir` fallback that serves the frontend (SPA fallback to `index.html`).
- **Auth** (`auth.rs`): `argon2id` password hashing; HS256 JWTs (pure-Rust
  `rust_crypto` provider, so it cross-compiles to Android/iOS without C/aws-lc).
  `AuthUser` is an axum extractor that validates the `Bearer` token; protected
  handlers simply take an `AuthUser` argument.
- **DB** (`db.rs`): a single `rusqlite::Connection` behind a `Mutex` (SQLite
  serialises writes; per-user desktop or modest web load doesn't warrant a
  pool). All queries are user-scoped, so authorization is enforced in SQL
  (`WHERE user_id = ?`) — a user physically cannot read another's rows.
- **Errors** (`error.rs`): one `AppError` enum → JSON `{ "error": ... }` with the
  right status code. UNIQUE violations surface as `409 Conflict`.
- **Images**: stored as BLOBs. Created/updated from `data:` URLs
  (`canvas.toDataURL()`); listed as metadata; rendered via the authenticated
  `/raw` endpoint (the gallery fetches it as a blob → object URL, so the
  `Authorization` header is still sent).

### Schema

```sql
users(id, username UNIQUE, email UNIQUE, password_hash, created_at)
images(id, user_id→users, name, mime, width, height, data BLOB, created_at, updated_at)
app_state(user_id→users PK, state_json, updated_at)
```

`ON DELETE CASCADE` ties images and state to their owner.

## Frontend integration

jspaint is vendored unmodified except for a small, commented include block in
`index.html`. All NOMAD code lives in `frontend/nomad/`:

- **`integration.js`** runs in the jspaint (parent) window. It:
  - appends a `.menu-button` labeled “🖼️ Gallery” to jspaint's `.menus` bar
    (inheriting native styling), and
  - hosts the gallery in a same-origin `<iframe>` overlay so **Bootstrap's
    global CSS reset can't leak into jspaint**, and
  - exposes `window.NomadBridge` — `getCanvasDataURL()`, `getCanvasSize()`,
    `loadImageDataURL(name, dataURL)` (uses jspaint's `open_from_file`) — the
    only surface the iframe touches in the parent.
- **`gallery.html` + `gallery.js` + `auth.js` + `api.js`** run inside the
  iframe. Being same-origin, the iframe calls `parent.NomadBridge` directly and
  the backend over `fetch`.

### Why an iframe?

Bootstrap ships a global reboot (`body`, `box-sizing`, font, etc.). Loaded into
jspaint's document it would shift the editor's pixel-exact layout. The iframe is
the cleanest isolation boundary while staying same-origin (so no postMessage
gymnastics for data — only a tiny open/close message channel).

## State persistence ("come back to your work")

Two complementary mechanisms:

1. **jspaint's own autosave** persists the in-progress canvas locally and
   restores it on load (unchanged upstream behaviour).
2. **Server-side `app_state`** stores the last-opened gallery image id per
   account. On opening the gallery while signed in, a non-intrusive “Resume …”
   banner offers to reopen it — so work follows the account across devices.

## Security notes

- Passwords: argon2id, unique per-hash salts.
- Tokens: HS256 JWT, expiry enforced, signature verified per request.
- Authorization: every image/state query is scoped by `user_id`; cross-user
  access returns `404` (covered by `users_cannot_access_each_others_images`).
- Input: data-URL payloads validated and size-capped (`NOMAD_MAX_IMAGE_BYTES`);
  image names length-checked; gallery renders names via `textContent` (no HTML
  injection).
- In production, set `NOMAD_JWT_SECRET` so tokens survive restarts.
