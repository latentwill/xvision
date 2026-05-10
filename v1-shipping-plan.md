# XVISION v1 — Shipping Plan

> **Scope of this doc:** the v1 *test* slice — what we are actually putting in front of users to validate the eval and strategy engines on Alpaca paper. This is narrower than the legacy `v1-build-steps.md` (which still describes the full Phase 0–12 program). When the two conflict, this file wins for v1 test.
>
> **Companion docs:**
> - Sequenced phases (legacy, full program): `v1-build-steps.md`
> - Architecture overview: `architecture.md`
> - Operator runbook: `MANUAL.md`
> - Open follow-ups: `FOLLOWUPS.md`
> - UI design lock (source of truth for design team): `docs/design/ui-elements.md`
> - Visual system tokens: `docs/design/gptprompts.md`

---

## What v1 test is

A single vertical slice: **author a strategy → backtest it → paper-trade it on Alpaca**.

- Authoring: CLI (`xvn strategy …`, `xvn skill …`) and the Wizard (chat → MCP → engine).
- Eval: backtest mode (fixture parquet replay) and paper mode (Alpaca paper).
- Comparison: `xvn eval compare` + the `/eval/compare` route.
- Operator chrome: settings, chat rail, command palette, README/MANUAL.

No live daemon, no chain, no autoresearcher in this slice.

---

## Build order

Each row links to its plan. Plans are agentic-executable; this table is the ordering wrapper.

