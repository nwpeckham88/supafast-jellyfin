# ADR 0002: Idempotency for Execute and Rollback

- Status: Accepted
- Date: 2026-04-02

## Context

Clients can retry requests due to network issues, UI refreshes, or timeouts. Retried execute or rollback requests must not duplicate filesystem operations.

## Decision

Idempotency is enforced for mutating endpoints using caller-provided keys.

- Execute and rollback require `idempotency_key`.
- The idempotency scope is `(operation_type, base_path, idempotency_key)`.
- On first request:
  - Persist job intent and transition to running state before writes.
  - Execute plan and persist terminal status and result payload.
- On duplicate request with the same scope:
  - Return previously persisted result payload and status.
  - Do not re-run filesystem operations.

## Consequences

- Retries become safe and predictable.
- Database uniqueness constraints can enforce single execution per scope.
- Requires careful response persistence to replay prior results.

## Notes

- Idempotency is required for execute and rollback, not for scan preview.
- If an original request is still running, duplicate requests return current job state.
