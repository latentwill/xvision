---
track: strategy-edit-top-level-fields
lane: integration
wave: qa-operator-2026-05-19
worktree: .worktrees/strategy-edit-top-level-fields
branch: task/strategy-edit-top-level-fields
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/src/strategies/manifest.rs
  - crates/xvision-engine/src/strategies/mod.rs
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/tests/strategy_update_metadata.rs
  - crates/xvision-dashboard/src/routes/strategies.rs
  - crates/xvision-dashboard/src/server.rs
  - crates/xvision-dashboard/tests/strategy_patch_route.rs
  - frontend/web/src/routes/strategies-detail.tsx
  - frontend/web/src/features/strategies/InlineEditField.tsx
  - frontend/web/src/features/strategies/InlineEditField.test.tsx
  - frontend/web/src/api/types.gen/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/strategies/agent_ref.rs
  - crates/xvision-engine/src/strategies/risk.rs
  - crates/xvision-engine/src/strategies/slot.rs
  - crates/xvision-engine/src/strategies/templates.rs
interfaces_used:
  - StrategyStore (save/load semantics)
  - PublicManifest (title = display_name, description = plain_summary)
  - DashboardError surfacing (per #256 conventions)
verification:
  - cargo test -p xvision-engine --test strategy_update_metadata
  - cargo test -p xvision-engine
  - cargo test -p xvision-dashboard --test strategy_patch_route
  - cargo test -p xvision-dashboard
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run InlineEditField strategies-detail
  - pnpm --dir frontend/web build
acceptance:
  - Engine: new `StrategyStore::update_metadata(id, patch)` taking a
    partial struct `StrategyMetadataPatch { display_name: Option<String>,
    plain_summary: Option<String>, asset_universe: Option<Vec<String>> }`.
    `None` fields are left unchanged. Validates each provided field
    against the same constraints `post_create` enforces (display_name
    non-empty; plain_summary non-empty if provided; asset_universe
    entries valid AssetSymbol strings). Returns the updated `Strategy`.
  - Engine: out of scope for the patch — `id`, `creator`, `template`,
    `published_at`, `risk_preset_or_config`, `agents`, `pipeline`,
    `risk`, `mechanical_params`. Those have dedicated routes
    (slot/agents/pipeline/risk) or are immutable post-create.
  - Dashboard: register `PATCH /api/strategy/:id` routing to
    `strategies::patch_metadata`. Request body shape mirrors the patch
    struct above. Returns the updated `Strategy` as `200 OK`. On
    validation failure, returns `400` with a classified
    `DashboardError` carrying an operator-readable remediation message
    (consistent with the #256 convention — do not regress to a raw
    400 with empty body).
  - Frontend: inline-edit affordance for display_name and plain_summary
    on `/strategies/:id`. Per the no-popup rule, this MUST be inline
    (text-input or contenteditable replacing the heading on click /
    edit-mode toggle — no modal, no sheet, no popover). Save persists
    via the new PATCH route; cancel restores the prior value; errors
    surface as a toast (toasts are the documented exception to the
    no-popup rule) with the remediation message from the dashboard.
  - Frontend: keep the strategy id stable across edits — the route
    parameter `:id` doesn't change, no router push needed. Confirm by
    component test that an edit cycle does not unmount the detail view.
  - Engine test: HTTP-level test (`strategy_patch_route.rs`) creates a
    strategy, patches display_name, GETs the strategy, asserts the
    field updated and `id` is unchanged. Second case: patch with an
    empty display_name returns 400 with a classified error. Third
    case: patch with no fields (`{}`) returns 200 unchanged (no-op
    patch is valid).
  - Engine test: cycle-id-stable round-trip — edit a strategy that has
    completed eval runs against it, confirm subsequent runs still
    resolve the same `strategy_id == agent_id` ULID and the existing
    runs remain queryable.
  - No new migration. Strategy storage already supports partial save
    (the slot/agents/pipeline/risk routes already write back full
    strategies; this is the same pattern).
  - No `try/catch` silencing (`feedback_alpha_root_cause`).
  - Frontend: extend `frontend/web/src/api/types.gen/**` regeneration
    to include the new patch endpoint + request/response types.
parallel_safe: true
parallel_conflicts:
  - "frontend/web/src/api/types.gen/**: generated file — any other track touching engine types will also regenerate. Coordinate via team/queue/ if there's a clash; conventionally the last-merged wins and earlier branches re-run codegen on rebase."
---

# Scope

Operator can't edit a strategy's top-level fields (title, description,
asset universe) after creation. The strategies route surface today
exposes:

```
GET    /api/strategy/:id
DELETE /api/strategy/:id
PUT    /api/strategy/:id/slot/:role
POST   /api/strategy/:id/agents
DELETE /api/strategy/:id/agents/:role
PATCH  /api/strategy/:id/agents/:role
PUT    /api/strategy/:id/pipeline
PUT    /api/strategy/:id/risk
```

There is no PATCH for the top-level `PublicManifest` fields. Operator
hit this on any typo in the create wizard — the only escape is
delete-and-recreate, which loses the strategy id and orphans any
existing eval runs that reference it.

This track ships the smallest possible patch route covering the
fields the create wizard already collects: `display_name` (title),
`plain_summary` (description), and `asset_universe` (multi-asset
support is in flight via the parallel multi-asset intake). It does
NOT ship strategy versioning, drafts, or an audit trail — those are
V2/V3 territory per the original intake.

Anchor reading:

- `team/intake/2026-05-19-qa-operator-round-4.md` item 2.
- `crates/xvision-engine/src/strategies/manifest.rs` for the
  `PublicManifest` field set.
- `crates/xvision-dashboard/src/routes/strategies.rs:33+` for the
  existing route handler patterns to mirror.

# Out of scope

- Strategy versioning (draft vs published). V2/V3.
- Audit trail / change history. V3+.
- Editing `agents`, `pipeline`, `risk`, `mechanical_params` — those
  have their own dedicated routes already.
- Editing `id`, `creator`, `template`, `published_at`,
  `risk_preset_or_config` — those are immutable post-create.
- Mint-time freeze logic (top-level fields locked once
  `agent_id` is published as an NFT token). Reasonable v1 answer:
  this is a pre-publish-only API; the NFT publish flow doesn't
  exist yet. Reasonable v2 answer: when publish ships, gate this
  route to reject on published strategies. For now, no gate.
- CLI parity (`xvn strategy edit` verb). Tempting but not blocking.
  File as a follow-up if operator asks.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/strategy-edit-top-level-fields status
git -C .worktrees/strategy-edit-top-level-fields log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/strategy-edit-top-level-fields \
  -b task/strategy-edit-top-level-fields origin/main
```

# Notes

Append checkpoints / PR links below. The choice of HTTP verb
(PATCH preferred; PUT acceptable if existing route conventions
demand it) is acceptance-bearing — document the choice and the
reasoning in the status note before opening the PR.
