# NOMAD Airlines — Claude Code Guide

## Project Overview

NOMAD Airlines is a Tauri desktop/mobile app that wraps **jspaint** (a browser-based MS Paint clone) and adds a Bootstrap gallery backed by SQLite. Features: local account auth (JWT), image upload/download/export, per-user state persistence ("resume last drawing").

## Repository Layout

```
/
├── backend/          # Rust/Axum HTTP server — auth, gallery API, SQLite
│   └── src/
│       ├── auth.rs         # JWT issuance + validation
│       ├── config.rs       # Env-var config (NOMAD_BIND_ADDR, NOMAD_JWT_SECRET, etc.)
│       ├── db.rs           # rusqlite connection + migrations
│       ├── models.rs       # Shared structs (User, Image, …)
│       ├── routes/
│       │   ├── auth_routes.rs    # POST /api/auth/register, /login, GET /me
│       │   ├── image_routes.rs   # CRUD for /api/images
│       │   └── state_routes.rs   # GET/PUT /api/state (resume-last-drawing)
│       └── error.rs        # Unified error type → HTTP response
├── frontend/         # jspaint base + NOMAD overlay
│   ├── index.html          # jspaint entry point (strict CSP)
│   ├── src/                # jspaint ES-module source
│   └── nomad/
│       ├── integration.js  # Runs in parent page — adds Gallery button, hosts iframe
│       ├── gallery.html    # Bootstrap gallery UI (isolated iframe — no CSP)
│       ├── gallery.js      # Gallery controller: auth ↔ grid, resume banner, actions
│       ├── auth.js         # Login/register form wiring; calls back into gallery.js
│       ├── api.js          # NomadAPI — fetch wrapper, JWT in localStorage
│       ├── gallery.css     # Gallery-specific styles
│       └── mobile.css      # Mobile layout overrides
├── src-tauri/        # Tauri v2 shell (builds desktop + Android)
│   └── tauri.conf.json     # Product config; frontend served from ../frontend
├── Dockerfile        # Multi-stage build: Rust backend + frontend static files
├── docker-compose.yml
└── render.yaml       # Render persistent-disk deployment blueprint
```

## Architecture: iframe Bridge Pattern

The gallery runs in a **same-origin `<iframe>`** (`nomad/gallery.html`) so Bootstrap's CSS reset can't disturb jspaint. Communication goes through `window.NomadBridge` (set by `integration.js` in the parent before the iframe loads):

```
jspaint parent window
  └─ window.NomadBridge = { getCanvasDataURL, loadImageDataURL, isEditorReady, … }
       ↑
  gallery iframe (nomad/gallery.js)
       var bridge = window.parent.NomadBridge;
```

- Parent → iframe: `postMessage({ type: "nomad:open" })` — triggers grid refresh + resume banner
- Iframe → parent: `postMessage({ type: "nomad:close" })` — hides overlay

## Backend API

Base URL: `/api`

| Method | Path | Purpose |
|--------|------|---------|
| POST | `/auth/register` | Create account → returns JWT |
| POST | `/auth/login` | `grant_type=password` → JWT |
| GET  | `/auth/me` | Validate token |
| GET  | `/images` | List user images |
| POST | `/images` | Create image (JSON body with `data_url`) |
| GET  | `/images/:id` | Get image (with `data_url`) |
| PUT  | `/images/:id` | Update image |
| DELETE | `/images/:id` | Delete image |
| GET  | `/images/:id/raw` | Raw bytes (auth required, for thumbnails) |
| GET  | `/state` | Get per-user editor state |
| PUT  | `/state` | Set per-user editor state |

Auth: `Authorization: Bearer <token>` on all `/images` and `/state` routes.

## Environment Variables (backend)

| Var | Default | Notes |
|-----|---------|-------|
| `NOMAD_BIND_ADDR` | `127.0.0.1:8787` | Set `0.0.0.0:8787` for Docker |
| `NOMAD_DB_PATH` | `nomad.db` | Use `/data/nomad.db` for persistence |
| `NOMAD_FRONTEND_DIR` | `../frontend` | Path to static files |
| `NOMAD_JWT_SECRET` | random (ephemeral) | **Must set in production** |
| `NOMAD_JWT_EXPIRY_SECS` | `86400` | Token lifetime in seconds |
| `NOMAD_MAX_IMAGE_BYTES` | `16777216` (16 MB) | Max decoded image size |

**Important:** If `NOMAD_JWT_SECRET` is not set, a random secret is generated per-process — tokens won't survive restarts. Always set it in Docker/Railway/Render deployments.

## Running Locally

### Backend only
```bash
cd backend
cargo run           # starts at 127.0.0.1:8787, serves ../frontend
```

### Full app via Tauri (desktop)
```bash
cd src-tauri
cargo tauri dev
```

### Docker
```bash
docker compose up --build
# App available at http://localhost:8787
```

## Key Design Decisions

### gallery.js auth flow
After login/register, `NomadAuth.bind` calls **`enterGallery()` directly** — not `init()`. Calling `init()` would make an extra `GET /api/auth/me` round-trip; a transient failure there silently calls `NomadAPI.logout()` and returns the user to the auth screen with no error, which also means `maybeResume()` is never reached and the Resume/Dismiss handlers are never set.

### Resume banner lifecycle
`maybeResume()` must be called:
1. After `renderGrid()` on first gallery entry (`enterGallery()`)
2. After `renderGrid()` every time the gallery overlay is **reopened** (the `nomad:open` message handler)

Forgetting (2) causes the resume banner to disappear after the first close.

### PDF export (gallery.js `downloadAsPDF`)
Implemented as an inline binary-safe PDF/1.4 generator — no external library. Uses `TextEncoder` for string parts and `Uint8Array` for raw JPEG bytes, then combines into a `Blob`. The image is embedded as DCT-encoded JPEG (`/Filter /DCTDecode`).

### CSP
`index.html` has a strict `<meta>` CSP (limits `script-src` to `'self'`, Firebase, YouTube). This applies only to the parent document. `gallery.html` inside the iframe has **no CSP** (it's static HTML without response headers), so Bootstrap CDN or other external scripts are loadable there if needed.

### jspaint globals
jspaint's ES modules expose selected functions as `window.*` globals for non-module consumers:
- `window.open_from_file` (set in `functions.js`) — used by `integration.js` to check `isEditorReady()`
- `window.new_local_session` (set in `sessions.js`)

## Deployment

- **Docker / docker-compose**: see `Dockerfile` + `docker-compose.yml`. Persistent data via named volume at `/data`.
- **Render**: see `render.yaml` (persistent disk at `/data`).
- **Railway**: see `docs/RAILWAY_DEPLOY.md` — set env vars in Railway dashboard, no volume needed if using Railway's persistent storage.
- **Tauri desktop**: `cargo tauri build` — bundles backend binary + frontend into a native installer.
- **Tauri Android**: build via Android Studio after `cargo tauri android init`.

## Branching Convention

- `main` — stable, deployable
- `cloud-deploy` — cloud deployment config (merged into main)
- `fix/*` — bug-fix branches
