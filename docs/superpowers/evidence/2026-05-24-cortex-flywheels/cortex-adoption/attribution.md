# Cortex Adoption Attribution Slice

Date: 2026-05-25

Scope: Phase 1.5 attribution-only preflight. No `cortex-core` dependency was
added in this slice; the current worktree still uses the in-tree
`xvision-memory` substrate. This deliberately satisfies the "attribution
first" gate before any backend swap.

Artifacts:

- `CREDITS.md` names `gambletan/cortex`, upstream URL, MIT license, planned
  version target, and role.
- `LICENSES/gambletan-cortex.txt` contains the upstream LICENSE text fetched
  from `https://raw.githubusercontent.com/gambletan/cortex/main/LICENSE` on
  2026-05-25.
- `README.md` Architecture credits `gambletan/cortex` as the planned memory
  engine backend and links to the upstream repository.
- Memory-touching docs now carry first-mention attribution:
  - `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`
  - `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`
  - `docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md`

Verification:

```text
test -f CREDITS.md
test -f LICENSES/gambletan-cortex.txt
rg -n "gambletan/cortex" CREDITS.md README.md docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md
```
