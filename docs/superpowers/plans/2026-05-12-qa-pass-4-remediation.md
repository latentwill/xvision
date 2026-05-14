# QA Pass 4 Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close QA pass 4 by making the shipped dashboard match operator expectations: no placeholder LLM default, editable settings, no dead home chrome, persistent chat, full eval-capable agent tools, 4H scenarios, responsive cache/CLI handling, and reliable list refresh.

**Architecture:** Treat this as a state-contract repair, not a styling pass. First reconcile the hidden `origin/codex/qa-pass-4` work against current `main` and port only still-needed code. Then fix backend capability gaps with focused API contracts, wire the frontend to those contracts with TanStack invalidation, and lock the chat agent to real strategy/eval/scenario tools. Finish with deploy verification against the same GHCR image QA will use.

**Tech Stack:** Rust workspace (`xvision-core`, `xvision-data`, `xvision-engine`, `xvision-dashboard`), Axum, SQLx/SQLite, TOML config via `toml_edit`, React 18 + TypeScript + TanStack Query, Vite/Vitest, `cargo`, `pnpm`, GitHub Actions/GHCR.

---

## Triage Findings

- `origin/codex/qa-pass-4` exists at `94ebd3a` and is not what is deployed on `main`. It contains remote CLI job API work, extra chat rail/strategy route work, and authoring/chart changes.
- Current `main` contains `bd53231` plus later build fixes, so QA work was not totally deleted. Some fixes landed, some were squashed without later branch work, and some current source still contradicts QA asks.
- Current source still renders the home `HealthCard` and `On-chain identity` card in `frontend/web/src/routes/home.tsx`.
- Current source uses `ReactMarkdown` in `frontend/web/src/components/shell/ChatRail.tsx`; if QA still sees raw markdown, either deployed assets are stale or a historical/session rendering path still fails. This needs a browser-level regression test, not only source inspection.
- Provider removal is blocked by the required `[default_llm]` model in `crates/xvision-core/src/config.rs` and `crates/xvision-engine/src/api/settings/providers.rs`. The synthetic `_default_llm` row is intentional in current code, but QA explicitly rejects it.
- Provider update is missing. Backend exposes add/remove/default/models, not edit/replace.
- Broker "replace" exists in UI, but it is a weak edit flow and has no focused regression test.
- `4H` is impossible because `BarGranularity` lacks `Hour4`, scenario validation only accepts `Hour1 | Day1`, and `ScenarioForm` only offers `1h | 1d`.
- Chat agent tools are still too narrow: no `list_strategies`, no `list_scenarios`, no strategy-name/scenario-name resolution, no existing-context strategy inference. That explains the template-only behavior.
- Eval launcher defaults to `paper`; backtest/eval still may instantiate LLM provider paths and returns raw `ANTHROPIC_API_KEY` validation instead of actionable UI state.
- Eval scenario dropdown uses `/api/eval/scenarios` with `staleTime: 5m`, so newly created scenarios do not appear immediately.
- New Agent tags are controlled from `draft.tags.join(", ")`, so typing comma-separated tags collapses intermediate input. Scenario tags work because that form keeps tag text/draft state sanely separated.
- Agent editor labels still say `Identity` and `Behavior`; slot model is a free text input instead of the provider model picker.

---

## Task 1: Reconcile Hidden Branch Work Without Regressing Main

**Files:**
- Inspect: `origin/codex/qa-pass-4`
- Modify as needed: `crates/xvision-dashboard/src/cli_jobs/*`
- Modify as needed: `crates/xvision-dashboard/src/routes/cli.rs`
- Modify as needed: `crates/xvision-engine/migrations/013_cli_jobs.sql`
- Modify as needed: `crates/xvision-dashboard/src/server.rs`
- Test: `crates/xvision-dashboard/tests/cli_jobs_routes.rs`

- [ ] Create a temporary comparison worktree or use `git show origin/codex/qa-pass-4:<path>` to review the remote CLI job implementation.
- [ ] Port only the remote CLI job API pieces needed for responsive UI actions: job create/status/output/cancel, SSE/output polling, persistent SQLite tables, and server routes.
- [ ] Do not wholesale merge `origin/codex/qa-pass-4`; it diverges from current `main` and would reintroduce removed marketing/docs/skills changes.
- [ ] Run `cargo test -p xvision-dashboard cli_jobs -- --nocapture`.
- [ ] Add a small frontend wrapper later in Task 8 for `xvn bars fetch` rather than making the UI wait on impossible manual CLI calls.

## Task 2: Remove Home Dead Chrome

**Files:**
- Modify: `frontend/web/src/routes/home.tsx`
- Test: `frontend/web/src/routes/home.test.tsx`

- [ ] Delete the `getHealth` query, `HealthCard` render, and local health summary from the home route.
- [ ] Delete the `getIdentity` query and the `On-chain identity` card from the home route.
- [ ] Rename `Control Tower` to `Dashboard` everywhere still visible on the home route.
- [ ] Keep useful counts and latest eval chart, but do not show local daemon or blockchain identity status on home.
- [ ] Add/adjust a Vitest assertion that `Dashboard` renders and `/local health|on-chain identity|identity/i` do not.
- [ ] Run `pnpm --dir frontend/web test -- home.test.tsx`.

