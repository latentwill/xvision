# xvision execution board

> One line per active track. This is a fixture for migrate-board tests;
> the rows here are synthetic and do not correspond to real contracts.

## Active

### Sample Wave A

- [sample-foundation-track](contracts/sample-foundation-track.md) - foundation - ready - sample foundation row for tests. Touches shared types and gates leaves.
- [sample-leaf-track](contracts/sample-leaf-track.md) - leaf - ready - sample leaf row for tests.

### Sample Wave B

- [sample-claimed-track](contracts/sample-claimed-track.md) - integration - claimed - sample claimed row. PR pushed but not merged. Stacks on sample-foundation-track.
- [sample-pr-open-track](contracts/sample-pr-open-track.md) - leaf - pr-open - sample row in PR_OPEN state. Two extra - dashes - in the summary should survive parsing.

## Deferred

- [sample-deferred-track](contracts/sample-deferred-track.md) - integration - deferred - sample deferred row, not active.

## Recently Closed

- **Some prose entry** - not a parseable row.
