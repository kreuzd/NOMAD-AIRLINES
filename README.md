# NOMAD Airlines ✈️🖼️

A cross-platform painting app built on **[jspaint](https://github.com/1j01/jspaint)**,
extended with a **Bootstrap image gallery** backed by **SQLite** and guarded by
**local-account authentication (JWT / OAuth2 password grant)**.

The same codebase ships to three targets:

| Target | How |
|---|---|
| 🌐 **Web / Docker** | Run the Rust backend; it serves the frontend over HTTP. |
| 🖥️ **Desktop** (macOS/Windows/Linux) | Tauri app embeds the backend and points its webview at it. |
| 📱 **Android** (and iOS) | Same Tauri shell, mobile target. |

> **Why one HTTP backend instead of Tauri IPC?** The frontend always talks to
> `/api/*` over `fetch`. The Tauri shell just starts that backend on a localhost
> port in-process, so there is exactly one frontend code path everywhere.

---

## Features

- **jspaint editor** — the full classic-Paint experience, unmodified, with a new
  **“🖼️ Gallery”** button added to its nav/menu bar.
- **Gallery** (Bootstrap grid, isolated in an iframe so it can't disturb jspaint):
  - **Create** — save the current drawing as a new gallery image.
  - **Import** — from device (file picker → gallery) and from gallery (→ editor).
  - **Export** — to device (download) and to gallery (save canvas back into an image).
  - **Manage** — open/edit, rename, delete; per-user, fully access-controlled.
- **Authentication** — register / log in with local credentials; argon2 password
  hashing; JWT bearer tokens issued via an OAuth2 *password*-grant-style endpoint.
- **State persistence** — your last-opened drawing is remembered server-side
  (per account), and jspaint's own local autosave restores in-progress work, so
  you return to where you left off after quitting.
- **SQLite** — users, images (stored as BLOBs), and per-user state.
- **Tests** — Rust unit + HTTP integration tests for auth, DB, image CRUD
  authorization, and state. (jspaint itself is intentionally not re-tested.)

---

## Repository layout

```
NOMAD-AIRLINES/
├── backend/              # Rust axum server (auth, SQLite, images, state, static serving)
│   ├── src/
│   │   ├── lib.rs        # AppState + router assembly + serve()
│   │   ├── main.rs       # `nomad-backend` binary (Docker/web target)
│   │   ├── config.rs     # env-driven configuration
│   │   ├── db.rs         # SQLite (rusqlite, bundled) + schema + queries
│   │   ├── auth.rs       # argon2 hashing, JWT, AuthUser extractor
│   │   ├── models.rs     # request/response types
│   │   ├── error.rs      # AppError → JSON responses
│   │   ├── util.rs       # data-URL parsing
│   │   └── routes/       # auth / images / state handlers
│   └── tests/            # HTTP integration tests
├── frontend/             # Vendored jspaint (MIT) + the NOMAD gallery
│   ├── index.html        # jspaint's, with a small NOMAD include block
│   └── nomad/            # ← all our frontend code lives here
│       ├── integration.js  # parent: nav button + iframe overlay + canvas bridge
│       ├── gallery.html    # iframe document (Bootstrap)
│       ├── api.js          # API client
│       ├── auth.js         # login/register UI
│       ├── gallery.js      # gallery controller
│       ├── gallery.css / parent.css
│       └── vendor/         # Bootstrap 5.3.3 (vendored, offline-friendly)
├── src-tauri/            # Tauri desktop + Android/iOS shell
├── Dockerfile            # web/server image
├── docker-compose.yml
├── .github/workflows/ci.yml
└── docs/ARCHITECTURE.md
```

---

## Running

### Web / Docker (no toolchain beyond Docker)

```bash
docker compose up --build
# open http://localhost:8787
```

Set a real secret in `docker-compose.yml` (`NOMAD_JWT_SECRET`) before deploying.

### Render (hackathon cloud deploy)

This repo includes a minimal Render Blueprint at [`render.yaml`](render.yaml).
See [`docs/RENDER_DEPLOY.md`](docs/RENDER_DEPLOY.md) for the deploy steps and
the SQLite persistence caveat.

### Railway (cloud deploy with persistent SQLite)

Railway can run the same Docker image with a persistent volume mounted at
`/data`, keeping gallery drawings in `/data/nomad.db`. See
[`docs/RAILWAY_DEPLOY.md`](docs/RAILWAY_DEPLOY.md).

### Backend directly (for development)

Requires a Rust toolchain (1.77+). SQLite is bundled by `rusqlite` — no system
library needed.

```bash
cd backend
NOMAD_FRONTEND_DIR=../frontend cargo run
# open http://127.0.0.1:8787
```

### Desktop (Tauri)

Prerequisites: Rust + the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
for your OS, and the Tauri CLI:

```bash
cargo install tauri-cli --version "^2"
cargo tauri dev        # run
cargo tauri build      # produce installers
```

### Android (Tauri mobile)

Prerequisites: the above, plus the Android SDK + NDK and `ANDROID_HOME`/`NDK_HOME`.

```bash
cargo tauri android init     # scaffold src-tauri/gen/android (one-time)
cargo tauri android dev      # run on emulator/device
cargo tauri android build    # produce an APK/AAB
```

> iOS is also supported by the same shell via `cargo tauri ios …` on macOS with Xcode.

---

## Configuration (environment variables)

| Variable | Default | Meaning |
|---|---|---|
| `NOMAD_BIND_ADDR` | `127.0.0.1:8787` | Address the HTTP server binds to. |
| `NOMAD_FRONTEND_DIR` | `../frontend` | Directory of the static frontend to serve. |
| `NOMAD_DB_PATH` | `nomad.db` | SQLite file path (`:memory:` for ephemeral). |
| `NOMAD_JWT_SECRET` | *(random)* | HMAC secret for JWTs. **Set in production.** |
| `NOMAD_JWT_EXPIRY_SECS` | `86400` | Access-token lifetime. |
| `NOMAD_MAX_IMAGE_BYTES` | `16777216` | Max decoded image size (16 MiB). |
| `NOMAD_PORT` | `8787` | Port the Tauri shell starts the backend on. |

---

## API reference

All responses are JSON. Errors are `{ "error": "<message>" }` with an HTTP status.

| Method | Path | Auth | Description |
|---|---|---|---|
| `GET` | `/api/health` | — | Liveness check. |
| `POST` | `/api/auth/register` | — | Create account → `{ access_token, user, … }`. |
| `POST` | `/api/auth/token` (or `/login`) | — | OAuth2 password grant → token. |
| `GET` | `/api/auth/me` | ✓ | Current user. |
| `GET` | `/api/images` | ✓ | List your images (metadata). |
| `POST` | `/api/images` | ✓ | Create/import from a data URL. |
| `GET` | `/api/images/{id}` | ✓ | One image incl. `data_url` (export). |
| `GET` | `/api/images/{id}/raw` | ✓ | Raw bytes with content type (thumbnails). |
| `PUT` | `/api/images/{id}` | ✓ | Rename and/or replace payload. |
| `DELETE` | `/api/images/{id}` | ✓ | Delete. |
| `GET` | `/api/state` | ✓ | Read per-user editor state. |
| `PUT` | `/api/state` | ✓ | Save per-user editor state. |

Authenticate by sending `Authorization: Bearer <access_token>`.

---

## Testing

```bash
cd backend
cargo test            # 18 unit + 11 integration tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs fmt, clippy,
tests, and a Docker build on every push/PR.

---

## Licensing & attribution

This project is MIT-licensed ([`LICENSE`](LICENSE)). It bundles jspaint and
Bootstrap, both MIT — see [`NOTICE.md`](NOTICE.md). jspaint's own license is
preserved at [`frontend/LICENSE.txt`](frontend/LICENSE.txt).
