# Reachability Pass — Isolated / Hidden Features Audit

**Date:** 2026-06-08
**Author:** reachability sweep (Claude)
**Scope:** Find capabilities that are *built and green-tested* but *not reachable*
by an operator through the `xvn` CLI or the dashboard UI — "built but hidden" code.

---

## Summary

The codebase is in good reachability shape. The two user-facing entry-point
surfaces are essentially complete:

- **CLI** — 38 top-level `xvn` verbs, all wired into `lib.rs` dispatch. **Zero**
  dead/stub/unregistered commands, no `todo!()`/`unimplemented!()` handlers.
- **Dashboard** — every route module in `routes/mod.rs` is mounted; every lazy
  page in `routes.tsx` is registered. No unmounted backend routes, no unregistered
  frontend pages.

The genuinely-hidden surface is small once false positives are removed:

| Axis | Raw candidates | Verified hidden | False-positive rate |
|---|---|---|---|
| 1 — tested lib features orphaned from CLI+UI | 5 | **2** | 60% |
| 2 — backend endpoints with no UI caller | ~30 | **8** | ~73% |
| 3 — CLI ↔ UI surface asymmetry | — | (inventory, not dead code) | — |

**Headline:** Only two tested library features are truly orphaned
(`chat_session::rich_blocks` inline-chart builders, `eval::provider_registry`),
and only eight backend endpoints have no operator path at all. Everything else
the first-pass sweep flagged turned out to be reachable transitively or via a
frontend caller the path-string grep initially missed.

---

## Methodology & caveats

- **Structural only — no cargo.** Per the repo's "no cargo in main checkout
  during multi-agent" rule, "green" here means: tests exist, are real assertions
  (not `assert!(true)` smoke), and live in a workspace that builds. Tests were
  **not executed** for this audit.
- **Reachability = static reference.** A feature is "reachable" if its public
  item (or the JSON/route it backs) is referenced from `crates/xvision-cli/src`,
  `crates/xvision-dashboard/src`, or `frontend/web/src` — directly or through a
  short, verified call chain.
- **Grep false-positive risk.** Path-param endpoints are built with template
  strings (`/api/eval/runs/${id}/cancel`), so naive prefix grep under-reports
  callers. Every Axis-2 "dark" claim below was verified against the static
  prefix **and** suffix. Every Axis-1 "orphaned" claim was re-grepped across
  both consumer crates to confirm zero hits, then the internal call chain was
  traced to separate *truly dead* from *internal-infra reachable*.
- **MCP is a surface.** `xvision-mcp` tools are reachable via the MCP server,
  which is a legitimate operator interface, so they are out of scope for
  "not reachable via cli/ui".

---

## Axis 1 — Tested library features orphaned from CLI + UI

### Verified ORPHANED (tested, zero callers on any operator path)

| Crate · module | Feature | Tests | Evidence | Verdict |
|---|---|---|---|---|
| `xvision-engine::chat_session::rich_blocks` | 6 inline chart/card builders: `build_inline_chart`, `inline_equity_chart_from_run_detail`, `inline_compare_chart_from_report`, `inline_returns_histogram_from_runs`, `run_list_card_from_summaries`, `inline_strategy_card_from_summary` | 10 inline tests (`rich_blocks.rs:667-819`) | `pub use`-re-exported in `chat_session/mod.rs:20-30` but **zero call sites** anywhere (cli, dashboard, other engine modules, frontend). Sibling `action_confirmation_card` from the same module **is** used by the chat rail — these six are not. | **Wire-up candidate.** Real "built but hidden" feature: inline equity charts / strategy cards / run-list cards / return histograms for the chat rail, fully built and tested but never emitted. Either finish the chat-session emit path or remove. |
| `xvision-eval::provider_registry` | `ProviderRegistry` | 6 inline tests | Only reference in the whole tree is a **doc comment** (`xvision-core/src/providers/model_metadata.rs:13`). No code constructs or calls it. | **Remove or wire.** Vestigial / future-infra. Canonical provider metadata lives in `xvision-core::providers::model_metadata`; this looks superseded. |

### Verified REACHABLE (first-pass flagged them, but they are surfaced) — recorded so they aren't re-flagged next sweep

