# ADR 0001: Two-Phase Execution Model

- Status: Accepted
- Date: 2026-04-02

## Context

Normalization and rollback operations mutate filesystem state and can fail mid-flight. We must prevent partial, unsafe writes caused by invalid plans, path escapes, or unresolved collisions.

## Decision

All mutating jobs use a strict two-phase model:

1. Validate phase (no writes)
- Build deterministic operation plan from request inputs.
- Canonicalize all source and target paths.
- Enforce base-path boundary checks for every operation.
- Detect target collisions and illegal operations.
- Verify lock eligibility for the base path.

2. Apply phase (writes allowed)
- Execute plan only if validation succeeds fully.
- Record each attempted operation in audit state.
- Abort on first unrecoverable error with failure status.

## Consequences

- Safety improves because no write occurs before full preflight passes.
- Failures are easier to reason about since plan and validation are deterministic.
- Validation cost is paid upfront but avoids expensive partial rollback scenarios.

## Notes

- This ADR governs both normalize execute and rollback execute.
- Future optimization must preserve validate-before-apply semantics.
