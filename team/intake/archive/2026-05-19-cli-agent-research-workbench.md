# Intake — 2026-05-19 — CLI as agent-native strategy research workbench

Agent-driven feedback (Hermes-Majestic, 2026-05-19 ~21:46). After
running a full strategy-iteration loop end-to-end against the live
`xvn` HTTP/MCP surface, the agent reports it had to bolt Python around
the CLI to glue the workflow together: POST strategy / agent / eval
requests, poll until runs finish, normalize metrics, count decisions
and action types, compare scenarios, preserve IDs, avoid stale UI
assumptions.

Verbatim at the bottom. The single sentence framing:

> "Turn xvision from 'an eval API I can script against' into 'a
> strategy research workbench an agent can operate fluently.'"

This intake is **not** a UX/QA round — it's a CLI + diagnostics + data-model
proposal. Several tracks introduce new persisted concepts (experiment
ledger, hypothesis manifest, regime labels) and would need spec/plan
treatment under `docs/superpowers/specs/` before contracts open.
Conductor should select a first wave (likely the CLI ergonomics
tracks) rather than decompose all 13 at once.

## Already-built building blocks (sanity check before decomposition)

Before opening contracts, confirm what already exists. Initial pass
based on repo state on 2026-05-19:

**Review feature is landed** — `xvn eval review <run_id> --agent <profile>`
exists at `crates/xvision-cli/src/commands/eval/review.rs`, the engine
pipeline is at `crates/xvision-engine/src/eval/review/`, the dashboard
route is at `crates/xvision-dashboard/src/routes/eval/review.rs`, and
the SPA has `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx`
with an `AgentPicker` letting the operator choose which agent profile
(and thus which provider/model) runs the review. The CLI's `--agent`
flag is the review-model picker the agent needs. No new picker track
required — but see #14 below for batch-auto-review wiring.

The rest of the building-block audit:

- `xvn strategy create` exists in `crates/xvision-cli/src/commands/strategy.rs`
  with `--template`, but does **not** bundle agent attachment + provider/model
  binding atomically. Today the operator does strategy create → agent
  create → wire agent into strategy in separate calls. The atomic verb
  the agent wants is genuinely missing.
- `crates/xvision-engine/src/strategies/validate.rs` exists but is
  shape-only (agent count, slot resolution). No preflight against
  provider enable state, scenario asset/timeframe, warmup, expected
  decisions. The `xvn strategy validate` verb is missing.
- `crates/xvision-cli/src/commands/ab_compare.rs` exists for paired
  cycle-id evals (`--cycles`). It does not cover "same strategy across
  N scenarios with normalized decision budget." That's a different
  shape — `xvn eval batch run` is genuinely missing.
- `crates/xvision-engine/src/api/eval.rs` already persists decisions,
  fills, equity curve. Action-distribution and behavior summary fields
  are **not** computed today — they'd be a new derivation pass.
- Scenarios store asset/timeframe/window/warmup but **no regime
  labels**. Regime tagging is new metadata + a likely small migration.
- Strategy `notes`/`description` are freeform; **no structured
  hypothesis fields**. Hypothesis manifest is new metadata.
- No experiment-ledger concept anywhere — it's pure greenfield.

Conductor and worker should re-verify each of these before opening a
track; the intent is to flag what's net-new vs. an extension.

## Findings → tracks (decomposition for conductor refinement)

### CLI ergonomics (highest leverage; smallest blast radius)

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 1 | P1 | `cli-strategy-create-atomic` | `xvn strategy create` becomes one verb that atomically creates strategy + agent + provider/model binding + role assignment; emits `eval_ready: bool` and `warnings: []` in JSON output |
| 2 | P1 | `cli-strategy-validate` | `xvn strategy validate <strategy-id> --scenario <id> --json` preflight: agents attached, providers enabled, scenario asset/timeframe match prompt assumptions, warmup adequate, expected decision count, output-schema compatibility |
| 3 | P1 | `cli-eval-batch-run-wait` | `xvn eval batch run --strategy <id> --scenarios a,b,c,d --wait --json` blocks until all terminal states; returns single batch object with run-level status + return/Sharpe/DD/decisions/action distribution |
| 4 | P1 | `cli-eval-compare-report` | `xvn eval compare --batch <id> --markdown|--json` first-class comparison: per-run return, Sharpe, max DD, decisions, action distribution, avg hold, flips, worst/best trade, late-entry / re-entry / invalidation-failure counts where computable |

