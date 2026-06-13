# Render Deploy

This branch includes a minimal Render Blueprint for a hackathon deployment.
It runs the existing Docker image as one web service, serving both the Rust API
and the vendored frontend.

## Deploy

1. Push this branch to GitHub.
2. In Render, create a new Blueprint from the repository.
3. Select `render.yaml`.
4. Deploy the `nomad-airlines` service.
5. Open the generated Render URL and check `/api/health`.

The Blueprint sets `NOMAD_JWT_SECRET` with Render's generated secret support, so
logins keep working across ordinary deploys as long as the service environment is
preserved.

## Persistence

The Blueprint uses the app's existing SQLite database at `/data/nomad.db` and
mounts a Render persistent disk at `/data`. Only files under the disk mount are
preserved across deploys and restarts, so keep `NOMAD_DB_PATH` under `/data`.

Persistent disks require a paid Render service plan and disable zero-downtime
deploys. For this app, that tradeoff is acceptable for a hackathon because it
keeps the implementation simple while preserving user drawings.

For the existing `nomad-airlines` Render service, add payment information at
https://dashboard.render.com/billing before applying the disk. Render returns
`Payment information is required` when the disk API is called without billing.

## Local Check

```bash
docker compose up --build
curl http://127.0.0.1:8787/api/health
```
