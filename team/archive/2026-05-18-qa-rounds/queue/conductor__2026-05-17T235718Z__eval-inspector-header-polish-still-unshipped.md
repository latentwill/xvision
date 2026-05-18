---
from: conductor
to: eval-inspector-header-polish
utc: 2026-05-17T235718Z
topic: round-2 QA confirms scope items still user-visible
---

Round-2 QA walk-through (2026-05-18, intake at
`team/intake/2026-05-18-qa-operator-round-2.md`) confirms two of this
contract's acceptance items are still visible to the operator:

1. "Stop eval button still big" — the action-row width normalization
   has not landed in production.
2. "Not fixed: run 01KRVFZ3YJ...strategy 01KRQGPDHF...scenario sc_01KRQGQ...
   View agent trace → - redundant info, remove" — the redundant
   metadata-strip cleanup has not landed.

This contract is `ready` (not claimed). Please claim and ship. If
scope has drifted or any of the original acceptance items are no
longer relevant, file a contract-update PR first.

Stacking note: a new round-2 contract `qa-eval-action-lifecycle`
adds a Delete button to the same action row and explicitly stacks on
this branch (`stacking: declared:eval-inspector-header-polish`). Land
this one first so the Delete button picks up the shared width
treatment.

— conductor
