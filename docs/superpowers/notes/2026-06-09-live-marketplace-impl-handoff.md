# Handoff — Live Trading + Marketplace blockchain implementation

> **Written:** 2026-06-09 (session paused for Claude Code restart)
> **Read this first, then the synthesis plan it executes.**

## What this is

Executing the synthesized blockchain implementation plan:
**`docs/superpowers/plans/2026-06-08-blockchain-implementation-synthesis.md`** (on `origin/main`).

That plan reconciles the old nav doc (`plans/2026-05-26-blockchain-plan-navigation.md`)
with the new spec (`specs/2026-06-08-live-trading-marketplace-spec.md`). Read the
synthesis plan §1 (build state), §4 (amendments), §5 (the two execution tracks),
§6 (manual deploy runbook). **User directive:** "Begin working on all items, stopping
only once manual intervention is required for deployment."

## Status as of pause

- **Docs are done and on `origin/main`** (commit `fee2210`): synthesis plan + new spec.
- **No code written yet.** Session investigated A1 and hit a blocker (below).
- **Worktree:** `.claude/worktrees/live-marketplace-impl`, branch
  `worktree-live-marketplace-impl`, based on `origin/main`. Clean except this handoff.
  Only this doc is committed on the branch; the worktree is otherwise disposable.

## ⚠️ The blocker that paused the session

**Subagents are credit-blocked** — spawning any `Agent` returns *"Usage credits
required for 1M context"*. The whole program is meant to run **subagent-driven**
(user's standing preference: memory `feedback_always_subagent_driven_execution` —
"Always 1"). Solo execution can't realistically cover a ~15-unit, multi-week program
in one session.

**To resume properly: enable `/usage-credits` first**, then drive subagent-driven.
If staying solo, focus the priority Live Trading track (A1→A2→cockpit) only.

## Hard guardrails (carry forward)

- **`VenueLabel::Live` (real-money) stays OFF.** Do not enable real-capital trading
  autonomously — it's gated/V4 in the plan and is an irreversible outward action.
  Per-run pause and the cockpit operate on paper/testnet venues only.
- **Worktree isolation** (CLAUDE.md): never work in the main checkout
  (`/Users/edkennedy/Code/xvision`) — it holds other agents' WIP. Use this worktree
  or a fresh one off `origin/main`.
- **Cargo:** `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-live-mp"`; build via
  `scripts/cargo` (disk guard), not bare cargo. Per-parallel-agent target suffix if
  fanning out.
- **Decisions already locked** (in the synthesis plan): **AM3** ERC-8004 agent =
  **strategy/listing**, NOT lineage (lineage/derivatives deprioritized). **AM1**
  ERC-2981 secondary royalty **dropped** (1155 infinite-supply + soulbound ⇒ no
  secondary market).

## Work units (the task list)

Order = dependency/priority. Tracks A,B off-chain; Track C is the chain code up to the
deploy wall.

