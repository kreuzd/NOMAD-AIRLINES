# NOMAD Airlines — web/server (Docker) target.
#
# Builds the Rust backend (which bundles SQLite via rusqlite) and ships it with
# the static frontend it serves. Desktop/Android are built separately via Tauri.

# ---- build stage ----------------------------------------------------------
FROM rust:1-bookworm AS build
WORKDIR /app

# Build dependencies first for better layer caching.
COPY backend/Cargo.toml backend/Cargo.lock* backend/
RUN mkdir -p backend/src && echo "fn main() {}" > backend/src/main.rs \
    && echo "" > backend/src/lib.rs \
    && cargo build --release --manifest-path backend/Cargo.toml || true

# Now the real sources.
COPY backend backend
RUN touch backend/src/main.rs backend/src/lib.rs \
    && cargo build --release --manifest-path backend/Cargo.toml

# ---- runtime stage --------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /app/backend/target/release/nomad-backend /usr/local/bin/nomad-backend
COPY frontend /app/frontend

ENV NOMAD_BIND_ADDR=0.0.0.0:8787 \
    NOMAD_FRONTEND_DIR=/app/frontend \
    NOMAD_DB_PATH=/data/nomad.db
# Persist the SQLite database across container restarts.
VOLUME ["/data"]
EXPOSE 8787

# IMPORTANT: set NOMAD_JWT_SECRET at runtime so tokens survive restarts.
CMD ["nomad-backend"]
