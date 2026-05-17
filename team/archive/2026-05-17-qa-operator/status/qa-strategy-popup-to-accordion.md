---
track: qa-strategy-popup-to-accordion
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision conductor session)
pr: 230
commits:
  - c304dad — qa: replace strategy-detail popup with inline accordion; merge attach surfaces
---

## Outcome

PR #230 open. Branch pushed.

Two operator fixes shipped on `authoring.tsx`:
- Per-agent `role="dialog"` overlay removed; folded into existing
  `▶/▼` inline expansion in `AttachedAgentRow`.
- "Attach existing" + "Create and attach" merged into one
  `AddAgentAccordion` component with a `Pick existing / Create new`
  tab toggle.

## Verification

- `pnpm --dir frontend/web typecheck` — PASS
- `pnpm --dir frontend/web test -- --run authoring` — 13/13 PASS
- `pnpm --dir frontend/web build` — PASS
- (`pnpm lint` script absent on `origin/main`; not a regression.)

## Path drift handled

Contract originally listed `strategies-new.tsx` but the operator's
"popup" and "Attach Existing / Create and Attach" surfaces all live on
`authoring.tsx`. Widened `allowed_paths` + declared multi-owner in
OWNERSHIP.md (shared with `qa-ui-micro-fixes` — disjoint changes:
whitespace + Run Eval card removal vs popup → accordion).

## Out-of-scope reminders honored

- No max_tokens UI change (owned by PR #223).
- No new agent attach paths beyond merging the two existing flows.
- No `border-white` / `border-gray-100/200` in the new accordion.
