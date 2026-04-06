-- Extend audit tables to support PgExecuteRepository.
-- Adds columns for request fingerprinting, plan hash, error tracking,
-- and per-entry status/ordering that the in-memory repo carried in RAM.

ALTER TABLE audit_sessions
    ADD COLUMN IF NOT EXISTS request_fingerprint TEXT,
    ADD COLUMN IF NOT EXISTS plan_hash TEXT,
    ADD COLUMN IF NOT EXISTS error TEXT;

ALTER TABLE audit_entries
    ADD COLUMN IF NOT EXISTS op_index INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed')),
    ADD COLUMN IF NOT EXISTS error_message TEXT;