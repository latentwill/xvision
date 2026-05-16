# Track briefing — `inspector-api`

**Plan:** [Strategy 2d — Dashboard + Wizard](../../docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md), Phase 2D.C Task 8 (backend slice).

**Worktree:** `.worktrees/inspector-api`
**Branch:** `feature/inspector-api-backbone`
**Base:** `origin/main` @ `d23feea`

## Why this track

The Wizard PRs (#36, #39, #41) shipped server-side authoring + the chat UI on `/setup`. The Inspector (`/authoring/<draft_id>`) is the next operator surface — it's where users edit slot prompts, risk presets, and re-validate drafts directly. Without it, the Wizard creates drafts that the user can read but can't directly tune.

`engine::authoring::*` already has the dispatcher fns (`update_slot` / `set_risk_config` / `validate_draft` / `create_strategy`) — landed in PR #36. What's missing: the audit-emitting `api::strategy::*` wrappers + the dashboard PUT routes. The Foundation pattern (PR #4 README) requires every external entry to go through `api::*` with audit, and the dashboard's existing `routes/strategies.rs` only has `list`.

This is the backend slice. Frontend Inspector page (the React form + slot editors) lives in a separate follow-up that depends on this PR's API surface.

## Scope of *this* PR (backend only)

1. `crates/xvision-engine/src/api/strategy.rs` — add four audit-emitting wrappers:
   - `create_strategy(ctx, req: CreateStrategyReq) -> ApiResult<CreateStrategyOut>` — wraps `authoring::create_strategy`. Maps "unknown template" to `Validation`, I/O to `Internal`.
   - `update_slot(ctx, req: UpdateSlotReq) -> ApiResult<UpdateSlotOut>` — wraps `authoring::update_slot`. Maps "draft not found" to `NotFound`, "unknown slot" / "no fields to update" to `Validation`, I/O to `Internal`.
   - `set_risk_config(ctx, req: SetRiskConfigReq) -> ApiResult<SetRiskConfigOut>` — wraps `authoring::set_risk_config`. Maps "unknown preset" / "preset and explicit are mutually exclusive" / "supply either preset or explicit" to `Validation`.
   - `validate_draft(ctx, agent_id: &str) -> ApiResult<ValidateDraftOut>` — wraps `authoring::validate_draft`. Maps "not found" to `NotFound`.
2. `crates/xvision-dashboard/src/routes/strategies.rs` — add four routes proxying to the wrappers:
   - `GET /api/strategy/:id` — full bundle for Inspector render (separate from existing `list` shape).
   - `PUT /api/strategy/:id/slot/:role` — slot fields (prompt / model_requirement / allowed_tools).
   - `PUT /api/strategy/:id/risk` — preset or explicit risk config.
   - `POST /api/strategy/:id/validate` — re-validates the draft for the Inspector's right-rail Validation card.
3. Wire the new routes into the dashboard router.
4. Tests:
   - 4 wrapper unit tests in `xvision-engine` (happy path + at least one error path each, asserting `api_audit` gets the expected row).
   - 1 round-trip dashboard test that exercises `GET /api/strategy/:id` after a `create` + `update_slot`.

## Out of scope (deferred — call out in PR body)

- **Frontend Inspector page** (Plan 2D.C Task 8 templates + JS) — React form with the 7 collapsible layer rows + Validation right rail. Depends on this PR's API surface.
- **Task 8a — LLM split editor + live preview** (Move E) — the 50/50 form/preview split with `POST /api/strategy/:id/slot/:role/preview` SSE. Depends on the FixtureStore from Plan 2D.C Task 8a Step 1, which is its own track.
- **`set_mechanical_param` wrapper + route** — the dispatcher fn exists; defer the wrapper until the frontend needs the field-level mechanical editor (form-level full-bundle PUT covers Inspector's mechanical sections in v1).

## Files this track touches (zero overlap with active sessions)

- `crates/xvision-engine/src/api/strategy.rs` (additive — four new fns + audit wiring)
- `crates/xvision-dashboard/src/routes/strategies.rs` (additive — four new routes)
- `crates/xvision-dashboard/src/routes/mod.rs` (router wiring)
- `crates/xvision-dashboard/src/lib.rs` (router wiring) — only if needed
- `crates/xvision-engine/tests/api_strategy_inspector.rs` (new — round-trip)

Active PRs checked: board is empty as of `d23feea`.

## v1 progress

After this PR + the frontend follow-up, the Inspector authoring surface
closes the §168 success-criterion 2 (operator can author end-to-end), in
combination with the Wizard PRs that already merged.
