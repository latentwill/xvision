# Dashboard redesign — continued-execution plan (ce-plan)

**Branch:** `feat/dashboard-redesign` · **Date:** 2026-06-10 ·
**Tracks:** beads `xvision-9pi` (live mistag), `xvision-eb5` (redesign + eval coverage)
**Design brief:** `docs/design/README.md` · **Before/after:** `docs/design-audit/assets/desktop-home-after-fix.png` → `desktop-home-after-redesign.png` / `desktop-home-after-redesign-fold.png`

## Shipped in this branch

| # | Item | Where |
|---|------|-------|
| 1 | Live-money discriminator on `/api/agent-runs` (`eval_mode`, `eval_run_status`, `paused`, `is_live_money`); live ⇔ parent eval run `mode=live` ∧ non-terminal ∧ agent run non-terminal | `crates/xvision-dashboard/src/routes/agent_runs.rs` |
| 2 | Boot-time sweep interrupting orphaned `running` agent_runs (the fake "20 live") | `agent_runs.rs` + `server.rs` |
| 3 | Honest liveness taxonomy in UI: live-money / paused / paper / **stale** (never "live") | `features/live/strip-status.ts`, `LiveSummaryStrip`, `StrategyPill` |
| 4 | Eval coverage server-side: `StrategySummary.{bundle_hash, origin, evaluated, last_eval_completed_at}`; evaluated matches completed runs by ULID **or** bundle hash (CLI runs now count) | `crates/xvision-engine/src/api/strategy.rs` |
| 5 | Optimizer-origin segmentation via `lineage_nodes` membership; nag → neutral segmented line ("N user awaiting first eval · M optimizer-generated") | same + `features/strategies/coverage.ts` |
| 6 | Home redesign: pulse band (equity + drawdown band + KPI sparklines + honest execution chip), attention band, Optimizer panel (acceptance meter, writer ladder, cycle trend, Σ spend, honest idle), strategy leaderboard (low-n + origin chips) | `routes/home.tsx`, `components/home/*`, `features/home/*` |
| 7 | F8 fix: non-finite guards on all uPlot gradient/fill plugins | `components/chart/v2/adapters/uplot-plugins.ts` |
| 8 | Verification: cargo (dashboard+engine) green, vitest 1841/1841, tsc clean, vite build green | — |

## Next: deploy (required before users see correct counts)

The live node (`xvn.tail2bb69.ts.net`) runs the **old** backend. Until deployed:
frontend degrades gracefully (absent `is_live_money`/`evaluated` ⇒ conservative:
nothing claims live-money; coverage falls back to client join). To ship:

```bash
scripts/deploy-image.sh --push root@<host>   # local build path, then recreate the service
```

Verify rollout digest per CLAUDE.md, then confirm on the node: live strip shows
0 live-money / stale demoted by the boot sweep; coverage line shows the
segmented counts (expected at audit time: 40 user awaiting · 2 optimizer · 54 evaluated).

## Follow-ups (priority order)

### P1 — correctness / trust
1. **Periodic orphan sweep** (not just boot): debounced job interrupting child
   agent_runs whose parent eval run has been terminal > grace window. Boot
   sweep covers restarts only. (Per-finalization hook was judged invasive:
   eval finalization flows through the batched `FinalizeWriter` mpsc pipeline
   in xvision-engine; crossing the eval↔observability boundary there needs its
   own design.) — notes in `docs/design/notes-live-status.md`.
2. **`win_rate` + `n_trades` on `RunSummary`** (backend): the pulse band omits
   win rate rather than fake it, and leaderboard sample-size chips currently
   key off run counts. Both research-mandated metrics need backend fields.
3. **Agent-run detail endpoint enrichment**: `GET /api/agent-runs/:id` serves
   the versioned `xvn.agent_run.v2` export (shared with `xvn run inspect`);
   remaining live-money fields should land in a future schema bump.

### P2 — product surface
4. **Mobile home (audit F7)**: at 390px the phone shell still lands on the
   chat surface; the new dashboard sections are unreachable on `/` at phone
   width (they stack fine when rendered). Needs a mobile-shell routing pass +
   chat via dock. Screenshot: `assets/mobile-home-chat-shell-f7.png`.
5. **Portfolio-level aggregation endpoint**: equity/PnL across strategies is
   client-side over per-run payloads today. A summary endpoint (total equity
   timeseries, aggregate PnL, open positions) unlocks a true portfolio hero.
6. **Strategy detail page** alignment with the research: equity + drawdown
   overlay, trade distribution, regime breakdown, decision/veto logs,
   structured "explain this result" panel. Leaderboard already links there.
7. **Comparison page** (same-period side-by-side, "choose this if…") — the
   `/api/eval/compare` endpoint already exists.

### P3 — hygiene
8. Delete superseded components once nothing references them:
   `HomeOutcomeStrip`, `StrategyOutcomesSummary`, `StrategyOutcomesList`
   (left in place, tests green).
9. Consolidate duplicate sparklines (`components/home/Sparkline` vs
   `features/marketplace/components/Sparkline`).
10. `last_eval_completed_at` uses lexicographic max of RFC3339 strings —
    normalize precision if formats ever diverge.
11. Optimizer-origin currently counts lineage membership of **any** status; if
    product wants only *active* lineage to suppress the awaiting-eval nag,
    it's a one-line WHERE change in `apply_eval_coverage()`.
12. Strategies list endpoint adds 2 batched SQLite queries per page (chunked
    at 400 binds) — fine today; move to a JOIN if profiles ever flag it.
13. Transient `api.request.error` console log observed against the live node
    during dev-proxy review (pre-existing fetch path) — re-check during the
    next design-audit run.

## Re-audit checklist (after deploy)

- [ ] `/` first screen answers: making money? machine working? what's next? —
      no em-dash rail, no fake live count, no nag tone.
- [ ] Live cockpit shows strategy names + honest mode badges (F6 partially
      remains: chip naming on /live is unchanged this branch).
- [ ] No `createLinearGradient: non-finite` console noise (F8).
- [ ] Optimizer page idle states (F8 copy) — home panel fixed; the /optimizer
      page itself still shows "Waiting for connection…" and is a follow-up.
