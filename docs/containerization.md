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

Optional host paths used by the backend container:

- `TV_BASE_PATH_HOST` (default: `./tmp/tv`)
- `MOVIES_BASE_PATH_HOST` (default: `./tmp/movies`)

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
- Migrations are applied by the Postgres image only on first initialization of an empty data volume.
