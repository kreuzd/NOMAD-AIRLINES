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

## Hackathon Persistence

The free Blueprint intentionally uses the app's existing SQLite database at
`/data/nomad.db` without adding a paid persistent disk. This is enough to demo
"same account, same drawings" while the service instance stays alive, but data
can disappear on restart, redeploy, or instance replacement.

For a more durable demo, add a Render persistent disk mounted at `/data`, or move
gallery images to a managed datastore later.

## Local Check

```bash
docker compose up --build
curl http://127.0.0.1:8787/api/health
```
