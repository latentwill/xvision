---
track: clawpatch-frontend-components
lane: leaf
wave: clawpatch-blockers-2026-05-21
worktree: .worktrees/clawpatch-frontend-components
branch: task/clawpatch-frontend-components
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  # B-6 HealthPill tests
  - frontend/web/src/components/shell/HealthPill.test.tsx          # NEW
  # B-7 CacheStatusBadge tests
  - frontend/web/src/components/scenario/CacheStatusBadge.test.tsx # NEW
  # B-8 AgentForm.duplicateSlot fix + regression test
  - frontend/web/src/components/agent/AgentForm.tsx
  - frontend/web/src/routes/agents.test.tsx
  # B-9 WizardPreviewChart memoization + test
  - frontend/web/src/components/chart/WizardPreviewChart.tsx
  - frontend/web/src/components/chart/WizardPreviewChart.test.tsx  # NEW
  # B-10 SlotForm provider-change + test
  - frontend/web/src/components/agent/SlotForm.tsx
  # B-11 MobileDrawer — see Notes: clawpatch's recommendation conflicts with no-popups rule
  - frontend/web/src/components/mobile/MobileDrawer.tsx
  - frontend/web/src/components/mobile/MobileDrawer.test.tsx       # NEW (only if no-popups rework allows)
