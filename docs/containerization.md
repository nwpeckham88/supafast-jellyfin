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

Create media/log paths once (simple single-root mapping):

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
