# Live/Eval Parity Crosswalk — 2026-06-12 QA Addendum

This is the third-agent QA lane for the 2026-06-12 profit-path and eval-round work.
It is intentionally doc-only so it can run beside the active implementation agents
without colliding with their worktrees.

Sources:
- `docs/superpowers/plans/2026-06-12-profit-path-audit-and-plan.md` from PR #948.
- `docs/research/2026-06-12-eval-round-platform-findings.md`.
- `docs/research/filter-capability-roadmap.md`.
- GitHub issues #932-#945.

## Non-Interference Contract

Do not edit these active implementation zones from this lane:

| Zone | Current owner | Reason |
|---|---|---|
| `crates/xvision-engine/src/eval/executor/backtest.rs` live region | PR #948 WS1 / `xvision-914`; QA Batch B also has edits in flight | Single-writer region for live parity, SL/TP, filter gate, cadence, win-rate, and retry call sites. |
| `crates/xvision-engine/src/agent/llm.rs`, `execute_cline.rs`, exit-enforcement tests | QA Batch B in `.worktrees/qa-release-manager-20260612` | PF-18 bracket-schema implementation is already in progress. |
| `eval_decisions` migrations, `DecisionRow`, export shape | Future PF-19/PF-20 batch | Storage changes need one deliberate migration owner. |
| `MetricsSummary`, finalize metrics, ts-export files, trust UI | PR #948 WS2 / `xvision-gzu` | Honesty envelope and UI trust frame are a coordinated track. |
| broker traits, reconciliation, `RealBrokerFills`, safety gate | PR #948 WS3 / `xvision-x6j` | Live survivability and account-equity sizing are a coordinated track. |

This lane owns only the parity governance below unless explicitly promoted by the
operator.

## Parity Gate

Every new eval, filter, risk, sizing, exit, or observability feature must answer
all five questions before implementation is marked ready:

1. **Backtest path:** Which deterministic code path computes or emits it during historical eval?
2. **Live path:** Which live-loop or broker path computes or emits the same concept during paper/live trading?
3. **Evidence path:** Where is it persisted and exported so backtest and live can be compared after the run?
4. **Operator surface:** Where does CLI/UI show it, and does the surface distinguish missing, legacy, or excluded data from zero?
5. **Parity test:** Which test proves identical inputs produce equivalent decisions, exits, metrics, or events across backtest and live?

If any answer is "not applicable", the implementation must persist an explicit
exclusion marker such as `*_excluded: true` or `source: "backtest_only"` rather
than silently omitting the field.

## Crosswalk

| Item | Existing owner | Parity risk | Required parity handoff |
|---|---|---|---|
| PF-17 / #932: SL/TP on filter-gated bars | QA Batch B, plus PR #948 WS1 WU1.2 live SL/TP | Backtest fix can land without live using the same exit machinery. | WS1 must add the same `PositionRiskState`/SLTP semantics to live and cover gated in-position bars in the live parity harness. |
| PF-18 / #933: bracket fields in trader schema | QA Batch B | Models can emit brackets, but live seeds or repair paths may still hide them. | Schema, Cline repair, backtest seed, and live seed must document the same optional fields and ranges. |
| PF-19 / #934: bracket persistence/export | Future QA batch | Backtest can persist brackets while live decision rows remain unauditable. | `DecisionRow`/export must carry effective bracket plus provenance for both backtest and live entries. |
| PF-02 / #935: `total_return_pct` units | PR #948 WS2-adjacent | Product trust UI could freeze a metric basis before units are settled. | Define `total_return_pct` as NAV/equity-curve return across backtest and live; any risk-capital return gets a distinct field. |
| PF-06/PF-09 / #938: filter stats, synthesized rows, trade-count semantics | PR #948 WS2-adjacent | UI can compare live and eval while counting synthetic rows or EOD liquidation differently. | Persist and surface `real_decisions`, `synthesized_decisions`, `filter_fire_rate`, and `liquidation_count` consistently. |
| PF-01/PF-05/PF-07 / #936/#938: run inspect, tokens, cost | PR #948 WS2/WS5-adjacent | Agent-run accounting and eval accounting can keep disagreeing. | Pick one per-run accounting source of truth; CLI/UI must display provenance and non-zero eval-pipeline calls. |
| CAP-941 / #941: position tokens + manage block | Wave 2 after exits real | Manage wakes may work in backtest but not live, recreating the original parity bug. | Position tokens must derive from a shared position snapshot abstraction used by backtest and live; live parity must include manage-wake cases. |
| CAP-945 / #945: default trigger context + branch attribution | Wave 2, low collision if scoped to filter context | Backtest can tell the model why it woke while live remains blind. | `filter_context.conditions`, branch labels, and `fire.reason` must be emitted into both backtest and live seeds and into `filter_events`. |
| CAP-942 / #942: offline filter replay | Independent Wave 2 | Replay can become a third truth that disagrees with eval/live filter behavior. | Replay must call the same filter evaluation module as backtest; acceptance includes replay count matching a real eval and live emitting the same event shape. |
| CAP-943 / #943: conviction/risk-at-stop sizing | Wave 2 after bracket source exists; intersects WS3 WU3.6 | Risk-at-stop can size from scenario equity in backtest but account equity live. | Sizing must record input equity source, stop distance, clamp reason, and effective notional; live uses account-equity sync from WS3. |
| CAP-944 / #944: new deterministic tokens | Wave 2 after replay or shared token tests | Token math can differ between replay, backtest, and live stream warmup. | Token tests must cover the shared evaluator; any live warmup limitation is surfaced as unavailable, not false. |
| UF-03: no-LLM always-enter-on-fire baseline | Unfiled | Baseline can be useful in eval but unavailable to optimizer/live evidence. | Add as an eval baseline first; WS4 can gate model value over filter-only after metric units and CI are settled. |
| UF-04/UF-08: partial close, cooldown by win/loss | Unfiled, after realized exits | These are exit-dependent and easy to implement on one path only. | Do not file for implementation until PF-17/PF-19 and live SL/TP parity are merged. |

## Recommended Third-Agent Work Order

1. Keep this lane doc-only until Batch B and WS1 settle their `backtest.rs` changes.
2. Ask the release-manager branch to import the **Parity Gate** section into `QA_TRACKER.md`.
3. For every remaining item in `QA_TRACKER.md`, add a `Live/eval parity` evidence column before it moves from `not-started` to `in-progress`.
4. Prefer first implementation slice after tracker import: #945 trigger-context propagation, because it can be scoped to shared filter context and event/export surfaces without owning the live loop.
5. Defer #941, #943, partial close, and cooldown asymmetry until the realized-exit train and live SL/TP parity are both green.

## Release Blocking Rule

No new profitability-relevant feature should be accepted with only a backtest proof.
The minimum release evidence is one of:

- a live parity harness test proving equivalent behavior under identical bars and decisions;
- a documented live exclusion marker rendered in CLI/UI; or
- an explicit dependency on the WS1/WS3 live parity owner before the item can leave `not-started`.