These four cover the agent's "biggest improvements" list and unblock
most of its Python scaffolding. Recommend they ship as one wave.

### Scenario authoring

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 5 | P2 | `cli-scenario-set-balance` | `xvn scenario set create` + `xvn scenarios select` with `--same-decisions` and `--max-decisions` constraints; selects existing scenarios that match, or clones/mutates to normalize decision count |
| 6 | P2 | `cli-scenario-inspect-card` | `xvn scenario inspect <id> --card` summary card: id/name/asset/timeframe/window/warmup/decision-count/regime+volatility labels/previous-run history. Plain-text card, not JSON dump |

### Strategy hypothesis + experiment ledger (data-model adds; needs spec)

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 7 | P2 | `strategy-manifest-with-hypothesis` | Add structured hypothesis fields to `Strategy`: family, hypothesis, target/avoid regimes, asset/timeframe assumptions, entry/exit/risk logic. Compatible with existing freeform prompt — does not replace it |
| 8 | P2 | `experiment-ledger-foundation` | New `experiments` table: id, question, strategy_id(s), scenario_ids, decision_budget, batch_id, result summary, conclusion, next_recommendation. Migration + API surface, no CLI yet |
| 9 | P2 | `cli-experiment-run-oneshot` | `xvn experiment run --name <slug> --strategy <id> --scenario-set <id> --same-decisions --wait --compare --markdown` end-to-end verb that materializes an experiment record from a batch run. Depends on #7 + #8 |

### Diagnostics

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 10 | P2 | `eval-decision-behavior-summary` | Compute and surface per-run behavior summary: flat_rate, trades_opened, direct_flips, avg_bars_held, reentries_after_loss, exits_on_invalidation, held_through_invalidation, primary failure mode. New derivation pass over existing decisions + fills |
| 11 | P2 | `eval-baseline-auto-comparison` | Every eval auto-runs buy-and-hold + always-flat + simple-trend + simple-mean-reversion baselines (use `xvision-eval` arms). Report adds `relative_to_buy_hold`, `relative_to_flat`, etc. |
| 12 | P3 | `scenario-regime-labels` | Structured regime labels on scenarios (trend, volatility, liquidity, chop_score, event_type, directional_persistence). Tag during scenario authoring + back-fill on existing scenarios. Feeds into #5 (`--regimes` filter) and #10 (regime-conditioned analysis) |

### Cross-cutting

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 13 | P2 | `mcp-surface-parity-for-new-verbs` | Every new CLI verb from #1-#9 gets an MCP-tool peer so the chat-rail agent can call them. Wired via existing `xvision-mcp` crate |
| 14 | P3 | `cli-eval-batch-auto-review` | Extend `xvn eval batch run` with `--review-with <agent-profile>` so the batch verb chains a `xvn eval review` call per finished run, attaching the review summary to the batch JSON. Reuses the already-landed review pipeline; thin wiring only |

14 tracks total. Conductor's first wave likely #1-#4 (CLI ergonomics).
#7-#9 want a spec under `docs/superpowers/specs/` before contracts open.

## Suggested first-wave bundling

If a 4-track wave is too wide for one conductor cycle, the agent's own
priority order suggests this batching:

- **Wave A (CLI ergonomics, ~4 tracks):** #1 `cli-strategy-create-atomic`,
  #2 `cli-strategy-validate`, #3 `cli-eval-batch-run-wait`,
  #4 `cli-eval-compare-report`. Self-contained; no schema migrations;
  unblocks the agent's Python-scripting pain in one wave.
- **Wave B (scenario authoring, 2 tracks):** #5 `cli-scenario-set-balance`,
  #6 `cli-scenario-inspect-card`. Follows on once batch runs exist.
- **Wave C (data model + experiments, 3 tracks):** #7, #8, #9. Needs
  a spec first (see Open questions below). One migration; one new table.
- **Wave D (diagnostics, 3 tracks):** #10, #11, #12. Can land
  incrementally; #10 and #11 don't depend on #12.
- **Wave E (MCP parity, 1 track):** #13. Roll-up at the end of A-D so
  the chat-rail agent gets every new verb in one pass.

## Open questions (for spec stage, not for the board)

1. Hypothesis manifest — column on `strategies` (JSONB), or a separate
   `strategy_hypothesis` table? Per-version, or current-only? Affects
   how #7 ships and what #8 records as a snapshot.
