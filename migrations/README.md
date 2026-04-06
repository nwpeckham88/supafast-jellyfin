# Migrations

PostgreSQL migration files:

- `0001_scan_and_audit_tables.sql` - baseline tables and indexes
- `0002_add_scan_result_json.sql` - adds `scan_sessions.result_json` backfill column for full preview payload storage

Migrations are applied automatically at backend startup whenever `DATABASE_URL` is set.

For manual use during development, you can still apply them yourself:

```bash
psql "$DATABASE_URL" -f migrations/0001_scan_and_audit_tables.sql
psql "$DATABASE_URL" -f migrations/0002_add_scan_result_json.sql
```

Startup uses the embedded `sqlx` migrator, so pending files in this directory will be applied before schema validation.
