# Fork feature migration to v0.3.0

The migration is tracked as source documentation plus small, reviewable implementation changes.

- [Feature inventory and porting plan](v0.3.0-feature-inventory.md) — what the old fork adds, how the features work, and the initial reuse/reimplementation decision.
- [v0.3.0 integration architecture](v0.3.0-integration-architecture.md) — verified current source seams and recommended ownership/scheduling boundaries.
- [v0.3.0 addon compatibility audit](v0.3.0-addon-compatibility.md) — first port of the three portable addons, API mismatch found and smoke-test coverage.

## Working rules

1. Keep `upgrade/v0.3.0` as the clean upstream baseline.
2. Work on isolated feature branches; do not promote incomplete migration work to `main`.
3. Update the relevant document in the same PR as each implementation change.
4. Record both automated and manual validation.
5. Do not mark behavior complete based only on successful compilation.
