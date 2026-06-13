# Third-party notices

NOMAD Airlines bundles the following third-party software. All are MIT-licensed
and their copyright notices are retained.

## jspaint

The painting frontend in [`frontend/`](frontend/) (everything except the
[`frontend/nomad/`](frontend/nomad/) directory) is **jspaint** by Isaiah Odhner.

- Source: https://github.com/1j01/jspaint
- License: MIT — see [`frontend/LICENSE.txt`](frontend/LICENSE.txt)
- Copyright (c) 2022 Isaiah Odhner

The NOMAD gallery integration lives entirely in `frontend/nomad/` and a small,
clearly-marked include block added to `frontend/index.html`; jspaint's own
source is otherwise unmodified.

## Bootstrap

The gallery UI ([`frontend/nomad/gallery.html`](frontend/nomad/gallery.html))
uses Bootstrap 5.3.3, vendored at `frontend/nomad/vendor/`.

- Source: https://github.com/twbs/bootstrap
- License: MIT
- Copyright (c) 2011-2024 The Bootstrap Authors

## Rust crates

The backend depends on crates including axum, tokio, rusqlite, argon2,
jsonwebtoken, serde, and tower-http, each under its own permissive license
(MIT/Apache-2.0). See `backend/Cargo.toml` and the crates' repositories.