## Task 3: Make LLM Provider Settings Truly Editable and Remove Required Placeholder Default

**Files:**
- Modify: `crates/xvision-core/src/config.rs`
- Modify: `crates/xvision-engine/src/api/settings/providers.rs`
- Modify: `crates/xvision-dashboard/src/routes/settings/providers.rs`
- Modify: `crates/xvision-dashboard/src/llm_dispatch.rs`
- Modify: `frontend/web/src/api/settings.ts`
- Modify: `frontend/web/src/routes/settings/providers.tsx`
- Test: `crates/xvision-core/src/config.rs`
- Test: `crates/xvision-engine/src/api/settings/providers.rs`

- [ ] Change runtime config so a workspace can have zero configured providers and no functional default LLM. Remove synthetic `_default_llm` from the provider list response.
- [ ] Replace "default is required" behavior with "explicit provider/model is required when no default is set" in `llm_dispatch::resolve`.
- [ ] Allow deleting the current default provider. On delete, clear `[default_llm]` instead of forcing the operator to create a replacement.
- [ ] Add `UpdateProviderRequest` with editable `kind`, `base_url`, `api_key_env`, optional `api_key`, and optional `enabled_models`.
- [ ] Add `PUT /api/settings/providers/:name` and use `toml_edit` to update the existing row in place; invalidate model cache for that provider.
- [ ] Replace provider row delete restriction copy in UI with direct edit/delete affordances.
- [ ] The provider model picker must show empty state as "No provider configured" rather than pointing at a placeholder.
- [ ] Tests: config loads with no providers and no default; deleting default succeeds and list returns zero providers; dispatch without provider/default returns a 400 with a setup message; provider update edits TOML and stored secret.

## Task 4: Fix Broker Replace UX

**Files:**
- Modify: `frontend/web/src/routes/settings/index.tsx`
- Modify if needed: `crates/xvision-engine/src/api/settings/brokers.rs`
- Test: add dashboard HTTP test or frontend test around broker replace

- [ ] Turn "replace" into explicit edit mode with `Save replacement`, `Cancel`, and clear validation messages.
- [ ] After saving replacement, invalidate `settingsKeys.brokers()` and render the new redacted suffix.
- [ ] Ensure the user can replace without clearing first.
- [ ] Add a regression test that stores one Alpaca key, replaces it, then observes the new suffix through `GET /api/settings/brokers`.

## Task 5: Add 4H Bars and Scenario Support

**Files:**
- Modify: `crates/xvision-data/src/alpaca.rs`
- Modify: `crates/xvision-engine/src/api/scenario.rs`
- Modify: scenario seed/cache helpers if they pattern-match granularities
- Modify: `frontend/web/src/components/scenario/ScenarioForm.tsx`
- Modify: `frontend/web/src/components/chart/WizardPreviewChart.tsx`
- Modify: generated TS types via `cargo xtask gen-types`
- Test: `crates/xvision-engine/tests/*scenario*`

- [ ] Add `BarGranularity::Hour4` and map it to Alpaca timeframe `4Hour`.
- [ ] Accept `Hour4` in scenario create/clone validation.
- [ ] Update cache key, expected bar count, preview, chart labels, and form radio options to include `4h`.
- [ ] Add a regression test creating a `Hour4` crypto scenario and previewing/building cache status.
- [ ] Run `cargo test -p xvision-engine scenario -- --nocapture` and `pnpm --dir frontend/web typecheck`.

## Task 6: Expand Chat Agent Tools Beyond Templates

**Files:**
- Modify: `crates/xvision-dashboard/prompts/wizard.md`
- Modify: `crates/xvision-dashboard/src/wizard_loop.rs`
- Modify: `frontend/web/src/components/shell/ChatRail.tsx`
- Test: `crates/xvision-dashboard/src/wizard_loop.rs`

- [ ] Add tool definitions and handlers for `list_strategies`, `list_scenarios`, `get_scenario`, and `resolve_strategy`.
- [ ] Let `run_eval` accept either ids or names, resolving "the strategy we have" from current scope/history when unambiguous.
- [ ] Update the prompt hard rules: use existing strategies/scenarios before asking for templates; do not ask for a template when an existing strategy and scenario can be resolved.
- [ ] Add wizard-loop tests for: "tell me what strategies I have"; "run an eval on the strategy we have scenario crypto range bound"; ambiguous strategy asks one clarifying question; missing bars returns the UI action from Task 8.
- [ ] Update ChatRail tool transcript labels for the new tools.

## Task 7: Make Eval Launch Failures Actionable

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Modify: `crates/xvision-dashboard/src/routes/eval_runs.rs`
- Modify: `frontend/web/src/routes/eval-runs.tsx`
- Test: dashboard HTTP eval route tests

