# Railway Deploy

This deployment runs the existing Docker image as one Railway service and stores
the SQLite database on a Railway volume mounted at `/data`.

## Current Setup

- Project: `nomad-airlines`
- Service: `nomad-airlines`
- Volume: `nomad-airlines-volume`
- Volume mount: `/data`
- Database path: `/data/nomad.db`

## Required Variables

Set these on the Railway service:

```bash
NOMAD_FRONTEND_DIR=/app/frontend
NOMAD_DB_PATH=/data/nomad.db
NOMAD_JWT_SECRET=<generated secret>
NOMAD_JWT_EXPIRY_SECS=86400
PORT=8787
```

The service listens on `8787`; generate the Railway domain for the same port.

## Deploy

```bash
railway up --service nomad-airlines --detach
railway domain --service nomad-airlines --port 8787
```

Check health:

```bash
curl https://<railway-domain>/api/health
```

## Persistence

Only data under `/data` is persistent. Keep `NOMAD_DB_PATH=/data/nomad.db` so
drawings survive deploys and restarts.