| # | Plan | Notes |
|---|---|---|
| 0 | **Working tree precondition** | Land or revert the in-flight working-tree changes (see "Preconditions" below) before any plan starts. Plan execution against a dirty tree is the fastest way to scramble migration numbers and module wiring. |
| 1 | [Strategy Engine MVP](docs/superpowers/plans/2026-05-08-strategy-creation-engine-mvp.md) | Foundation. Bundle / template / inline agent loop. Already merged. |
| 2 | [Terminology rename — Option B](docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md) | Mechanical; run before any plan that uses post-rename names. SQL migration already in tree (`xvision-core/migrations/0002_rename_setup_to_cycle.sql`); finish the Rust rename if not yet committed. |
| 3 | **[Engine API Foundation](docs/superpowers/plans/2026-05-10-engine-api-foundation.md)** | **NEW.** Lifts `xvision-engine/src/api/` skeleton from xvn-scheduling-and-agent-cli spec. Every subsequent CLI handler / MCP tool dispatches through `engine::api::<domain>::<fn>(ctx, req)` — no business logic in CLI or MCP wrappers. ~1 day. Owns migration `001_api_audit.sql`. |
| 4 | **Plan 2c §Task 7 — `BrokerSurface` trait (extracted)** | **NEW partial.** Pull only Task 7 of [Plan 2c](docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md#task-7-brokersurface-trait--dispatch) into v1 test scope: the `BrokerSurface` trait + Alpaca/Orderly impls in `xvision-execution`. Skip every other task in Plan 2c (scheduler, live daemon, deploy CLI). Required because Eval's paper mode wraps this trait. |
| 5 | [Eval Engine](docs/superpowers/plans/2026-05-08-eval-engine-plan.md) | Backtest + Alpaca paper mode. Paper executor wraps the `BrokerSurface` from #4. Owns migration `002_eval.sql`. CLI handlers + MCP verbs both dispatch through `engine::api::eval::*`. |
| 6 | [Strategy 2a — MCP + tool-call + 7 templates](docs/superpowers/plans/2026-05-08-strategy-engine-2a-mcp-tools-templates.md) | Tool-use loop, full template set. MCP server registers `engine::api::*` functions as tools (Foundation pattern). Eval auto-picks up tool-calls (shared `execute_slot`). |
| 7 | [LLM Providers & Per-Arm Models](docs/superpowers/plans/2026-05-10-llm-providers-and-per-arm-models-plan.md) | `[[providers]]` registry + per-arm `SlotRef`. Needed before settings UI ships. CLI dispatches through `engine::api::settings::*` (added by this plan + Settings). |
| 8 | [Strategy 2b — Skills](docs/superpowers/plans/2026-05-08-strategy-engine-2b-skills.md) | Local OSShip-style skills attached to slots. Marketplace deferred. CLI dispatches through `engine::api::skill::*`. |
| 9 | [Strategy 2d — Dashboard + Wizard](docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md) | The product face. **Live cockpit + marketplace routes deferred** — see "Out of scope" below and the plan's v1-test cut note. |
| 10 | [Settings & Onboarding](docs/superpowers/plans/2026-05-10-settings-and-onboarding-plan.md) | `/setup` + `/settings/{providers,brokers,daemon,identity,danger}`. No SQLite migration (config/default.toml only). |
| 11 | [Chat Rail Persistence](docs/superpowers/plans/2026-05-10-chat-rail-persistence-plan.md) | Promotes the wizard into a persistent rail across routes. Owns migration `003_chat_sessions.sql` (convert from inline rusqlite schema during integration). |
| 12 | [Command Palette](docs/superpowers/plans/2026-05-10-command-palette-plan.md) | ⌘K over strategies / runs / findings / scenarios. Owns migration `004_search_index.sql` (FTS5 virtual table; convert from inline schema during integration). |
| 13 | [Leverage items](docs/superpowers/plans/2026-05-10-leverage-items.md) | Ship A, B, C, D, F (docs) + E.1 (`xvn eod` CLI). E.2 and G defer — see the plan's "v1 test cut" block. |

### Themeing

Removed from v1 scope. The design handoff produces one final visual treatment for the routes listed below; no in-product theme switcher, no light/dark toggle, no `themes.md` pilot. Theme exploration files in `docs/design/themes-candidates.md` + `docs/design/themes/*.png` stay as design history.

---

## Out of scope for v1 test

| Out | Where it lives | Why deferred |
|---|---|---|
| Durable scheduler + live deploy daemon | [Plan 2c](docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md) | Eval paper mode covers Alpaca exposure for v1 test; no `xvn live deploy` needed. |
| ERC-8004 / Mantle / Orderly live | Phases 6.5, 11.5 in `v1-build-steps.md`; wallet plan | Chain surface deferred to Plan 5. |
| Marketplace publish / browse / install | [Plan 5 (blockchain)](docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md) + [smart-contract spec](docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md) | Eval writes attestations to local SQLite only; on-chain push waits. |
| Non-custodial trading-key store | [wallet plan + amendments](docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-amendments.md) | Bound to Plan 5; Alpaca paper doesn't need it. |
| Autoresearcher (mutator / judge / lineage seal) | [AR-1](docs/superpowers/plans/2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md) / [AR-2](docs/superpowers/plans/2026-05-09-autoresearcher-2-cycle-judge-evals.md) / [AR-3](docs/superpowers/plans/2026-05-09-autoresearcher-3-dashboard.md) | Separate program; no overlap with v1 test. |
| Lab Notebook (`/journal`) | [lab-notebook plan](docs/superpowers/plans/2026-05-10-lab-notebook-plan.md) | Marked DEFERRED at the top of its plan file. |
| Deferred archetypes (Pass-Ribbon, Lineage tree, Slot Machine, Spreadsheet, Power Notebook, Canvas) | [deferred-archetypes-roadmap](docs/superpowers/plans/2026-05-10-deferred-archetypes-roadmap.md) | All post-v1; trigger conditions documented per archetype. |
| `xvn eod` cron registration; runtime agent rename | Leverage items E.2, G | E.2 needs Plan 2c; G needs the wallet-plan `strategies` table. The CLI for E.1 still ships. |

---

## UI elements — design team handoff list

The single source of truth for design intent is **[`docs/design/ui-elements.md`](docs/design/ui-elements.md)** (v0.2). Visual tokens live in **[`docs/design/gptprompts.md`](docs/design/gptprompts.md)**. Theme exploration is in [`docs/design/themes-candidates.md`](docs/design/themes-candidates.md) with PNG candidates under `docs/design/themes/`.

Below is the cut of `ui-elements.md` that ships for the v1 test, and what design team needs to deliver per surface.

### Global chrome (every authenticated route)

| Element | `ui-elements.md` § | Status | Design needed |
|---|---|---|---|
| Page frame (1280×800 baseline, dark theme tokens) | §1.1 | Ship | Confirm token values + spacing scale; reconcile with the chosen theme candidate. |
| Top nav bar (logo, route tabs, env pill, user menu) | §1.2 | Ship | Final tab set: `Setup`, `Strategies`, `Eval`, `Settings` (no `Live`, no `Marketplace` in v1 test). |
| Toast / notification region | §1.3 | Ship | Three states: success / warning / error. Stacking + dismiss timing. |
| Chat rail (collapsed 40px / expanded 360px) | §1.4 | Ship | Collapsed icon, expanded layout, route-context chip styling, unread dot. Per-route open-state spec. |
| Command palette (⌘K modal) | §1.5 | Ship | Modal frame, result-row layout per artifact kind (strategy / run / finding / scenario), keyboard hint footer. |

### Routes shipping for v1 test

| Route | `ui-elements.md` § | Status | Design needed |
|---|---|---|---|
| `/` Control Tower | §2 | Ship — **trimmed** | Header, "Needs attention" panel, "Drafts in progress", "Recent findings", empty/cold-start. **Cut for v1 test:** "Live deployments rollup" (§2.2.2) and "Eval queue" (§2.2.3) — no scheduler/queue surface. **Stub:** "Lineage cue" (§2.3.3) shows count only. |
| `/setup` Wizard | §3 | Ship | Two-column chat / live-progress, first-run no-key state, error states, cross-cycle entry points. |
| `/authoring/<draft_id>` Inspector | §4 | Ship | Sticky bundle outline tree, mechanical-section editors, **LLM slot split editor (Move E)**, per-slot footer, skill picker modal, inspector rail. |
| `/strategies` Drafts list | §5 | Ship | Table columns, row actions, `Forked from` column (lineage tree itself is deferred). |
| `/eval/runs` Run list / leaderboard | §6 | Ship | Header strip, filter bar, run-table columns, status pills. |
| `/eval/runs/<id>` Run detail | §7 (verify section number) | Ship | Equity curve, trade markers, metrics tiles, findings list. |
| `/eval/compare?ids=…` Comparison view | Plan 2d + §7 | Ship | Side-by-side equity curves + trade markers + findings. v1 cut: no drawdown overlay, no regime-shaded background, no NL Q&A. |
| `/settings` shell + 5 sub-pages | (Settings & Onboarding plan) | Ship | Sidebar nav; Providers / Brokers / Daemon (heartbeat only) / Identity (read-only stub) / Danger zone. |

### Routes NOT shipping for v1 test (no design work needed yet)

| Route | Why deferred |
|---|---|
| `/live/<deployment_id>` Flight Deck cockpit | Needs Plan 2c (scheduler + live daemon). |
| `/marketplace` listings grid | Plan 5 (blockchain). |
| `/journal` Lab Notebook | Deferred plan. |
| `/lab` Power Notebook, `/canvas` spatial graph, `/sweeps/<id>` Spreadsheet, `/setup?starter=1` Slot Machine | All in `deferred-archetypes-roadmap.md`. |

### Cross-route patterns the design team should also produce

| Pattern | Where it appears | Design needed |
|---|---|---|
| Status pill set (queued / running / completed / failed / cancelled) | Eval routes, Control Tower | Color + icon per state, dark-theme variants. |
| Empty-state illustrations / copy | Every route's cold start | One per route; copy is forward-pointing ("Start a draft", "Run your first eval") not apologetic. |
| Skill chip | Inspector slot footer, skill picker | Compact chip showing skill name + version. |
| Provider chip | Inspector slot footer, settings | `provider/model` label + status dot. |
| Findings card | Run detail, Control Tower | Title + 1-line summary + tag set + `Add to journal` (button stub in v1 test — journal route is deferred). |

### Theme

Removed from v1. The design handoff delivers a single final visual treatment for every route + chrome element listed above. No theme switcher, no light/dark toggle, no theme pilot in v1. The candidate PNGs in `docs/design/themes/` are reference material for the design team's selection and don't ship as user-facing options.

---

## Migration reservations

Single owner: `xvision-engine/migrations/`. Every plan that touches `xvn.db` claims its number from this table **before** writing any SQL. The wallet plan's `xvision-data/src/migrations/` (timestamp-prefixed `20260510000001…`) targets a different DB file and stays out of this registry.

| Number | Owner plan | Tables | v1 test? |
|---|---|---|---|
| `001_api_audit.sql` | Engine API Foundation (#3) | `api_audit` | ✅ |
| `002_eval.sql` | Eval Engine (#5) | `eval_runs`, `eval_events`, `eval_attestations`, `scenarios` | ✅ |
| `003_chat_sessions.sql` | Chat Rail Persistence (#11) | `chat_sessions`, `chat_messages` | ✅ |
| `004_search_index.sql` | Command Palette (#12) | `search_artifacts` (FTS5 virtual table) | ✅ |
| `005_journal.sql` | Lab Notebook (deferred) | `journal_entries` | ⛔ deferred |
| `006_scheduler.sql` | xvn-scheduling-and-agent-cli (deferred) | `schedules`, `schedule_fires` | ⛔ deferred |
| `007_autoresearch.sql` | AR-1 (deferred) | autoresearch core tables | ⛔ deferred |
| `008_autoresearch_evals.sql` | AR-2 (deferred) | `canary_runs`, `mutator_ladder_snapshots`, `diversity_samples` | ⛔ deferred |

Notes:
- xvn-scheduling-and-agent-cli's `api_audit` was originally numbered 002; ownership of `api_audit` has moved to the new Engine API Foundation plan at 001. The scheduler tables stay reserved at 006.
- AR-1's original `003_autoresearch.sql` and AR-2's `004_autoresearch_evals.sql` need renumbering when those plans are picked up — call this table out in the plan files at pickup time.
- Chat Rail and Command Palette plans currently reference inline rusqlite schemas; convert to numbered `.sql` migrations during integration so the registry stays authoritative.
- Settings & Onboarding has **no** SQLite migration (rewrites `config/default.toml` via `toml_edit` only).
- Plan 2c §Task 7 (BrokerSurface, item #4) has no migration — it's a trait + impls in `xvision-execution`.

## Decisions resolved

| Decision | Resolution |
|---|---|
| **Eval paper mode dependency** | Pull Plan 2c §Task 7 (`BrokerSurface` trait + Alpaca/Orderly impls) into v1 test scope as item #4. Skip the rest of Plan 2c (scheduler, live daemon, deploy CLI). Eval's paper executor calls `BrokerSurface` directly. |
| **Typed engine API** | Adopted as item #3 (Engine API Foundation). Every CLI handler / MCP tool dispatches through `engine::api::<domain>::<fn>(ctx, req)`. Pattern documented in `crates/xvision-engine/src/api/README.md` (created by the foundation plan). |
| **Migration numbering** | See "Migration reservations" above. Single owner: `xvision-engine/migrations/`. |
| **Themeing** | Removed from v1. Design handoff delivers one final treatment. |
| **Findings extractor LLM provider** | Use the `default = true` provider from the `[[providers]]` registry (LLM-Providers plan, item #7). CLI accepts an optional `--findings-provider <name>` override. Eval plan's findings extractor reads provider via `engine::api::settings::get_default_provider(ctx)`. |
| **Alpaca asset coverage** | BTC-only for v1 test. The 4 canonical eval scenarios MUST be BTC-only; verify before plan #5 starts. Multi-asset Alpaca (loading symbol map from `config/whitelist.toml`) is a v1.1 follow-up tracked in `FOLLOWUPS.md`. |
| **Plan 2d live cockpit** | Deferred. Plan 2d ships Wizard + Inspector + Strategies list + Eval routes only. Live cockpit (`/live/<id>`) and marketplace routes wait for Plan 2c (deferred) and Plan 5 (blockchain) respectively. The plan file carries a v1-test cut note (mirroring leverage-items). |

## Preconditions

Before plan execution starts:

1. **Working tree** — uncommitted changes touch CLI command files (`asset.rs`, `indicator.rs`, `intern.rs`, `metrics.rs`, `risk.rs`, `show_briefing.rs`, `store_cmd.rs`, `trader.rs`, `venue.rs`) and `xvision-core/{store.rs,config.rs}`. Looks like the terminology rename + early CLI surfacing is mid-flight. **Land or revert these before starting plan #3.** Plan execution against a moving codebase will create migration-number races and module-wiring conflicts.
2. **Plan #1 baseline green** — `cargo test -p xvision-engine` must pass before #3 starts. Terminology rename Phase 0 already requires this.
3. **Eval scenarios are BTC-only** — verify before plan #5 starts. If any scenario references non-BTC assets, either subset to BTC or extend Alpaca's symbol map (currently hardcoded BTC/USD at `crates/xvision-execution/src/alpaca.rs`) as a prereq.

---

## Telemetry, success criteria, exit checks

- **Authoring:** new user lands on `/setup`, completes a Wizard turn, persists a draft, and sees it in `/strategies` — no errors, no orphan rows.
- **Backtest:** `xvn eval run <strategy> --scenario <id>` against fixture parquet completes, persists metrics + findings, surfaces in `/eval/runs/<id>`.
- **Alpaca paper:** the same strategy runs in paper mode against Alpaca, fills are persisted to the event store, equity curve renders.
- **Compare:** `/eval/compare?ids=<a>,<b>` renders both runs side-by-side with findings.
- **Reset:** `xvn eod` produces a sensible markdown report from the test-session data.

If all five pass for one strategy template (e.g. `mean_reversion`) end-to-end, the v1 test slice is shippable. Repeat for at least one more template (e.g. `trend_follower`) before declaring v1 test done.