- [ ] Default web eval launcher to `backtest`, not `paper`.
- [ ] Add preflight validation for provider/model and broker credentials before queueing a run.
- [ ] Convert raw `ANTHROPIC_API_KEY env var is required` into a structured 400 with action copy: pick a configured provider/model or run with a deterministic/non-LLM baseline if available.
- [ ] Surface backend errors inline in the dialog and keep the dialog open.
- [ ] Add a test for missing provider key returning the structured validation error.

## Task 8: Make Bars Cache Actions Responsive in UI

**Files:**
- Depends on Task 1 remote CLI job API
- Modify: `frontend/web/src/api/cli.ts`
- Modify: `frontend/web/src/components/scenario/CacheStatusBadge.tsx`
- Modify: `frontend/web/src/components/chart/ScenarioChart.tsx`
- Modify: `frontend/web/src/routes/scenarios-detail.tsx`
- Modify: `frontend/web/src/components/chart/WizardPreviewChart.tsx`

- [ ] Replace static "run `xvn bars fetch`" copy with a `Fetch bars` button when cache status is `NotCached` or `PartiallyCached`.
- [ ] Button creates a CLI job: `["xvn", "bars", "fetch", "--asset", asset, "--granularity", granularity, "--from", from, "--to", to]`.
- [ ] Show queued/running/output/error states and poll or stream job output.
- [ ] Invalidate scenario chart/preview queries when the job completes.
- [ ] Add frontend test that NotCached renders a button, clicking it starts a job, and completion invalidates chart query keys.

## Task 9: Fix List Refresh Across Create/Edit Flows

**Files:**
- Modify: `frontend/web/src/routes/scenarios-new.tsx`
- Modify: `frontend/web/src/routes/eval-runs.tsx`
- Modify: `frontend/web/src/routes/scenarios-detail.tsx`
- Modify: `frontend/web/src/api/eval.ts`

- [ ] Stop using the stale `/api/eval/scenarios` list for the eval launcher, or invalidate it whenever scenarios mutate.
- [ ] Prefer `/api/scenarios` for the launcher so custom scenarios and canonical scenarios share one source.
- [ ] On scenario create/clone/archive/delete, invalidate `scenarioKeys.all`, `evalKeys.scenarios()`, and any launcher query keys.
- [ ] On strategy create/update/archive, invalidate `strategyKeys.all`.
- [ ] Add tests for "new scenario appears in Start eval without reload" and "new strategy appears in Start eval without reload."

## Task 10: Repair Agent Form Tags, Labels, and Model Selection

**Files:**
- Modify: `frontend/web/src/components/agent/AgentForm.tsx`
- Modify: `frontend/web/src/components/agent/SlotForm.tsx`
- Modify: `frontend/web/src/components/ModelPicker.tsx` if needed
- Test: add/update agent form tests

- [ ] Rename `Identity` card to `Profile`.
- [ ] Rename `Behavior` card to `Agent`.
- [ ] Replace free-text slot model input with `ModelPicker`, filtered by the selected provider.
- [ ] Disable model selection until a provider is selected; clear model when provider changes.
- [ ] Replace direct `draft.tags.join(", ")` control with a text buffer or shared tag input so typing comma-separated tags works.
- [ ] Add tests that `alpha, beta` saves as two tags and that model options come from enabled provider models only.

## Task 11: Verify Markdown and Chat History With Browser Tests

**Files:**
- Modify if needed: `frontend/web/src/components/shell/ChatRail.tsx`
- Test: add Vitest/Playwright coverage for chat rail

- [ ] Add a test that an assistant message containing `**bold**` and `* item` renders as `<strong>` and `<li>`.
- [ ] Add a test that changing routes, then returning to the same scope, keeps or reloads the same session history.
- [ ] If the source test passes but QA still fails, treat it as deploy artifact staleness and validate the baked SPA in the Docker image.

## Task 12: Build and Deploy Verification

**Files:**
- Modify if needed: `.github/workflows/docker.yml`
- Verify: GHCR image and QA stack

- [ ] Run `cargo test -p xvision-core -p xvision-engine -p xvision-dashboard`.
- [ ] Run `pnpm --dir frontend/web typecheck`.
- [ ] Run `pnpm --dir frontend/web test`.
- [ ] Build the same Docker target used by QA and verify the embedded SPA contains the updated labels (`Dashboard`, no `On-chain identity`).
- [ ] Redeploy QA from the rebuilt image and confirm `/api/version` or image digest matches the commit being tested.
- [ ] Smoke test: edit provider, delete last provider, replace broker, create 4H scenario, fetch bars from UI, create scenario then start eval sees it, chat agent lists strategies, chat agent runs eval by existing strategy/scenario names.

---

## Execution Notes

- Do not merge `origin/codex/qa-pass-4` directly. Port its remote CLI job pieces intentionally.
- Prioritize Tasks 2, 3, 5, 6, 8, and 12 for QA-visible correctness.
- Provider default semantics are the highest-risk backend change because `RuntimeConfig.default_llm` is currently required and many callers assume it exists.
- Chat markdown appears implemented in current source, so verify against the shipped artifact before rewriting the renderer.
