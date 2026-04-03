-- Backfill migration for environments that already applied 0001 without result_json.

ALTER TABLE scan_sessions
ADD COLUMN IF NOT EXISTS result_json JSONB NOT NULL DEFAULT '{}'::jsonb;

UPDATE scan_sessions
SET result_json = jsonb_build_object(
	'scan_id', id,
	'media_type', media_type,
	'groups', '[]'::jsonb,
	'summary', summary_json
)
WHERE result_json = '{}'::jsonb;

ALTER TABLE scan_sessions
ALTER COLUMN result_json DROP DEFAULT;
