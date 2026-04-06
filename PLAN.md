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

## Gap Analysis (2026-04-06)

Assessment of current implementation against this plan. Build compiles cleanly; 94 tests pass, 0 fail, 1 ignored (requires live Postgres).

### What's Working Well

| Area | Notes |
|------|-------|
| Directory parser | Handles `[imdb-X]`, `[tmdbid-X]`, `[tvdbid-X]`, unicode colons, underscores, trailing separators, year extraction |
| Episode parser | Protected files, compound extensions, `-thumb` suffix, release group stripping, NFO enrichment |
| Quality parser | Extracts resolution, codec, source, group per spec |
| Season parser | Normalizes `S01`, `season 1`, `Season_1` → `Season 1` |
| Path safety | Canonicalization, boundary enforcement, parent-dir traversal rejection, symlink checking |
| Base path locks | Per-base-path mutual exclusion for concurrent jobs |
| API envelope | `{ success, data, error, meta }` per spec |
| Config/startup | All env vars from spec, startup path validation with write probes, credential masking |
| Idempotency | Request fingerprinting, claim-or-replay pattern for execute and rollback |
| Correlation IDs | Middleware injects/propagates correlation IDs |
| Rollback | Reverse-chronological order, blocked-reason classification, compensating undo on failure |
| Trickplay handling | Companion `.trickplay` dirs detected and renamed alongside media files |

### Issues

#### CRITICAL

1. **No `move_file` or `delete_dir` operation types in executor.** `PlanOpType` only has `RenameDir` and `RenameFile`. Merge code in `execute.rs` uses `RenameFile` for file moves during merge, but there is no `delete_dir` to clean up empty source directories after merge. Empty source show directories (e.g., `Ted Lasso - [tmdbid-97546]`) will be left behind.

2. **Execute repository is in-memory only.** `ExecuteRepository` has no Postgres implementation (unlike `ScanRepository`). All job state, audit entries, and idempotency records are lost on restart. Rollback is impossible after server restart. Idempotency guarantees are ephemeral. The schema above specifies `audit_sessions` and `audit_entries` tables for persistent state.

3. **EXDEV cross-filesystem moves not handled.** `execute.rs` uses bare `std::fs::rename()`. This plan specifies "Handle EXDEV via copy + fsync + atomic replace + delete, with audit entries." If NAS storage is on a different mount, renames fail with EXDEV.

#### HIGH

4. **Scan service doesn't count episodes or detect conflicts.** `scan.rs`: `total_episodes` is always 0, `conflicts` is always 0. Scanner only reads top-level directories — doesn't descend into season folders. Preview UI shows incomplete summary data.

5. **ScanGroup lacks episode-level detail.** `ScanGroup` only has `normalized_name` and `source_directories`. This plan describes a hierarchical response with episode lists, quality info, and conflict markers per episode slot. Frontend can't show per-episode detail or let users resolve per-episode conflicts.

6. **Quality parser extracted but never used in merge decisions.** `extract_quality_info()` exists but is never called by scanner or executor. Auto-resolution for materially equivalent files (same resolution/codec/source, size within 5%) is not implemented. Merge conflict resolution is entirely manual.

7. **Movie mode is incomplete.** Scan endpoint accepts `"movie"` and directory parser works for movies, but `build_plan()` only calls `build_tv_nested_operations()`. Movie directories should get dir renames only (no episode file processing), but `build_group_merge_operations` still tries episode filename normalization for movie files.

8. **`is_year()` is too permissive.** Accepts any 4-digit number (1000–9999). Titles containing 4-digit numbers that aren't years could be misinterpreted. Should constrain to a reasonable range (e.g., 1880–2035).

#### MEDIUM

9. **Rollback state is in-memory only.** `RollbackState` in `rollback.rs` uses `RwLock` and is lost on restart. Separate from issue #2 but same root cause.

10. **`validate_plan_for_media_type()` is a no-op.** `execute.rs`: function is stubbed out. No validation that movie plans don't contain episode file renames or that TV plans have season structure.

11. **No pagination on scan results.** `ScanResult` returns all groups in one response. For large libraries (1,000+ shows), this is a very large payload.

12. **`DefaultHasher` for plan/request fingerprints.** `execute.rs`: `DefaultHasher` is not guaranteed stable across Rust versions. If the binary is upgraded, idempotency keys silently change meaning. Should use a stable hash (SHA-256, xxhash, etc.).

13. **No startup reconciliation.** If the server crashes mid-execute, in-memory state is lost and the filesystem is left in a partial state with no way to resume or detect the incomplete job.

14. **`follow_symlinks` config declared but never read.** `config.rs` defines it, but no code checks `config.follow_symlinks`. Scanner always follows the default `is_dir()` behavior.

15. **No rate limiting on endpoints.** Security checklist in this plan requires rate limiting.

#### LOW

16. **Duplicate `PaginationMeta` structs.** `execute.rs` and `rollback.rs` define separate identical pagination meta structs.

17. **Health endpoint returns plain text.** Other endpoints use `ApiResponse` envelope; health returns bare `"ok"`.

18. **`base_path` exposed in `ScanResult`.** Scan result serializes `base_path`, exposing server filesystem paths to the client.

### Phase Completion Assessment

| Phase | Status | Completeness | Key Gaps |
|-------|--------|-------------|----------|
| Phase 0 - ADRs + Safety | Done | ~80% | Missing: ADR docs, EXDEV handling |
| Phase 1 - Parsers + Scaffolding | Done | ~95% | Quality parser unused |
| Phase 2 - DB + Scanner + Preview | Partial | ~60% | Scan repo has Postgres. Scanner only does directory grouping, no episode enumeration |
| Phase 3 - Execute + Audit | Partial | ~70% | Execute works in-memory. Missing: persistent audit, EXDEV, `delete_dir` ops, episode-level merge |
| Phase 4 - Rollback | Partial | ~75% | Works end-to-end but in-memory only. Blocked-reason detection is solid |
| Phase 5 - Scale + UX Hardening | Early | ~30% | Movie scan works but execute path lacks movie-specific handling. No background progress, no trickplay during merge |
| Phase 6 - Verification | Early | ~40% | 94 unit tests pass. Missing: integration tests for real filesystem merges, failure injection, E2E Playwright |

### Remediation Priority

**Production blockers (do first):**

1. Persist execute/audit state to Postgres — addresses issues #2, #9, #13
2. Handle EXDEV cross-filesystem renames — issue #3
3. Add `move_file`/`delete_dir` op types + empty-dir cleanup after merges — issue #1

**Functionality gaps (do next):**

4. Enrich scan with episode-level detail + episode counting — issues #4, #5
5. Wire up quality parser for auto-merge resolution — issue #6
6. Movie-specific execute path — issue #7

**Correctness and hardening:**

7. Replace `DefaultHasher` with stable hash for fingerprints — issue #12
8. Implement `validate_plan_for_media_type()` — issue #10
9. Constrain `is_year()` range — issue #8
10. Add pagination to scan results — issue #11
11. Wire up `follow_symlinks` config — issue #14
12. Add rate limiting — issue #15
13. Deduplicate `PaginationMeta`, fix health envelope, review `base_path` exposure — issues #16, #17, #18

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