forbidden_paths:
  - frontend/web/src/components/shell/HealthPill.tsx               # B-6 is tests-only
  - frontend/web/src/components/scenario/CacheStatusBadge.tsx      # B-7 is tests-only
  - crates/**                                                      # frontend only
  - frontend/web/src/api/types.gen/**                              # generated
interfaces_used:
  - "@testing-library/react"                                       # render, screen, fireEvent
  - "@tanstack/react-query"                                        # QueryClientProvider for B-6/B-9
  - existing component prop surfaces                               # do not change AgentForm/SlotForm/etc. public APIs
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- components/shell/HealthPill
  - pnpm --dir frontend/web test -- components/scenario/CacheStatusBadge
  - pnpm --dir frontend/web test -- components/chart/WizardPreviewChart
  - pnpm --dir frontend/web test -- components/agent/AgentForm
  - pnpm --dir frontend/web test -- components/agent/SlotForm
  - pnpm --dir frontend/web test -- components/mobile/MobileDrawer
  - pnpm --dir frontend/web test -- routes/agents
  - pnpm --dir frontend/web lint
acceptance:
  - **B-6 closed.** `HealthPill.test.tsx` exists and covers: pending/loading state, rejected/offline state, ok state, degraded state, down state, and the title summary built from probes. Tests wrap render under `QueryClientProvider` with mocked `getHealth`. Worker runs `clawpatch revalidate --finding fnd_sig-feat-ui-flow-368150e279-c2d1_425d678994` and confirms closed.
  - **B-7 closed.** `CacheStatusBadge.test.tsx` covers the three status variants, button rendering/click behavior, disabled state, and the fetchStatus-only rendering when `onFetch` is absent. Revalidate closes `fnd_sig-feat-ui-flow-7f1ddf7f4e-f0c9_fc3d85213c`.
  - **B-8 closed.** `AgentForm.duplicateSlot` (around `frontend/web/src/components/agent/AgentForm.tsx:183`) explicitly sets `max_tokens: null` on the new slot, preserving the rest. A regression test in `routes/agents.test.tsx` asserts a duplicated slot has `max_tokens === null` even if the source slot had a stored value. Revalidate closes `fnd_sig-feat-ui-flow-98a40b66c8-6d2a_48f373bf7f`. **Note**: per the existing comment at `AgentForm.tsx:32-36`, the per-slot `max_tokens` field was deliberately removed from the UI. The fix is defensive — ensuring duplicates don't accidentally carry a stale stored value if one exists in the underlying record.
  - **B-9 closed.** `WizardPreviewChart.tsx` memoizes the synthesized `ScenarioChartPayload` via `useMemo` keyed by `query.data`, the debounced `asset/from/to/granularity`, and cache-status inputs. A stable placeholder `created_at` is used for preview payloads. `WizardPreviewChart.test.tsx` asserts the memo identity is stable across unrelated re-renders. Revalidate closes `fnd_sig-feat-ui-flow-f276b9b4f5-53e4_89387de97b`.
  - **B-10 closed.** `SlotForm.tsx`'s provider select handler clears `slot.model` when the chosen provider does not offer the current model (or routes through the same provider/model update path used by `ModelPicker`). A focused interaction test in `routes/agents.test.tsx` or a sibling SlotForm test asserts the model clears on incompatible-provider switch. Revalidate closes `fnd_sig-feat-ui-flow-0e07bcd326-2bbe_8ce24d101a`.
  - **B-11 ESCALATION.** Clawpatch's recommendation for `MobileDrawer.tsx` (give it `role="dialog"` + `aria-modal="true"` + Tab focus trap) directly contradicts the CLAUDE.md no-popups rule. **The worker MUST escalate before implementing the focus-trapping fix as-spec'd.** Two acceptable paths: (a) re-design `MobileDrawer` as a non-modal inline drawer that doesn't steal focus and doesn't need Tab trapping (the no-popups-rule-compliant solution), or (b) explicitly grant `MobileDrawer` a no-popups exemption in CLAUDE.md (operator decision; outside this contract's scope) and *then* implement the focus-management fix. The PR includes a brief note in `Notes:` explaining which path was chosen. If neither path can land, `MobileDrawer` is *not* covered by this contract and B-11 stays open as a known-conflict with the no-popups rule.
  - **No prod regressions.** `pnpm --dir frontend/web test --run` passes (minus pre-existing failures, which the PR enumerates and confirms were red on `origin/main` first).

---

# Scope

Tracks B-6 through B-11 of `team/intake/2026-05-19-clawpatch-blockers.md`.
Six findings, all frontend-component-level fixes or new component tests
that clawpatch's autonomous loop tried and failed to land. Bundled
because they share the same surface category and reviewer mindset.

# Out of scope

- Engine/observability findings (B-1 through B-5). Owned by sibling
  contracts.
- Refactoring shared shell/scenario/agent/chart/mobile component public
  APIs.
- Adding new components.
- Storybook entries or visual-regression tests (out of scope per
  clawpatch's recommendation, which is strictly behavioral).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/clawpatch-frontend-components status
git -C .worktrees/clawpatch-frontend-components log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/clawpatch-frontend-components -b task/clawpatch-frontend-components origin/main
```

# Notes

**B-11 no-popups conflict.** `MobileDrawer.tsx` currently exists (recon
2026-05-21 confirms the file at `frontend/web/src/components/mobile/MobileDrawer.tsx`).
Clawpatch's recommendation to add `role="dialog"` + `aria-modal="true"`
+ focus-trapping would make `MobileDrawer` a popup in the meaningful
sense — focus-stealing, Escape-to-close, Tab-trapped — which violates
the project rule in CLAUDE.md ("dashboard SPA does not use popups,
modals, sheets, popovers, or any overlay that steals focus or paints
over the primary surface").

The only sanctioned overlay exception is `<MListSheet>` (operator-
approved 2026-05-20 for mobile list filters). `MobileDrawer` is not
on the exemption list. The worker has two compliant paths:

1. **Refactor MobileDrawer into a non-modal inline drawer** that
   doesn't trap focus or paint over the primary surface — typically
   a side-rail or accordion shape. This is the default and aligns
   with the CLAUDE.md rule.
2. **Get an explicit operator exemption** added to CLAUDE.md (as a
   second exception line under `<MListSheet>`) before implementing
   the focus-trapping fix. This is operator-only; the worker can't
   add the exemption themselves.

Either path resolves B-11. Doing neither and leaving the finding open
is also acceptable — the contract's acceptance criterion explicitly
permits the "leave open and escalate" outcome.

**Recon findings to save the worker time:**

- `HealthPill.tsx`, `CacheStatusBadge.tsx`, `WizardPreviewChart.tsx`,
  `MobileDrawer.tsx`, `AgentForm.tsx`, `SlotForm.tsx` all exist at
  their expected paths.
- `AgentForm.tsx:32-36` has an explicit "do not bring `max_tokens` back
  in any downstream refactor" comment. The B-8 fix is one
  defensive-null line; honor the existing convention.
- `frontend/web/src/components/agent/SlotForm.tsx` already reads from
  `settingsKeys.providers()` (line 36) — the model-clearing logic
  hooks into the same place the provider select renders.
