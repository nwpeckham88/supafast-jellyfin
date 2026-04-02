# ADR 0003: Per-Base-Path Lock Strategy

- Status: Accepted
- Date: 2026-04-02

## Context

Concurrent normalize/rollback jobs against the same media root can race and corrupt state. We need a lock model that prevents conflicting jobs while allowing parallelism across unrelated roots.

## Decision

Use one active mutating job per canonicalized base path.

- Lock key: canonical base path string.
- Mutating job types: normalize execute and rollback execute.
- Acquisition:
  - Lock must be acquired before apply phase begins.
  - Validation can run without lock, but apply requires lock.
- Behavior on contention:
  - Reject with conflict/busy response for same base path.
  - Allow concurrent jobs for different base paths.

## Consequences

- Prevents conflicting writes within one library root.
- Maintains throughput for independent roots (for example TV vs movies).
- Requires robust canonicalization for deterministic lock keys.

## Notes

- In-process lock is sufficient for a single instance baseline.
- Multi-instance deployments will replace this with distributed locking while keeping lock-key semantics.