2. Experiment ledger — is it a first-class entity surfaced in the SPA,
   or CLI/JSON-only at v1? The agent's framing suggests CLI-first is
   fine; SPA surface is a follow-on.
3. Baselines — re-use `xvision-eval` arms verbatim, or are
   "buy_hold / always_flat / simple_trend / simple_mean_reversion" new
   `Algorithm` impls that the eval pipeline runs alongside the strategy?
4. Regime labels — derive automatically from scenario bars (volatility
   bucket, trend coefficient), or operator-authored, or both?
5. Action distribution + behavior summary — compute at eval-run
   finalization and persist, or compute on-demand from decisions + fills?
   Finalization-time persists ages of decisions cheaply; on-demand is
   simpler but adds latency to every inspector load.

## Status reconciliation — 2026-05-21

**All 14 tracks in this intake have shipped.** Verified via git log
+ source presence on `origin/main` 2026-05-21.

| Track | Wave | Shipped via | File evidence |
|---|---|---|---|
| #1 `cli-strategy-create-atomic` | A | `2b77c11` | `crates/xvision-cli/src/commands/strategy.rs` |
| #2 `cli-strategy-validate` | A | `6448934` | `strategy.rs` (`xvn strategy validate` verb) |
| #3 `cli-eval-batch-run-wait` | A | `6c6d123` | `crates/xvision-cli/src/commands/eval/batch.rs` |
| #4 `cli-eval-compare-report` | A | `9c91842` | `crates/xvision-cli/src/commands/eval/compare_format.rs` |
| #5 `cli-scenario-set-balance` | B | `a786d0d`, `557d89c` | `xvn scenarios select`, `--same-decisions` in `scenario.rs:269-275` |
| #6 `cli-scenario-inspect-card` | B | `d129469` | `format_inspect_card` at `scenario.rs:737` |
| #7 `strategy-manifest-with-hypothesis` | C | `c09b120`, `a640542` | `Hypothesis` struct on `Strategy` |
| #8 `experiment-ledger-foundation` | C | `458073d`, `a640542` | migration 022 `experiments` table |
| #9 `cli-experiment-run-oneshot` | C | `ccfe376`, `78c0d7b`, `893337c` | `xvn experiment run` orchestrator |
| #10 `eval-decision-behavior-summary` | C | `8ccd8bc`, `296efd1` | per-asset reentry tracking; on-demand from decisions |
| #11 `eval-baseline-auto-comparison` | C | `4cc798c`, `dd4c054`, `64938b3` | baseline arms wired into `BacktestExecutor`; compare-markdown column |
| #12 `scenario-regime-labels` | C | `26246a4`, `29fc9ec`, `1f3b5d4` | regime column + `xvn scenario classify`/`set-regime` |
| #13 `mcp-surface-parity-for-new-verbs` | D/E | `fe55dde`, `d9e9bd8` | 6 MCP tool peers for the new CLI verbs |
| #14 `cli-eval-batch-auto-review` | D | `0c74e3d` | `--review-with` orders reviews before `finalize_batch` |

The intake's "Recommended first wave" (#1-4) plus wave B/C/D/E all
shipped. No outstanding contracts from this intake.

Operator follow-on intake covering token-burn safety + model bakeoffs:
`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md` —
addendum to this one, decomposed separately starting 2026-05-21 with
`cli-operator-safety-p0` (bundled cancel + hard-limits + scope-guardrails).

## Out of scope for this intake

- The QA22 inspector-polish wave (`2026-05-19-qa-operator-round-4.md`).
  Different surface (UI polish + bridge bugs) vs. CLI/data-model adds.
- SPA experiment-ledger UX. Could come later as a V2x roadmap item.
- xvision-play / live-trading hooks. The agent's brainstorm is squarely
  about research-loop ergonomics; live execution is not in the ask.

## Verbatim agent message