1. **A1 — Per-strategy pause (backend).** *In progress; fully mapped, see below.*
2. **A2 — Stop + close positions on cancel.** Extend cancel (`xvision-engine/src/api/eval.rs:~420`) to close open broker positions (from `eval_decisions`, opens w/o matching close) via the broker surface before terminating; persist closes so equity/PnL settle.
3. **B — Live Trading cockpit UI.** New spec §2: strategy strip + column picker, wallet banner, account stat strip, active positions table, pause/resume/stop transport controls, `LiveStrategiesSection` home replacement, `/live/:id` deep-link. **Backend SSE already streams** equity (`EquityPoint`) + decisions (`LiveDecisionRow.pnl_realized`) at `xvision-engine/src/api/chart.rs:1093`. Frontend lives in `frontend/web/src/routes/live*.tsx`; current cockpit is the minimal `LiveChartV2Container` only.
4. **C1 — Contract finishing.** Apply **AM3** (agent = strategy/listing): repurpose subgraph `Lineage` entity per-strategy + adjust `register()` call sites. Verify `forge build && forge test` green (**AM9** — audit hit an OZ v5.0.2 `mcopy` vs `evm_version=shanghai` issue, though #627 reported 58/58; reconcile, cf. memory PR #630).
5. **C4 — On-chain drivers.** Implement `Erc8004MantleDriver` 4 verbs (`crates/xvision-marketplace/src/adapter.rs:194`, currently `NotImplemented`) + `PinataDriver` put/get (`ipfs.rs:46`). Code only; live-chain test is deploy-gated.
6. **C6 — Attestation engine.** 20-trade rolling trigger → sharpe-delta vs listed → verdict → license-gated `giveFeedback` via `IdentityClient::post_reputation` (`xvision-identity/src/client.rs:349`, real but unwired). Connect off-chain Ed25519 `attest()` (`eval.rs:4095`) per **AM4**.
7. **C8 — Frontend marketplace activation (non-deploy).** Settings → Marketplace opt-in tab; real `MarketplaceData` API client (swap `FixtureMarketplaceData` at `frontend/web/src/features/marketplace/routes/MarketplaceLayout.tsx:7`; subgraph-backed parts are deploy-gated); express-deploy CTA on receipt; purchased-strategy badge + Source filter in `/strategies`; TESTNET labels.

**Not autonomously doable — surface to operator:** AM2 (canonical gen-art renderer: Rust
SVG vs frontend canvas — they differ from same seed), C5 (validation-registry signer
service — undesigned), and all §6 deploy steps.

## A1 detailed map (resume here)

Goal: per-run `paused` flag honored in the executor (additive to global pause), with
pause/resume routes. Do TDD.

- **Table:** `eval_runs` (confirmed). Add `paused` BOOL default 0 + `paused_at` nullable.
- **Migration:** next is **061**. Create `crates/xvision-engine/migrations/061_eval_run_paused.sql` (+ `.down.sql`).
  Migrations are NOT `sqlx::migrate!` — they register as `include_str!` constants in
  **`crates/xvision-engine/src/api/mod.rs`** (~line 100-170) applied via guarded
  `migrate_*` fns in `ApiContext::open`. Simple `ALTER TABLE … ADD COLUMN` guarded on
  column existence — mirror how `041_chat_session_rail_state` / `031_eval_runs_venue_label`
  are applied.
- **Model:** `Run` struct + `RunStatus` in `crates/xvision-engine/src/eval/run.rs:56-151`.
  Add `paused: bool` to `Run`; add `RunStore::set_paused(run_id, bool)`; populate on load.
- **Executor honor point:** NOTE — `SafetyGate::check_broker_submit`
  (`safety/gate.rs:124`) has **no live caller**; the gate is wired at
  `SafetyGatedExecutor` in **`crates/xvision-execution/src/`**. Find where global pause
  causes a skip/abort (`RunAbort::SafetyPaused`, `run.rs:18-49`) and add the per-run
  check alongside it. When a run is paused: **skip the broker submit for that cycle, do
  NOT terminate the run** (it keeps running, just doesn't trade).
- **Routes:** add `POST /api/eval/runs/:id/pause` + `/resume` in
  `crates/xvision-dashboard/src/routes/eval_runs.rs`, mirroring the global
  `POST /api/safety/pause|resume` in `crates/xvision-dashboard/src/routes/safety.rs`.
- **Verify:** `scripts/cargo build -p xvision-engine -p xvision-dashboard` + targeted tests green. Commit on this branch.

## How to resume after restart

```bash
# from main checkout
git fetch origin
git show origin/worktree-live-marketplace-impl:docs/superpowers/notes/2026-06-09-live-marketplace-impl-handoff.md   # this file
# re-enter the worktree (it should still exist) or recreate:
cd /Users/edkennedy/Code/xvision/.claude/worktrees/live-marketplace-impl 2>/dev/null || \
  git worktree add .claude/worktrees/live-marketplace-impl worktree-live-marketplace-impl
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-live-mp"
```

Then: enable `/usage-credits` (for subagent-driven), or proceed solo on A1 using the map above.
