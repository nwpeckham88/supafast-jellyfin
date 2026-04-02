# supafast-jellyfin: Media Library Normalizer - Implementation Plan

## Overview

A media library normalizer for TV shows and movies. It scans directories, normalizes folder and episode filenames to a canonical format, detects and merges duplicate folders (same show, different provider tags), records all operations in an audit log, and supports rollback.

Stack: Rust/Axum backend, SvelteKit/Tailwind frontend, PostgreSQL.

## Non-Functional Requirements

- Safety: never mutate paths outside configured media roots.
- Reliability: every attempted filesystem operation is captured in persistent audit state.
- Idempotency: repeated execute/rollback requests with the same key must not duplicate work.
- Performance: scan and preview remain responsive for large libraries (1,000+ shows).
- Observability: correlation IDs and structured logs for requests and jobs.

---

## Target Format

`Title - Subtitle (Year)`

- Colons -> ` - `
- Underscores -> spaces
- Strip `[tmdbid-X]`, `[tvdbid-X]`, `[imdbid-X]` suffixes
- Strip quality tags: `[WEBDL-1080p]`, `[h264]`, trailing release group suffixes like `-ETHEL`
- Move year to end in parentheses
- Strip trailing separator artifacts

---

## Parser 1: Directory Names

10-step algorithm:

1. Strip provider ID tags: `[tmdbid-X]`, `[tvdbid-X]`, `[imdbid-X]`
2. Strip remaining bracket groups that are not a bare year
3. Extract year (already in parens, bare at end, after ` - `)
4. Replace `:` with ` - `
5. Replace `_` with space
6. Collapse multiple spaces
7. Strip trailing ` - ` artifacts
8. Trim whitespace
9. Strip trailing separator artifacts (safety net)
10. Reassemble: `Title (Year)` or `Title` if no year

### Verified Examples

| Input | Output |
|-------|--------|
| `A Perfect Planet 2021 - [tmdbid-115194]` | `A Perfect Planet (2021)` |
| `Alien - Earth 2025 - [tmdbid-157239]` | `Alien - Earth (2025)` |
| `Animal Control - 2023 [tvdbid-423022]` | `Animal Control (2023)` |
| `Animal Control 1997 - [tmdbid-214162]` | `Animal Control (1997)` |
| `Animaniacs 1993 - [tmdbid-82]` | `Animaniacs (1993)` |
| `Spider-Man: Into the Spider-Verse (2018)` | `Spider-Man - Into the Spider-Verse (2018)` |
| `Blade_Runner_2049` | `Blade Runner (2049)` |

---

## Parser 2: Episode Filenames

Strip all `[bracket groups]` (except bare years) and trailing `-GROUP` suffix.
Preserve `-thumb` suffix and multi-part extensions.

### File types renamed

- `.mkv`, `.mp4` (video)
- `.nfo` (metadata; contents unchanged)
- `-thumb.jpg` (episode thumbnail)
- `.srt`, `.eng.srt`, `.spa.srt`, etc. (subtitles)
- `.sharky.json` (external metadata)
- `.trickplay/` (trickplay sprite folder)

### Files never renamed

- `season.nfo`
- `tvshow.nfo`
- `poster.jpg`, `fanart.jpg`, `banner.jpg`, `clearlogo.png`
- `backdrop*.jpg`, `season*-poster.jpg`
- Season folders (`Season 1/`, etc.)

### Verified Examples

```text
Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL.mkv
-> Animal Control - S04E06 - Roosters and Moles.mkv

Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL-thumb.jpg
-> Animal Control - S04E06 - Roosters and Moles-thumb.jpg

A Perfect Planet - S01E01 - Volcano [tmdbid-115194].nfo
-> A Perfect Planet - S01E01 - Volcano.nfo

A Perfect Planet - S01E01 - Volcano [tmdbid-115194].eng.srt
-> A Perfect Planet - S01E01 - Volcano.eng.srt

Animal Control - S04E06 - Roosters and Moles [WEBDL-1080p][h264]-ETHEL.sharky.json
-> Animal Control - S04E06 - Roosters and Moles.sharky.json
```

---

## Parser 3: Quality Tags (for merge comparison)

Extract quality signals from filenames.

Output: `QualityInfo { resolution, codec, source, group, size_bytes }`

| Pattern | Examples |
|---------|----------|
| Resolution | `480p`, `720p`, `1080p`, `2160p`, `4k` |
| Codec | `h264`, `h265`, `hevc`, `x264`, `x265`, `av1` |
| Source | `WEBDL`, `WEB-DL`, `WEBRip`, `BluRay`, `HDTV`, `Remux` |
| Group | `-ETHEL`, `-NTG` |

---

## Merge Logic

When 2+ directories normalize to the same target name, their episodes are merged.

For each episode slot (matched by season/episode number):

