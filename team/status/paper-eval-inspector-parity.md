# paper-eval-inspector-parity — status

**Track:** `team/contracts/paper-eval-inspector-parity.md`
**Owner:** conductor recon (Claude, 2026-05-21)
**Status:** recon complete; **no engine-side gap visible from source**.
Operator-side reproduction needed before a fix can be scoped.

## Contract first-deliverable

> "Root-cause established first. Worker confirms whether paper runs
> persist `DecisionRow`s, equity samples, and broker-fill metadata to
> the same tables that the backtest executor uses."

## Findings

### Decisions persistence — parity

Both `BacktestExecutor` (`crates/xvision-engine/src/eval/executor/backtest.rs`)
and `PaperExecutor` (`.../paper.rs`) write to the shared
`eval_decisions` table via `RunStore::record_decision(&DecisionRow)`.
Paper's emit sites:

- `paper.rs:798` — inherited (early-stop policy) decision
- `paper.rs:1080` — primary decision per cycle
- `paper.rs:1259` — guardrail-rewritten decision
- `paper.rs:1432` — explicit short/flat decision

All four emit `RunChartEvent::Decision` on the same event bus the SPA
listens on at `/live/<run_id>`. Backtest emits the same shape from the
same `DecisionRow` struct.

Failure-path coverage: `recoverable_broker_decision_row` (`paper.rs:242-280`)
builds a synthetic decision row for the case where a broker submit
raised a recoverable error — preserves the agent's intent + tags the
error class in the justification. Persisted via the same
`store.record_decision`.

### Equity samples — parity

Backtest calls `store.record_equity(&run.id, ts, balance)` after every
decision. Paper does the same:

- `paper.rs:812` — primary per-bar equity sample
- `paper.rs:1094` — equity update after a recoverable broker error
- `paper.rs:1266` — equity update post-guardrail
- `paper.rs:1452` — final flatten equity sample

Same `eval_equity_samples` table (or the equivalent — confirmed via
`record_equity`'s single implementation in `RunStore`).

### MetricsSummary — parity

`paper.rs:1523` constructs the same `MetricsSummary` shape backtest
does (`total_return_pct`, `sharpe`, `max_drawdown_pct`, `win_rate`,
`n_trades`, `n_decisions`). Persisted via the same finalize path.

### Frontend rendering — no mode-fork

`frontend/web/src/routes/eval-runs-detail.tsx`:

- Line 261: `equityCurve={detail.equity_curve}` — unconditional
- Line 605-617: KPI grid (Sharpe / Max DD / Gross % / Total PnL / Mode
  / Started / Completed) — unconditional, sourced from `summary`
- Line 280-282: `<DecisionsPanel rows={detail.decisions} />` —
  unconditional, no mode-aware branch

Mobile route (`eval-runs-detail-mobile.tsx`) — same: `summary.mode` is
displayed but not used to fork render paths.

`RunDetail` wire shape (`api/types.gen/RunDetail.ts`):

```
{ summary: RunSummary, decisions: DecisionRowDto[], equity_curve: EquityPoint[] }
```

Identical for both modes.

## Conclusion

**From source inspection alone, the paper inspector should display the
same Total PnL summary, decisions table, and orders as the backtest
inspector.** No engine-side persistence gap, no frontend mode-fork.

The intake's operator report ("PnL shows on backtest, not on paper"
and "Buy sell orders dont show") therefore points to one of three
non-source causes:

1. **A specific live paper run** where the broker rejected every
   submit (Alpaca paper credentials missing/expired, network blocked),
   so the executor only persisted no-op decisions and never recorded
   any meaningful equity delta — the inspector renders blank PnL
   because there's nothing to render.
2. **Mid-flight inspection.** Paper runs at 1-bar cadence are
   minutes-long, not seconds. The operator may have opened the
   inspector before the first cycle finished, seen blank PnL, and
   reported it without waiting for the run to terminate.
3. **A regression that has since been fixed** between when the QA
   Round 4 batch was filed (2026-05-19) and the recent observability
   wave (PRs #277, #244 — harness-prompt-hash, blob-fetch-route).

## Next-step ask

To move this contract forward we need **one of**:

- A specific run_id from a paper eval the operator believes is missing
  data, plus a screenshot of the inspector and the run's JSON from
  `xvn eval show <run_id> --json`. With those we can diff
  expected-vs-actual at the data layer.
- A reproducer script that creates a paper run on a known-good
  scenario and captures the inspector's render against
  `expect`-style assertions in an e2e test.
- Operator confirmation that the issue is no longer reproducible on
  current main, in which case this contract closes as "resolved by
  observability wave" without code change.

## Recommendation

Park the contract at `status: blocked` (blocked on operator
repro) until one of the three asks above lands. Don't add speculative
"defensive" rendering code — per `feedback_alpha_root_cause`, the
right move is to fix the actual cause once we can see it.
