-- Allow recycle-bin-backed cleanup operations to be recorded in audit_entries.

ALTER TABLE audit_entries
    DROP CONSTRAINT IF EXISTS audit_entries_op_type_check;

ALTER TABLE audit_entries
    ADD CONSTRAINT audit_entries_op_type_check
    CHECK (op_type IN ('rename_dir', 'rename_file', 'move_file', 'delete_file', 'delete_dir'));
