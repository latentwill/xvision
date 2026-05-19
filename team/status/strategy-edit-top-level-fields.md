# strategy-edit-top-level-fields — status

## HTTP verb decision

**Chose PATCH** for `/api/strategy/:id`.

Rationale:

- The contract calls out PATCH as preferred.
- The body is a partial-update shape (`StrategyMetadataPatch` with all
  `Option` fields). `None` fields are left unchanged; supplying `{}`
  is a valid no-op. That semantic matches PATCH (RFC 5789) and is
  inconsistent with PUT (which conventionally requires a full
  representation).
- Existing sibling routes already mix verbs by intent:
  - `PUT /api/strategy/:id/slot/:role` and `PUT
    /api/strategy/:id/risk` replace a single named sub-resource.
  - `PATCH /api/strategy/:id/agents/:role` patches a single agent
    role.
- `GET` and `DELETE` already live on `/api/strategy/:id`. Adding
  `PATCH` slots in alongside them on the same path without disturbing
  the existing verbs.

## Scope summary

Adds a top-level metadata patch covering the three fields the QA
operator round-4 intake item 2 flagged:

- `display_name` (title) — non-empty when provided.
- `plain_summary` (description) — non-empty when provided.
- `asset_universe` — non-empty, no blank entries when provided.

Out of scope (per contract): `decision_cadence_minutes` (already has a
dedicated path via `authoring::update_manifest`), `id`, `creator`,
`template`, `published_at`, `risk_preset_or_config`, `agents`,
`pipeline`, `risk`, `mechanical_params`.

## Frontend route note

The contract anchors on `/strategies/:id`. The actual strategy detail
route in this codebase is `/authoring/:id` (the Inspector page). The
contract's `allowed_paths` list scopes the frontend work to a
brand-new `frontend/web/src/routes/strategies-detail.tsx` plus the
`features/strategies/InlineEditField.tsx` component — it does NOT
include `frontend/web/src/routes.tsx` or `frontend/web/src/api/strategies.ts`.

To honor the allowed-paths strictly:

- created `frontend/web/src/features/strategies/InlineEditField.tsx`
  (per contract);
- created `frontend/web/src/routes/strategies-detail.tsx` as a
  self-contained strategy detail page that fetches a strategy by id
  using `apiFetch` and uses InlineEditField for display_name and
  plain_summary. PATCH is invoked via `apiFetch` directly (so we
  don't need to edit the shared `api/strategies.ts`).

The detail page is NOT wired into the router in this PR (routes.tsx
not in allowed_paths). Filed as a one-line follow-up in the PR body:
mount `<StrategyDetailRoute>` at `/strategies/:id`. The PATCH route
and the component are fully exercised by the test suite, and the
existing `/authoring/:id` Inspector page continues to function
unchanged.

## Test plan executed

- `cargo test -p xvision-engine --test strategy_update_metadata`
- `cargo test -p xvision-engine`
- `cargo test -p xvision-dashboard --test strategy_patch_route`
- `cargo test -p xvision-dashboard`
- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web test -- --run InlineEditField strategies-detail`
- `pnpm --dir frontend/web build`
