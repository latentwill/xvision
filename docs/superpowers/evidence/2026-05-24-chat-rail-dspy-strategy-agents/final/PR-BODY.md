# Chat rail unification + DSPy optimizer + strategy-agent improvements

Implements `docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md`
(Phases 0–5) on `feat/chat-rail-dspy-strategy-agents`, based on the in-review
`feat/cline-runtime-unification` branch. 82 commits.

## What shipped (all tested; evidence in `docs/superpowers/evidence/2026-05-24-chat-rail-dspy-strategy-agents/`)

**Phase 1 — chat rail foundation.** A single `UnifiedEvent` taxonomy
(adjacently tagged `{kind,data}`) that both the chat rail and trace dock project
from, with `RunEvent`→ and `WizardEvent`→ projectors; a persisted session event
log (`session_events`, migration 042) + per-session broadcast + a unified SSE
stream `GET /api/chat-rail/sessions/:id/stream?after_seq=` (replay→`replay_complete`
→live tail, reconnect/resume by session_id); durable session rail-state
(migration 041); a pure `reduceRows` reducer feeding one-source rail + dock.
Evidence incl. a live SSE capture + a real dashboard screenshot.

**Phase 2 — chat rail safety.** Server-side Research/Act enforcement
(DB-authoritative; a write tool in Research mode fails closed, a spoofed client
mode can't bypass); three-state tool policy (migration 043; disabled tools hidden
from the model); focus chain (`$XVN_HOME/scopes/<kind>/<id>/focus.md`, strict
path-safety, loaded on session start + re-injected per turn); content-addressed
checkpoints with byte-identical restore (migration 044), snapshotted before every
mutating tool; a tool-row registry; and a hook engine (blocking/async, timeout,
retries, fail-open/closed, max-concurrency).

**Phase 3 — DSPy/DSRs offline optimizer.** `xvision-dspy` crate (`dspy-rs 0.7.3`,
**non-default-member; `xvision-engine` verified free of dspy-rs/rig-core**),
capability signatures + `DummyLM` deterministic adapter, an optimization store
(migration 045: runs/candidates/snapshots/demos/lineage, reproducible-from-inputs),
the `xvn optimize` CLI (run/inspect/export-demos/import-demos/accept-as-child/
revert/explain-missing-data; distinct exit codes 10–15), and dashboard optimizer
surfaces (run detail, candidate table, prompt diff, accept/revert).

**Phase 4 — strategy agents.** Capability-completeness diagnostics (typed
statuses + `assert_launchable` gate wired into eval `start_run` — a strategy
missing a required capability never launches); no-short-circuit guardrails (10
distinct typed codes, fired at live call sites: launch preflight, schema-recovery
exhaustion, stale-optimized-prompt); holdout discipline (migration 046; accept
refused without a holdout unless a recorded override; overfit blocks marketplace
mint unless waived); checkpointed reversible strategy swap-to-child; the agent
diagnostics/readiness/mint UI surfaces.

**Cross-cutting.** Two latent bugs the evidence gates surfaced and fixed: a serde
`kind`-field collision in the event taxonomy, and migrations 041/042 never being
wired into the runtime `ApiContext::open` registry (the engine uses a hand-rolled
registry, not `sqlx::migrate!`). Operator/contributor docs + the three `xvision-*`
skills updated; `check_agent_docs.sh` repointed off its stale pre-split path.

## Validation (Phase 5 gate — see `final/release-gate.txt`)
- `pnpm --dir frontend/web build` (tsc -b && vite build): OK. Frontend reducer/
  store/diagnostics vitest green.
- Cargo lib + integration suites GREEN across observability/engine/dashboard/dspy
  (chat_session 49, diagnostics 18, guardrails 26, mint 24, optimization 9,
  checkpoint 6, focus 12, hooks 9, unified_event 6; integration: unified_stream 3,
  safety 11, checkpoints 3, focus 7, optimizations 7, diagnostics 6, mint_holdout
  10, launch-gate 4, invalid-schema 1; dspy 5).
- `docs-freshness-lint.sh` + `check_agent_docs.sh`: OK.

## Intentional deferrals (named per the plan's deferral policy; each fails safe)
1. **NeedsApproval interactive approve→resume round-trip** (Phase 2.3). Policy is
   decided and a `ToolPolicyChecked{NeedsApproval}` event is emitted; at execution
   the tool is **blocked pending approval** (never auto-executed). The interactive
   resume flow is deferred. Fail-safe: blocks, not silent execution. Deny paths
   (research-mode write, disabled tool) are fully wired + tested.
2. **Hook-engine live wiring.** The hook runner is built + tested (9 tests) but
   not yet invoked at the chat-route event-commit point (the documented wiring
   site). Fail-safe: Research/Act + tool-policy enforcement is independently wired
   and tested, so safety does not depend on hooks firing. Tracked.
3. **Four guardrail call sites** (`disabled_tool`, `empty_demo_set`,
   `filter_signal_requested_but_absent`, `dashboard_artifact_without_persisted_row`)
   are detected + typed + unit-tested but not yet wired into their live sites
   (the wiring track scoped to eval/dispatch). `strategy_references_unattached_slot`
   is wired as defense-in-depth but pre-empted upstream by `resolve_agent_slots`
   (documented in-code). Tracked.
4. **`optimizer_version` provenance** records the workspace crate version string
   rather than the pinned `dspy-rs 0.7.3`. Cosmetic provenance polish.

## Pre-existing failures (NOT introduced here — see `final/release-gate.txt`)
`tests/strategy_clone_model.rs` compile gap (multi-asset `PublicManifest`),
`api_eval_run.rs` review-autofire test, 14 frontend vitest + 6 wizard_loop unit
tests — all confirmed on baseline by the respective tracks.

## Base-branch note
This branch is based on the unpushed/in-review `feat/cline-runtime-unification`.
Do not merge to `main` before that lands; rebase if the Cline review changes its
commits.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