| Scenario | Action |
|----------|--------|
| Episode only in one source | Auto-select |
| Same resolution + codec + source + size within 5% | Auto-select larger |
| Different resolution/codec/source or size delta > 5% | Conflict; user chooses |

Similarity:

```text
similar(a, b) =
  a.extension == b.extension
  AND a.quality.resolution == b.quality.resolution
  AND a.quality.codec == b.quality.codec
  AND a.quality.source == b.quality.source
  AND abs(a.size - b.size) / max(a.size, b.size) <= 0.05
```

---

## Filesystem Safety Rules

- Canonicalize all source and target paths.
- Reject operations outside selected `base_path`.
- Do not follow symlinks by default.
- Validate all target collisions before first write.
- Enforce one active execute/rollback job per `base_path`.
- Handle `EXDEV` via copy + fsync + atomic replace + delete, with audit entries.
- Handle case-only renames safely using temporary names when required.

---

## Database Schema

```sql
CREATE TABLE scan_sessions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    media_type   TEXT NOT NULL CHECK (media_type IN ('tv', 'movie')),
    base_path    TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('running', 'completed', 'expired', 'failed')),
    expires_at   TIMESTAMPTZ NOT NULL,
    summary_json JSONB NOT NULL
);

CREATE TABLE audit_sessions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    media_type      TEXT NOT NULL CHECK (media_type IN ('tv', 'movie')),
    base_path       TEXT NOT NULL,
    scan_id         UUID REFERENCES scan_sessions(id),
    idempotency_key TEXT UNIQUE,
    status          TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'rolled_back')),
    dir_count       INTEGER NOT NULL DEFAULT 0,
    file_count      INTEGER NOT NULL DEFAULT 0,
    merge_count     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_entries (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id  UUID NOT NULL REFERENCES audit_sessions(id) ON DELETE CASCADE,
    op_type     TEXT NOT NULL CHECK (op_type IN ('rename_dir', 'rename_file', 'move_file', 'delete_dir')),
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    file_size   BIGINT,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_entries_session ON audit_entries(session_id);
CREATE INDEX idx_scan_sessions_status_expires ON scan_sessions(status, expires_at);
```

---

## API Endpoints