> Processed the interrupted context: I loaded the brainstorming
> guidance and the xvision strategy ops / repo skills. The key
> relevant reminder is: the current workflow works, but it's too much
> manual API orchestration — strategy creation, agent attachment, eval
> launch, polling, and result comparison are being glued together with
> Python.
>
> **Short answer**
>
> Yes: the Python scripting could mostly be removed.
>
> What I needed Python for was not "strategy intelligence"; it was
> workflow plumbing:
>
> - POST strategy / agent / eval requests
> - poll until runs finish
> - normalize metrics
> - count decisions and action types
> - compare scenarios
> - preserve IDs
> - avoid stale UI/browser assumptions
>
> That should be first-class in xvn.
>
> The ideal CLI would let an agent do:
>
>     xvn strategy create compression-sniper-v3 --from prompt.md --provider openrouter --model kimi
>     xvn eval batch run compression-sniper-v3 \
>       --scenarios eth-7d,btc-bull-8d,btc-crash-8d,sol-8d \
>       --same-decisions 49 \
>       --mode backtest \
>       --wait \
>       --json
>     xvn eval compare --latest --by scenario --show actions,return,sharpe,drawdown
>
> instead of writing Python around the HTTP API.
>
> ---
>
> **The biggest improvements for agent efficiency**
>
> **1. Make strategy + attached agent creation one command**
>
> Current pain: a "strategy" is not actually runnable unless it has an
> attached provider/model-backed agent. That invariant is easy to miss.
>
> Better:
>
>     xvn strategy create \
>       --name "ETH 4h compression sniper v3" \
>       --role trader \
>       --prompt prompt.md \
>       --provider openrouter \
>       --model kimi-k2 \
>       --asset ETH/USD \
>       --timeframe 4h \
>       --json
>
> This should atomically create:
>
> - strategy
> - attached agent
> - slot/provider/model binding
> - role assignment
> - validation report
>
> Output should include:
>
>     {
>       "strategy_id": "...",
>       "agent_id": "...",
>       "eval_ready": true,
>       "provider": "openrouter",
>       "model": "kimi-k2",
>       "warnings": []
>     }
>
> This would remove a lot of manual endpoint probing.
>
> ---
>
> **2. Add `xvn strategy validate`**
>
> Before running eval, I want a single command that answers: "Will
> this actually run?"
>
>     xvn strategy validate 01KS07AZ5R9V7HQ3EWQE1AQAH0 --scenario sc_... --json
>
> It should check:
>
> - strategy exists
> - attached agents exist
> - trader role exists
> - provider/model is set
> - provider/model is enabled
> - scenario asset/timeframe matches prompt assumptions
> - warmup bars are enough
> - expected decision count
> - output schema compatibility
> - common JSON-output risks
>
> Example useful output:
>
>     {
>       "eval_ready": true,
>       "expected_decisions": 49,
>       "asset": "SOL/USD",
>       "timeframe": "4h",
>       "warnings": [
>         "Prompt mentions ETH but scenario asset is SOL/USD"
>       ]
>     }
>
> This would prevent a lot of wasted evals.
>
> ---
>
> **3. Add native batch eval with polling**
>
> Current pain: I used Python because launching 4 evals and waiting
> for all terminal states is annoying manually.
>
> Better:
>
>     xvn eval batch run \
>       --strategy 01KS07AZ5R9V7HQ3EWQE1AQAH0 \
>       --scenarios scenarios.json \
>       --mode backtest \
>       --wait \
>       --poll 2s \
>       --json
>
> It should return one clean object:
>
>     {
>       "batch_id": "batch_...",
>       "strategy_id": "...",
>       "runs": [
>         {
>           "scenario": "ETH 7 day sprint",
>           "run_id": "...",
>           "status": "completed",
>           "return_pct": -8.85,
>           "sharpe": -33.54,
>           "drawdown_pct": 8.85,
>           "decisions": 49,
>           "actions": {
>             "flat": 35,
>             "short_open": 6,
>             "hold": 7,
>             "long_open": 1
>           }
>         }
>       ]
>     }
>
> Important: `--wait` should block until all runs complete, fail,
> cancel, or timeout.
>
> ---
>
> **4. Add "same decision count" as a CLI-level concept**
>
> The user's constraint — "4 scenarios, same number of decisions, each
> under 105" — is exactly the kind of thing agents should not
> hand-roll.
>
> Better:
>
>     xvn scenarios select \
>       --assets ETH/USD,BTC/USD,SOL/USD \
>       --timeframe 4h \
>       --target-decisions 49 \
>       --count 4 \
>       --regimes bull,bear,range,crash \
>       --json
>
> Or:
>
>     xvn eval batch run \
>       --strategy compression-sniper-v3 \
>       --scenario-set crypto-regime-4pack \
>       --same-decisions \
>       --max-decisions 105 \
>       --wait
>
> The CLI should either:
>
> - select matching existing scenarios, or
> - clone/mutate scenarios to normalize decision count.
>
> Right now I had to discover and assemble that manually.
>
> ---
>
> **5. Add scenario cards**
>
> Agents need fast context, not just IDs.
>
> Useful command:
>
>     xvn scenario inspect sc_01KS07B20X5BV3RM6CH3YG69ZQ --card
>
> Output:
>
>     id: sc_01KS07B20X5BV3RM6CH3YG69ZQ
>     name: SOL 8d extended clone
>     asset: SOL/USD
>     timeframe: 4h
>     date_window: 2025-...
>     warmup_bars: 200
>     decision_bars: 49
>     regime_label: trend / expansion
>     volatility_label: high
>     source: cloned_from sc_...
>     previous_runs:
>       count: 3
>       best_return_pct: -2.32
>
> This would help me reason without repeatedly querying and
> reconstructing context.
>
> ---
>
> **6. Add run comparison as a first-class report**
>
> Current pain: I had to manually fetch runs, extract metrics, count
> actions, and compare.
>
> Better:
>
>     xvn eval compare \
>       --runs 01KS07BE...,01KS07D9...,01KS07F2...,01KS07GZ... \
>       --sort return \
>       --json
>
> Or:
>
>     xvn eval compare --batch batch_... --markdown
>
> It should report:
>
> - return
> - Sharpe
> - max drawdown
> - decisions
> - action distribution
> - avg hold duration
> - number of position flips
> - worst trade
> - best trade
> - late-entry count if detectable
> - repeated re-entry count
> - invalidation failures
>
> For trading strategy iteration, the action behavior matters almost
> as much as return.
>
> ---
>
> **What information would help me create better strategies?**
>
> **1. A strategy manifest with hypothesis fields**
>
> Instead of only a freeform prompt, each strategy should have
> structured intent:
>
>     name: compression-sniper-v3
>     family: compression-breakout
>     hypothesis: >
>       Price compression followed by directional expansion has favorable R:R
>       when broader trend and volume confirm.
>     target_regime:
>       - post-compression trend
>     avoid_regime:
>       - chop
>       - late parabolic move
>       - low-volume fakeout
>     asset_assumptions:
>       - works better on high-beta majors
>     timeframe_assumptions:
>       preferred: 4h
>     entry_logic:
>       - compression
>       - breakout
>       - confirmation
>     exit_logic:
>       - invalidation
>       - momentum failure
>       - time stop
>     risk_logic:
>       max_trade_frequency: low
>       no_direct_flips: true
>
> Then the agent can compare results against the actual hypothesis
> instead of just seeing "lost money."
>
> ---
>
> **2. Decision trace summaries**
>
> Raw decisions are useful, but I need higher-level diagnostics.
>
> For each run:
>
>     behavior_summary:
>       flat_rate: 73%
>       trades_opened: 10
>       direct_flips: 0
>       avg_bars_held: 3.2
>       reentries_after_loss: 4
>       entries_after_large_move: 5
>       exits_on_invalidation: 2
>       held_through_invalidation: 3
>       primary_failure_mode: late_entries
>
> That would make iteration much sharper.
>
> Instead of guessing "maybe too many shorts," I could say:
>
>     "This version lost because it entered after expansion was
>     already exhausted and held through momentum invalidation."
>
> ---
>
> **3. Built-in baseline comparison**
>
> Every eval should automatically compare to:
>
> - buy and hold
> - always flat
> - simple trend baseline
> - simple mean-reversion baseline
> - previous version of same strategy family
>
> Example:
>
>     strategy_return: -2.32%
>     buy_hold_return: +6.8%
>     always_flat_return: 0%
>     trend_baseline_return: +3.1%
>     relative_to_buy_hold: -9.12%
>
> This matters because "lost money" is not enough. In a crash
> scenario, losing -2% might be good if buy-and-hold lost -15%. In a
> bull scenario, -2% might be terrible.
>
> ---
>
> **4. Scenario regime labels**
>
> Strategy design depends heavily on regime. I need structured labels
> like:
>
>     regime:
>       trend: up
>       volatility: expanding
>       liquidity: normal
>       chop_score: low
>       event_type: post-crash recovery
>       directional_persistence: high
>
> Then I can ask better questions:
>
> - Did compression sniper only fail in chop?
> - Did it work better in high-vol expansion?
> - Is it asset-specific or regime-specific?
> - Is the issue timeframe or logic?
>
> Right now I'm inferring too much from scenario names.
>
> ---
>
> **5. A strategy experiment ledger**
>
> Every run should attach to an experiment record:
>
>     experiment: compression-sniper-v2-cross-scenario
>     question: >
>       Does stricter compression breakout logic generalize across four regimes
>       at the same decision count?
>     strategy_id: ...
>     scenario_ids: [...]
>     decision_budget: 49
>     result:
>       profitable_count: 0
>       best_scenario: SOL 8d extended
>       worst_scenario: BTC crash
>     conclusion: >
>       Selectivity improved but no positive edge. Needs regime gate or different family.
>     next_recommendation:
>       - tighten SOL-like trend/compression variant
>       - compare against 1h/2h only after adding stronger invalidation
>
> This would make the tool much more agent-native. Agents need memory
> of experiment intent, not just runs.
>
> ---
>
> **What CLI commands I'd want most**
>
> Strategy lifecycle:
>
>     xvn strategy create --manifest strategy.yaml --json
>     xvn strategy clone old-id --name "v3" --patch prompt.md --json
>     xvn strategy validate strategy-id --scenario scenario-id --json
>     xvn strategy show strategy-id --with-agents --json
>     xvn strategy diff v2 v3
>
> Scenario selection:
>
>     xvn scenario list --asset BTC/USD --timeframe 4h --json
>     xvn scenario inspect scenario-id --card
>     xvn scenario clone scenario-id --days 8 --target-decisions 49 --json
>     xvn scenario set create crypto-4pack --scenarios ... --json
>     xvn scenario set balance --same-decisions --max-decisions 105
>
> Eval running:
>
>     xvn eval run --strategy strategy-id --scenario scenario-id --wait --json
>     xvn eval batch run --strategy strategy-id --scenario-set crypto-4pack --wait --json
>     xvn eval status batch-id --json
>     xvn eval cancel batch-id
>
> Result analysis:
>
>     xvn eval summarize run-id --json
>     xvn eval compare --batch batch-id --markdown
>     xvn eval trace run-id --failures
>     xvn eval diagnose run-id --strategy-hypothesis strategy.yaml
>     xvn eval leaderboard --family compression-breakout --by sharpe
>
> ---
>
> **The specific thing that would have saved the most time**
>
> A single command like this:
>
>     xvn experiment run \
>       --name compression-sniper-v2-cross-scenario \
>       --strategy 01KS07AZ5R9V7HQ3EWQE1AQAH0 \
>       --scenario-set four-regime-same-decisions \
>       --max-decisions 105 \
>       --same-decisions \
>       --wait \
>       --compare \
>       --markdown
>
> And output:
>
>     ## compression-sniper-v2-cross-scenario
>
>     All runs completed. Same decision count: 49.
>
>     - ETH 7 day sprint: -8.85%, Sharpe -33.54, DD 8.85%
>     - BTC bull 8d: -6.17%, Sharpe -23.95, DD 6.17%
>     - BTC crash 8d: -9.56%, Sharpe -8.75, DD 14.68%
>     - SOL extended 8d: -2.32%, Sharpe -2.64, DD 10.74%
>
>     Conclusion:
>     Compression-only v2 remained selective but did not produce positive edge.
>     Best fit: SOL extended trend/compression.
>     Recommended next step: preserve selectivity, add stronger trend/regime gate and invalidation.
>
> That would eliminate almost all ad hoc Python.
>
> ---
>
> **Agent-facing design principle**
>
> The CLI should optimize around experiments, not individual API
> objects.
>
> Right now the underlying objects are:
>
> - strategies
> - agents
> - scenarios
> - eval runs
>
> But the user's actual intent is usually:
>
>     "Try this idea against comparable scenarios and tell me if it's
>     worth pursuing."
>
> So the most agent-efficient abstraction is:
>
>     Hypothesis → Strategy Variant → Scenario Set → Batch Eval → Diagnosis → Next Recommendation
>
> If xvn directly supported that flow, agents could spend more time on
> strategy reasoning and less time babysitting IDs, endpoints, and
> polling loops.
>
> ---
>
> **My recommendation**
>
> I'd improve xvision CLI in this order:
>
> 1. `xvn strategy create/validate` that handles agent attachment automatically
> 2. `xvn eval batch run --wait --json`
> 3. `xvn eval compare` with action distributions
> 4. scenario sets with same-decision-count balancing
> 5. experiment manifests / ledgers
> 6. run diagnosis: late entries, churn, flips, held-through-invalidation
> 7. baseline comparison built into every report
>
> That would turn xvision from "an eval API I can script against" into
> "a strategy research workbench an agent can operate fluently."