| Crate · module | Why it looked orphaned | Why it's actually reachable |
|---|---|---|
| `xvision-eval::bootstrap` (`paired_bootstrap_sharpe_delta`, `BootstrapResult`) | No direct ref from cli/ or dashboard/ | Live internal use: `eval/report.rs:16,89` and `eval/metrics.rs` compute Δ-Sharpe CIs; `report.rs` is rendered by the `xvn report` verb and the eval-run-detail UI. |
| `xvision-engine::mint::holdout` (`detect_overfit`, `metric_coverage_gap`, `capability_required_metrics`) | Symbols absent from a direct cli/dashboard grep | Imported by `dashboard/routes/optimizations.rs:39` (the overfit gate) and re-exported at `engine/lib.rs:52`. Surfaced by the optimizations detail UI (holdout / waive-overfit / mint buttons). |
| `xvision-engine::strategies::mechanistic` (`MechanisticConfig`, `EntryRule`, `ClosePolicy`, `DecisionMode`) | `MechanisticConfig` not named in cli/dashboard Rust | Authored end-to-end via `frontend/web/src/routes/authoring.tsx` (agentic ↔ mechanistic toggle, entry-rule / close-policy builder) → `PUT /api/strategy/:id/mechanical_params`; executed by `eval/executor/backtest.rs:1534` (`mechanistic_action`). Reachable via JSON, not a Rust symbol ref. |
| `xvision-filters` (DSL: `ConditionTree`, `Operator`, `Operand`, `IndicatorName`, …) | 47 tests, no obvious cli verb | Built via the strategy **filter** authoring UI → `PUT /api/strategy/:id/filter` (`strategies.rs:490` / `strategies.ts:339`). Reachable. |

> Note the 60% false-positive rate on the raw Axis-1 list — symbol-name grep alone
> systematically over-reports orphans for anything crossing the Rust↔JSON boundary
> (mechanistic, filters) or surfaced only through a rendered report (bootstrap).

---

## Axis 2 — Backend endpoints with no UI caller

Verified against `frontend/web/src` (static prefix + suffix). **8 of ~30** are
genuinely dark. The rest are reached (e.g. eval `cancel`/`retry`/`delete`,
scenario `clone`/`archive`, danger `factory-reset`/`reset-workspace`, all
provider catalog/model endpoints — all have confirmed frontend callers).

| Method · path | Handler | Alt operator path | Verdict |
|---|---|---|---|
| `GET /api/agent-runs/:id/memory-recalls` | `agent_runs::list_memory_recalls` | none (sibling `memory-events` **is** used) | Dark — likely superseded by `memory-events`. Remove or surface. |
| `POST /api/settings/danger/regen-identity` | `settings::danger::regen_identity` | no CLI verb | Dark — siblings `factory-reset` & `reset-workspace` have buttons; this one has none. Gap. |
| `GET /api/chat-rail/tool-policy` | `chat_rail::get_tool_policy` | no CLI verb | Dark — Phase 2.3 tool-policy surface never wired to UI. |
| `PUT /api/chat-rail/tool-policy` | `chat_rail::put_tool_policy` | no CLI verb | Dark — as above; operators can't edit tool policy. |
| `GET /api/autooptimizer/findings/:bundle_hash` | `autooptimizer_route::get_findings` | no CLI verb | Dark — findings likely shown via experiment-detail instead; endpoint unused. |
| `DELETE /api/autooptimizer/schedule/:id` | `autooptimizer_cycle::delete_schedule` | no CLI verb | Dark — schedules can be created (POST) but not deleted from UI. Gap. |
| `POST /api/autooptimizer/cycles/:cycle_id/pause` | `autooptimizer_cycle::pause_cycle` | no CLI verb | Dark — frontend `pauseSession` hook **exists but is never mounted on a control**. Half-wired. |
| `POST /api/autooptimizer/cycles/:cycle_id/resume` | `autooptimizer_cycle::resume_cycle` | no CLI verb | Dark — frontend `resumeSession` hook exists but unused. Half-wired (pairs with pause). |