All responses use envelope: `{ success, data, error, meta? }`

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/normalize/scan` | Create scan session + preview |
| `GET` | `/api/normalize/scan/{scan_id}` | Fetch cached scan result |
| `DELETE` | `/api/normalize/scan/{scan_id}` | Expire cached scan result |
| `POST` | `/api/normalize/execute` | Execute approved plan (`scan_id` + idempotency key) |
| `GET` | `/api/normalize/jobs/{job_id}` | Poll normalize job status |
| `GET` | `/api/rollback/sessions` | List past normalization sessions |
| `POST` | `/api/rollback/preview` | Preview rollback |
| `POST` | `/api/rollback/execute` | Execute rollback (idempotent) |
| `GET` | `/api/health` | Health check |

### `POST /api/normalize/scan` response shape

```json
{
  "success": true,
  "data": {
    "scan_id": "uuid",
    "media_type": "tv",
    "groups": [],
    "summary": {
      "total_shows": 42,
      "total_episodes": 1893,
      "merges": 3,
      "conflicts": 7
    }
  },
  "error": null
}
```

### `POST /api/normalize/execute` request shape

```json
{
  "scan_id": "uuid",
  "idempotency_key": "uuid",
  "media_type": "tv",
  "base_path": "/mnt/nas/servarr/data/media/tv/",
  "name_overrides": [],
  "conflict_resolutions": [],
  "dry_run": false
}
```

Execution rules:

- `scan_id` must exist, be unexpired, and match `media_type` and `base_path`.
- Reusing the same `idempotency_key` returns prior result without re-applying ops.
- Preflight validation must pass before filesystem writes begin.

---

## Configuration

| Env Var | Default | Description |
|---------|---------|-------------|
| `TV_BASE_PATH` | `/mnt/nas/servarr/data/media/tv/` | TV library root |
| `MOVIES_BASE_PATH` | `/mnt/nas/servarr/data/media/movies/` | Movie library root |
| `DATABASE_URL` | - | PostgreSQL connection string |
| `AUDIT_LOG_DIR` | `/var/log/supafast-jellyfin/` | Audit log output directory |
| `PORT` | `3001` | Backend listen port |
| `SCAN_TTL_MINUTES` | `60` | Time scan results remain executable |
| `FOLLOW_SYMLINKS` | `false` | Whether scanner traverses symlinks |

---

## Implementation Phases

### Phase 0 - ADRs + Safety Baseline

1. ADR: two-phase execution model (validate then apply).
2. ADR: idempotency behavior for execute/rollback.
3. ADR: per-base-path lock strategy.
4. Add correlation IDs and structured logging.
5. Implement canonicalization and boundary checks.

Deliverable: safety primitives and architectural decisions locked.

### Phase 1 - Parsers + Scaffolding

1. Initialize Rust project and dependencies.
2. Initialize SvelteKit project (Tailwind, API proxy).
3. Implement `config.rs`, `error.rs`.
4. Implement parser modules with unit tests.
5. Define model structs (`scan`, `plan`, `audit`, `rollback`).

Deliverable: buildable project with parser test suite passing.

### Phase 2 - DB + Scanner + Preview UI

1. Add migrations for `scan_sessions`, `audit_sessions`, `audit_entries`.
2. Add DB init and repository layer (`audit_repo`, `scan_repo`).
3. Build scanner and merger services.
4. Implement scan endpoints (`POST/GET/DELETE /normalize/scan`).
5. Build preview UI and conflict selection UX.

Deliverable: full hierarchical preview with cached scan sessions.

### Phase 3 - Two-Phase Execute + Audit

1. Build deterministic plan from `scan_id` + user overrides.
2. Preflight validation (collisions, permissions, boundaries).
3. Execute operation graph with per-op audit state.
4. Add execute and job-status endpoints.
5. Enforce idempotency at DB and route layers.

Deliverable: safe, idempotent normalize execution.

### Phase 4 - Rollback

1. Build session listing and rollback preview.
2. Execute reverse operations in reverse-chronological order.
3. Add idempotent rollback endpoint.
4. Build rollback UI.

Deliverable: conflict-aware rollback flow.

### Phase 5 - Scale + UX Hardening

1. Movie mode (directory-only rename).
2. `.trickplay/` handling in executor.
3. Background progress updates and pagination.
4. Error boundaries, retry UX, and notifications.

Deliverable: stable UX for large real-world libraries.

### Phase 6 - Verification + Release Gate

1. Integration tests for scanner/executor/merger/rollback.
2. Failure-injection tests (`EXDEV`, permission denied, target exists, crash mid-run).
3. Playwright E2E tests for scan/execute/rollback.
4. Load test representative library snapshot.

Deliverable: release checklist passed with recovery playbook.

---

## Risks and Mitigations

| Risk | Level | Mitigation |
|------|-------|------------|
| Wrong file selected in merge | CRITICAL | Mandatory preview + user choice for conflicts + rollback |
| Regex strips valid title content | HIGH | Conservative stripping rules + unknown pattern logging |
| Partial failure during execute | HIGH | Per-op audit state + rollback support + startup reconciliation |
| Rollback target already exists | MEDIUM | Preview checks current state and blocks conflicted reversals |
| User override causes duplicate target | MEDIUM | Preflight uniqueness validation before writes |
| Compound extension misidentified | MEDIUM | Explicit allowlist (`.eng.srt`, `.sharky.json`, etc.) |
| Cross-filesystem move (`EXDEV`) | LOW | Fallback copy+delete with audited steps |
| Duplicate execute request from UI retry | HIGH | Idempotency key + unique constraint replay behavior |
| Concurrent jobs on same base path | HIGH | Per-base-path locking |
| Path traversal/symlink escape | CRITICAL | Canonicalization + root boundary enforcement |

---

## Testing Strategy

### Unit tests

- `dirname` parser: spec examples and edge cases.
- `filename` parser: spec examples, compound extensions, thumb suffix handling.
- `quality` parser: full/partial/no tags and source variants.
- `merger`: identical files, +/-5% size, codec/resolution mismatches, multi-source.
- `normalizer`: overrides, conflict resolution, duplicate target validation.

### Integration tests (`backend/tests/`)

- Scanner grouping and conflict detection with temp dir fixtures.
- Executor end-to-end filesystem mutations plus audit assertions.
- Rollback round trip: execute then rollback to original state.
- Idempotency replay: same key returns prior result with no duplicate ops.
- Concurrency guard: second execute on same `base_path` rejected or queued.
- `EXDEV` fallback behavior and audit correctness.
- Crash recovery reconciliation on restart.

### Frontend E2E (Playwright)

- Scan flow, conflict resolution, inline rename edits, execute, rollback.

Coverage target: >=80% overall, parser/executor layers near 100%.

---

## Success Criteria

- [ ] Directory parser normalizes all specified examples and edge cases.
- [ ] Filename parser strips tags as specified while preserving protected files.
- [ ] Merge detection groups by normalized show name correctly.
- [ ] Auto-resolution handles materially equivalent files.
- [ ] Conflict UI presents all candidates and requires explicit choice.
- [ ] Execute and rollback are idempotent under request retries.
- [ ] No operation targets path outside selected base path.
- [ ] Audit log records every operation attempt and outcome.
- [ ] Rollback restores original state when filesystem has not externally diverged.
- [ ] Movie mode works without season/episode logic.
- [ ] API responses consistently use envelope format.
- [ ] UX remains usable for 100+ shows and large scan results.
