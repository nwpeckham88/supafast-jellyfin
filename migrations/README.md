# Migrations

PostgreSQL migration files:

- `0001_scan_and_audit_tables.sql` - baseline tables and indexes
- `0002_add_scan_result_json.sql` - adds `scan_sessions.result_json` backfill column for full preview payload storage

This repository does not yet include an automatic migration runner. Apply manually for now:

```bash
psql "$DATABASE_URL" -f migrations/0001_scan_and_audit_tables.sql
psql "$DATABASE_URL" -f migrations/0002_add_scan_result_json.sql
```

Next step: wire a migration tool (for example `sqlx-cli` or `refinery`) into startup/bootstrap.
