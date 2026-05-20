---
track: scenario-clone-form-structural-fields
lane: integration
wave: qa-2026-05-19
worktree: .worktrees/scenario-clone-form-structural-fields
branch: task/scenario-clone-form-structural-fields
base: origin/main
status: ready
depends_on: []                                                  # ScenarioForm already lifted; inline accordion already shipped (#341 commit 53f3e3f)
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/scenarios-detail.tsx                # extend the inline clone accordion to mount ScenarioForm
  - frontend/web/src/routes/scenarios-detail.test.tsx           # add a clone-with-structural-overrides test
  - frontend/web/src/components/scenario/ScenarioForm.tsx       # only if a controlled-form prop is missing for the clone use case
  - frontend/web/src/components/scenario/ScenarioForm.test.tsx  # if a new prop needs coverage
  - frontend/web/src/api/scenarios.ts                           # only if ScenarioMutations payload typing needs an export
forbidden_paths:
  - frontend/web/src/routes/scenarios-new.tsx                   # do not regress the new-scenario flow; reuse, don't refactor
  - frontend/web/src/routes/scenarios.tsx                       # list route, owned by lists-v1 phase 2c
  - frontend/web/src/components/lists/**                        # phase-1 components locked
  - crates/xvision-engine/**                                    # engine accepts partial mutations today — no changes
  - crates/**                                                   # frontend only
interfaces_used:
  - ScenarioForm                                                # frontend/web/src/components/scenario/ScenarioForm.tsx
  - ScenarioFormDraft                                           # already exported
  - ScenarioMutations                                           # API request shape (frontend/web/src/api/scenarios.ts)
  - cloneScenario                                               # API client call
  - displayScenarioName                                         # if renaming the clone uses the same canonical accessor
parallel_safe: true                                             # frontend-only, no overlap with active tracks
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/scenarios-detail
  - pnpm --dir frontend/web lint
acceptance:
  - **Inline clone accordion mounts `<ScenarioForm initial={parent}>`.** On `routes/scenarios-detail.tsx`, the existing inline accordion (`:55-306` per recon) gains the structural fields by mounting the already-lifted `ScenarioForm` with the parent scenario hydrated as `initial`. The simple text-field overrides (display_name, description, notes, tags) that #341 commit `53f3e3f` shipped continue to work — this is additive.
  - **Five structural overrides supported.** Operator can change any of `time_window`, `asset`, `granularity`, `venue`, `warmup_bars` before submitting the clone. Each maps to the same `ScenarioMutations` payload field the API accepts today.
  - **Wizard form widgets reused, not duplicated.** The asset picker, date-range picker, granularity selector, and venue-settings widgets are the ones already in `ScenarioForm`. No fork.
  - **No popups.** Per workspace rule (`/CLAUDE.md` no-popups). The inline accordion stays inline — no dialog/sheet/popover for the structural form.
  - **Partial-mutation fidelity.** The form default-fills from the parent and only sends fields the operator actually changed. Verify the engine's `null` = inherit semantics (intake §"Recommended approach" item 3) still hold; the form must not blast unchanged fields back through the API.
  - **Bar-cache key recomputed.** A clone with `asset` / `time_window` / `granularity` changed lands with a different bar-cache key from the parent. Validate via the existing `cloneScenario` response (the new scenario id resolves to a row with the recomputed key).
  - **Simple-text-field clone still works.** A clone that changes only `notes` (the existing path) lands without touching the structural fields. Test asserts the API request payload has no time_window/asset/granularity/venue/warmup_bars keys when the form was not touched.
  - **New test.** `scenarios-detail.test.tsx` asserts: (a) opening the clone accordion mounts the form pre-filled from the parent, (b) changing granularity and submitting sends the new granularity in the `ScenarioMutations` payload, (c) submitting with only notes changed does not include structural fields.

---

# Scope

Followup track carved from QA Round 4 (`team/intake/2026-05-19-qa-operator-round-4.md`,
§"Followups → `scenario-clone-form-structural-fields`"). The
`strategy-clone-editable-frontend` track shipped partially in PR #341
(commit `53f3e3f`) — the simple text-field inline clone-edit form on
`/scenarios/:id` is live for `display_name`, `description`, `notes`,
`tags`. The engine's `ScenarioMutations` payload accepts more:
`time_window`, `asset`, `granularity`, `venue`, `warmup_bars`. These
have cascading effects on the bar-cache key, fetch jobs, indicator
window, and so on, so they need the wizard's existing validation +
preview UX — not a hand-rolled form.

Recon (2026-05-21) confirms `<ScenarioForm>` has already been lifted
into `frontend/web/src/components/scenario/ScenarioForm.tsx` and
already accepts an `initial?: ScenarioFormDraft`-shaped prop, including
asset/time_window/granularity/venue/warmup hydration. So the heavy
lift the intake described ("lift ScenarioForm from `routes/scenarios-new.tsx`
into a shared component") is done. This contract is the second half:
*using* that lifted component inside the inline clone accordion.

# Out of scope

- In-place mutation of scenarios (would need a new `update_scenario`
  API and engine invariants around cache keys). Clone-with-mutations
  stays the canonical authoring path (intake §"Out of scope").
- The new-scenario wizard. `scenarios-new.tsx` is in `forbidden_paths`
  — touching it risks regressions and isn't this track's job.
- Adding new fields to `ScenarioMutations` beyond what the engine
  already accepts. The five structural fields are the engine's current
  surface.
- List-route changes (lists-v1 phase 2c owns `scenarios.tsx`).
- Re-laying-out the detail page. Inline-only, no chrome rework.
- Strategy clone-edit (a separate track shipped via #341).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/scenario-clone-form-structural-fields status
git -C .worktrees/scenario-clone-form-structural-fields log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/scenario-clone-form-structural-fields -b task/scenario-clone-form-structural-fields origin/main
```

# Notes

Recon (2026-05-21) found the post-#341 state:

- `ScenarioForm` already lives at
  `frontend/web/src/components/scenario/ScenarioForm.tsx` and accepts
  `initial?: ScenarioFormDraft` hydration with all five structural
  fields (verified at `:28-138`).
- The inline clone accordion is mounted at
  `frontend/web/src/routes/scenarios-detail.tsx:55-306`, currently
  managing only `cloneNotes`, `cloneDisplayName`, `cloneDescription`,
  `cloneTags`.
- The intake's "Recommended approach" §1 (lift `ScenarioForm`) is
  already done; this track only needs to do §2 (mount it inside the
  inline accordion) and §3 (confirm partial-mutation semantics).

If the existing `ScenarioForm` prop surface is missing a `controlled`
mode that lets the clone accordion both hydrate from parent and
diff-submit only changed fields, add the minimal prop (e.g.
`onChange?(draft)` or `value?` + `onChange?`). Avoid restructuring the
component — keep the new-scenario wizard's call site working.
