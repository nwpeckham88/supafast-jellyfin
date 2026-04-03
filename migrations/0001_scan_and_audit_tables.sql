-- Phase 2 baseline schema for scan preview and audit logging.
-- PostgreSQL 14+

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS scan_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    media_type TEXT NOT NULL CHECK (media_type IN ('tv', 'movie')),
    base_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'expired', 'failed')),
    expires_at TIMESTAMPTZ NOT NULL,
    summary_json JSONB NOT NULL,
    result_json JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    media_type TEXT NOT NULL CHECK (media_type IN ('tv', 'movie')),
    base_path TEXT NOT NULL,
    scan_id UUID REFERENCES scan_sessions(id),
    idempotency_key TEXT UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'failed', 'rolled_back')),
    dir_count INTEGER NOT NULL DEFAULT 0,
    file_count INTEGER NOT NULL DEFAULT 0,
    merge_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS audit_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES audit_sessions(id) ON DELETE CASCADE,
    op_type TEXT NOT NULL CHECK (op_type IN ('rename_dir', 'rename_file', 'move_file', 'delete_dir')),
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    file_size BIGINT,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_entries_session ON audit_entries(session_id);
CREATE INDEX IF NOT EXISTS idx_scan_sessions_status_expires ON scan_sessions(status, expires_at);
