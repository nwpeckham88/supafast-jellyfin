# Containerization Guide

This repository can run as a 3-service stack:

- `postgres`: persistence
- `backend`: Rust/Axum API (`:3001`)
- `frontend`: SvelteKit app (`:3000`)

## 1. Prepare environment

Copy the template and adjust values if needed:

```bash
cp .env.docker.example .env
```

Set container user IDs to your host user (prevents root-owned files on mounted paths):

```bash
echo "APP_UID=$(id -u)" >> .env
echo "APP_GID=$(id -g)" >> .env
```

Create media/log paths once (separate TV/movie mappings):

```bash
mkdir -p ./tmp/media/tv ./tmp/media/movies ./tmp/logs
```

Optional mapping variables:

- `TV_LIBRARY_HOST_PATH` (default: `./tmp/media/tv`)
- `MOVIES_LIBRARY_HOST_PATH` (default: `./tmp/media/movies`)
- `AUDIT_LOG_HOST_PATH` (default: `./tmp/logs`)

## 2. Build and run

```bash
docker compose up --build
```

Open:

- Frontend: `http://localhost:3000`
- Backend health: `http://localhost:3001/api/health`

## 3. Stop stack

```bash
docker compose down
```

## 4. Reset database volume

Use this to re-run SQL init scripts in `migrations/`:

```bash
docker compose down -v
```

## Notes

- Backend bind address is configurable via `HOST` (default `127.0.0.1`) and `PORT`.
- `PUBLIC_API_BASE_URL` is a frontend build-time env; rebuild frontend container after changes.
- Frontend container proxy uses `VITE_API_PROXY_TARGET` (default `http://backend:3001`) so `/api/*` calls work inside Docker networking.
- Migrations are applied by the Postgres image only on first initialization of an empty data volume.

## Startup Troubleshooting

The backend now performs startup path diagnostics and validation before serving requests.
If mounts or env vars are misconfigured, startup fails fast with a clear reason.

Typical log lines to look for:

- `startup configuration diagnostics`
- `startup path validation passed`
- `startup validation failed for TV_BASE_PATH: ...`
- `startup validation failed for MOVIES_BASE_PATH: ...`
- `startup validation failed for AUDIT_LOG_DIR: ...`

Common fixes:

1. Ensure host directories exist:
	- `mkdir -p ./tmp/media/tv ./tmp/media/movies ./tmp/logs`
2. Verify compose mappings:
	- `TV_LIBRARY_HOST_PATH` -> `${TV_BASE_PATH}`
	- `MOVIES_LIBRARY_HOST_PATH` -> `${MOVIES_BASE_PATH}`
3. Verify permissions for the container user (`APP_UID`/`APP_GID`) on mounted folders.
4. Verify environment values are loaded:
	- `docker compose config`
5. For database issues, confirm `DATABASE_URL` points to the compose Postgres service.