**Pattern:** the autooptimizer cluster (pause / resume / delete-schedule / findings)
is the densest pocket of dark endpoints — backend + partial frontend API plumbing
landed, but the controls were never placed in `LiveCycleView`. The chat-rail
tool-policy pair is a fully-built Phase 2.3 surface with no UI at all.

---

## Axis 3 — CLI ↔ UI surface asymmetry (inventory, not dead code)

These are reachable on one surface but absent on the other. Not bugs — recorded
so the asymmetry is a deliberate choice, not drift.

**CLI-only (no dashboard route):**
`indicator` (sma/ema/rsi/bollinger/atr/macd/donchian/fib/stddev/zscore/cmo),
`bars` (fetch/ls/rm/gc), `eod`, `doctor`, `trajectory` (inspect/validate/purge/reindex),
`store` (migrate/stats), `migrate`, `model bakeoff`, `experiment` (new/ls/show/update/run
— note `/optimizer/experiment/:hash` is the *autooptimizer* experiment, unrelated),
`run inspect`, `intern`/`trader` preview, `venue` (portfolio/close-position),
`fire-trade`, `last`, `show-briefing`/`show-decision`/`show-metrics`.

- *Live execution* (`fire-trade`, `venue`) being CLI-only is **intentional** —
  consistent with backtest-only product framing and live trades gated behind
  `XVN_DASHBOARD_CLI_DEVMODE` (see memory notes). Leave as-is.
- *Dev/maintenance* verbs (`store`, `migrate`, `trajectory`, `doctor`, `bars`,
  `indicator`) are reasonably CLI-only. No action.
- `experiment` (the eval A/B experiment manager) is the one **product** feature
  with a full CLI CRUD surface and **no UI** — worth a deliberate decision.

**UI-only (no CLI verb):**
`wizard` (`POST /api/wizard/chat`), `search` (`/api/search`), `safety`
(pause/resume/state/audit), `focus` (`/api/chat-rail/focus`), the whole
`chat-rail`, `chart-lab` (dev playground), `charts` dashboards, and the richer
`marketplace` discovery pages (`leaderboard`/`creator`/`lineage`/`receipts` —
CLI marketplace only has list/publish/buy/attest).

- `safety` pause/resume having **no CLI verb** is the most notable gap: an
  operator on the CLI cannot halt trading without the dashboard. Consider an
  `xvn safety pause|resume|state` verb for parity.

---

## Recommendations

**Wire-up (real hidden features):**
1. `chat_session::rich_blocks` — finish the chat-session emit path so the 6 inline
   chart/card builders actually render in the chat rail, or delete them. (Owning
   wave: chat-rail/DSPy — see memory `project_chat_rail_dspy_wave`.)
2. Autooptimizer **pause/resume controls** — the backend + frontend hooks exist;
   place the buttons in `LiveCycleView`. (Owning surface: autooptimizer P4.)
3. Autooptimizer **schedule delete** + chat-rail **tool-policy editor** — surface
   the existing endpoints, or document them as intentionally API-only.

**Remove / decide (vestigial):**
4. `eval::provider_registry` — appears superseded by `xvision-core` provider
   metadata; remove or justify as future-infra.
5. `agent-runs/:id/memory-recalls` + `danger/regen-identity` + `autooptimizer
   findings/:bundle_hash` — dark endpoints with no caller; remove or wire.

**Parity (asymmetry, optional):**
6. `xvn safety` verb for CLI trade-halt parity.
7. Decide whether the `experiment` A/B manager deserves a UI (currently CLI-only).

**Leave as-is (intentional):** live-execution CLI verbs, dev/maintenance verbs,
MCP tools, chart-lab playground.

---

## Appendix — per-axis source of truth

- CLI verbs: `crates/xvision-cli/src/lib.rs` (enum + `Cli::run` dispatch),
  `crates/xvision-cli/src/commands/mod.rs`.
- Backend routes: `crates/xvision-dashboard/src/server.rs`
  (`readonly_router` / `mutating_router` / `auth_router`).
- Frontend routes: `frontend/web/src/routes.tsx`; API callers in
  `frontend/web/src/api/*.ts` and feature/route components.
- Orphan call-chain traces: greps cited inline above (all file:line).
