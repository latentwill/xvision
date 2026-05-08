# Eval Engine — Design

> **Status:** Draft for user review · 2026-05-08
> **Companion specs:** [Strategy Creation Engine](./2026-05-08-strategy-creation-engine-design.md) (the producer of artifacts this engine consumes). Brainstorm history at [`2026-05-08-eval-engine-decisions-so-far.md`](./2026-05-08-eval-engine-decisions-so-far.md).

---

## 1. Scope and relationship to other engines

The Eval Engine is one of Xianvec's four engines (per the Strategy Creation Engine spec §1):

- **Strategy Creation Engine** — produces strategy bundles (the input)
- **Eval Engine** *(this spec)* — runs strategies against scenarios, produces metrics + findings + receipts (the output)
- **Marketplace** — surfaces eval attestations on listings
- **Identity** — signs eval results into ERC-8004 reputation receipts

The eval engine consumes a strategy bundle (from the creation engine) and a **scenario** (a deterministic configuration of time range + asset universe + capital + risk + slippage + regime tags). It runs the strategy through the scenario in either backtest or paper mode, captures every decision and fill, computes metrics, and emits a structured **finding record** for downstream consumers (autoresearcher loop, marketplace listings, comparison views).

## 2. Locked decisions (from brainstorm)

| # | Decision |
|---|---|
| 1 | **Modes:** backtest + paper, toggleable per run via clean execution-surface abstraction. |
| 2 | **Scenario knobs:** time/asset/regime + capital/position/risk + slippage/fee/latency. Synthetic stress deferred. |
| 3 | **Runs are first-class artifacts** identified by (strategy, params, scenario) triple. Comparison renders any subset. |
| 4 | **Visualization:** Rust axum server + SPA dashboard. Live progress via SSE. |
| 5 | **Chart library:** TradingView Lightweight Charts (Apache 2.0). Advanced as post-hackathon upgrade path. |
| 6 | **NL evaluator v1:** structured-finding extractor only (JSON records). Q&A surface deferred to v2. |
| 7 | **Concurrency tiering:** free = 1 scenario at a time. Paid = unlocks batch sweeps over parameter grids. |
| 8 | **Persistence:** SQLite + JSONL trade tapes on disk. ULID run ids. Postgres migration deferred. |
| 9 | **Architecture:** folded into the greenfield `xianvec-engine` crate as `xianvec-engine/src/eval/`. Old `xianvec-eval` deprecated. |
| 10 | **Cost preview:** show estimated **token count** before run (not dollar cost — token usage is deterministic, dollars vary by provider/model). |

## 3. Architecture

The eval engine is a module inside `xianvec-engine`, sharing the same persistence (SQLite), scheduler (ported from SwarmClaw), and tool registry as the strategy creation engine. This unification means strategies can be authored, evaluated, and published from the same binary state without crossing crate boundaries.

```
xianvec-engine/
└── src/
    ├── strategy/      # bundle, templates, slots (per strategy creation spec)
    ├── skills/        # OSShip-style skill marshaling
    ├── tools/         # tool registry shared by strategies + eval-time agents
    ├── scheduler/     # durable scheduler (ported from SwarmClaw)
    ├── eval/          # THIS SPEC
    │   ├── run.rs           # Run identity + lifecycle
    │   ├── scenario.rs      # Scenario definition + fixtures
    │   ├── executor.rs      # Backtest sim + paper exec dispatch
    │   ├── store.rs         # SQLite + JSONL persistence
    │   ├── metrics.rs       # Sharpe, drawdown, win rate, etc.
    │   ├── findings.rs      # NL extractor (LLM client + record schema)
    │   ├── compare.rs       # Run-set comparison logic
    │   └── progress.rs      # SSE event emitters
    ├── sealing/       # Tier A/B publishing
    ├── marketplace/   # listings + 8004 calls
    ├── mcp/           # MCP server
    ├── cli/           # CLI verbs
    └── dashboard/     # axum + SPA
```

## 4. Run model

A **run** represents one execution of (strategy_bundle, scenario, params_override).

```rust
struct Run {
    id: Ulid,
    strategy_bundle_hash: ContentHash,  // resolves to strategy bundle
    scenario_id: Ulid,                  // resolves to scenario fixture
    params_override: Option<Json>,       // strategy params overridden for this run
    mode: RunMode,                       // Backtest | Paper
    status: RunStatus,                   // Queued | Running | Completed | Failed | Cancelled
    started_at: DateTime,
    completed_at: Option<DateTime>,
    metrics: Option<MetricsSummary>,    // populated on completion
    error: Option<RunError>,
}

enum RunStatus { Queued, Running, Completed, Failed, Cancelled }
enum RunMode { Backtest, Paper }
```

**Storage layout per run:**

```
~/.xvn/runs/<run_id>/
  config.json        # full run configuration (strategy hash, scenario, params, mode)
  metrics.json       # final metrics
  trades.jsonl       # one line per trade (entry, exit, fill, P&L)
  decisions.jsonl    # one line per agent decision (slot, prompt, response, tokens)
  equity.parquet     # equity curve sampled at scenario cadence
  findings.jsonl     # one line per finding record (schema in §11)
  events.jsonl       # one line per SSE event emitted (for replay/audit)
```

SQLite tables (`~/.xvn/state.db`):
- `runs` — Run struct above
- `run_metrics_summary` — denormalized for fast leaderboard queries
- `run_attestations` — eval results signed for 8004 publishing
- `scenarios` — registered scenarios

## 5. Scenario format

Scenarios are deterministic; same scenario + same strategy + same seed = same result. Stored as JSON in SQLite with optional fixtures pinned by content hash for reproducibility.

```json
{
  "id": "01H8N7Z...",
  "display_name": "BTC bull regime Q1 2025",
  "description": "Strong uptrend, low volatility regime",
  "time_window": { "start": "2025-01-01T00:00Z", "end": "2025-04-01T00:00Z" },
  "asset_universe": ["BTC/USD"],
  "regime_tags": ["trending_bull", "low_vol"],
  "capital": { "initial": 10000, "currency": "USD" },
  "risk": {
    "max_concurrent_positions": 2,
    "max_leverage": 3,
    "daily_loss_kill_switch_pct": 5
  },
  "slippage": { "model": "linear", "bps": 5 },
  "fees": { "maker_bps": 10, "taker_bps": 25 },
  "latency": { "decision_to_fill_ms": 250 },
  "data_seed": "alpaca-historical-v1",
  "created_at": "2026-05-08T12:00:00Z",
  "created_by": "@xianvec_official"
}
```

A **canonical scenario set** ships with xvn for the marketplace's published-eval baseline:
- `crypto-bull-q1-2025`
- `crypto-bear-q3-2024`
- `crypto-chop-q2-2025`
- `crypto-event-flash-crash-2024-08`

These are the scenarios sellers run against at publish time so buyers see consistent comparable performance numbers across all listings.

## 6. Eval modes

### Backtest mode

Replays historical OHLCV from Alpaca's data API. Strategy fires per its scheduler (e.g., every 15m); decisions are simulated against the next-bar fill with the scenario's slippage + fee + latency model. Pure, fast, parallelizable. The strategy's LLM agents fire identically to live mode — they get the same indicator panels, same decisions are prompted — but the broker call is intercepted and turned into a simulated fill.

### Paper mode

Strategies run forward against Alpaca's paper endpoint. Real-time data, real broker plumbing, simulated funds. Slower (minutes-to-hours per data point), one timeline per run. Used for forward-validation before going live with real money.

Both modes share the same execution-surface trait so strategy code is mode-agnostic. A single run is one mode at a time; switching requires a new run.

## 7. Concurrency and tiering

**Free tier:** 1 scenario in flight at a time. Sequential queue with live status. Simpler scheduler bookkeeping; sufficient for L1/L2/L3 users iterating on individual strategies.

**Paid tier:** unlocks batch sweeps:

```
xvn eval batch --strategy <id> --grid params.json
xvn eval batch --strategies <id_a,id_b,id_c> --scenario <id> --concurrency 8
```

Batch mode runs N runs concurrently (CPU-bound for backtests, sequential for paper against shared rate budget). LLM rate limits are buyer's problem — they're using their own keys.

The tier check happens at scheduler-enqueue time. Free users can submit batches but they execute serially.

## 8. Token estimation

Before running, the eval engine computes a deterministic token estimate based on:
- Number of decision points in the scenario (time_window / decision_cadence)
- Number of LLM slots in the strategy bundle (regime classifier + intern + trader)
- Average token count per slot prompt (measured from prompt + estimated context)
- Scenarios where memory grows (e.g., trader sees recent decisions) bump the estimate

Returned as a structured object, never converted to dollars:

```json
{
  "estimated_tokens": {
    "input": 45000,
    "output": 8500,
    "total": 53500
  },
  "decision_points": 1080,
  "estimated_runtime_seconds": 120
}
```

Wizard / CLI / MCP surface this to the user before confirming the run. The user evaluates whether the token cost fits their budget at their provider's rates.

## 9. Live progress (SSE schema)

axum server emits an SSE stream per active run at `GET /runs/<run_id>/events`. Each line is a JSON event:

```json
{ "type": "run.started", "run_id": "01H8N...", "estimated_tokens": 53500, "ts": "..." }
{ "type": "run.tick", "run_id": "01H8N...", "scenario_progress_pct": 12.5, "current_ts": "2025-01-15T08:00Z" }
{ "type": "agent.fired", "run_id": "01H8N...", "slot": "trader", "tokens_used": 1350, "ts": "..." }
{ "type": "decision.emitted", "run_id": "01H8N...", "action": "long_open", "asset": "BTC/USD", "size": 0.05, "conviction": 0.7 }
{ "type": "fill.recorded", "run_id": "01H8N...", "side": "buy", "price": 95400.0, "qty": 0.05, "fee": 1.19 }
{ "type": "metrics.updated", "run_id": "01H8N...", "equity": 10240.5, "drawdown_pct": 0.0, "n_trades": 3 }
{ "type": "finding.extracted", "run_id": "01H8N...", "kind": "regime_drift", "severity": "info", "evidence": "..." }
{ "type": "run.completed", "run_id": "01H8N...", "metrics": { ... }, "tokens_used": 51200 }
{ "type": "run.failed", "run_id": "01H8N...", "error": "..." }
```

Events also persisted to `events.jsonl` per-run for replay/audit. Dashboard subscribes to SSE for live charts + leaderboard updates; wizard subscribes for in-page progress display.

## 10. Comparison view UI

The comparison view at `/runs/compare?ids=<id_a>,<id_b>,...` renders any picked subset of runs side-by-side or overlaid. Built with TradingView Lightweight Charts.

**v1 panels:**
- **Equity curve overlay** — N lines, color-coded by run, sampled from `equity.parquet`. Shared time axis. Crosshair sync.
- **Trade markers per strategy** — toggle each strategy on/off; selected strategy's buy/sell markers appear on a price chart for the asset.
- **Metrics table** — one row per run: total_return_pct, sharpe, max_drawdown_pct, win_rate, n_trades, tokens_used, total_estimated_cost_in_tokens.
- **Findings panel** — flat list of findings across the selected run set, grouped by kind. Click to drill into the run that produced the finding.

**Deferred to v2:** drawdown overlay, regime-shaded background, trade tape diff, NL Q&A box.

## 11. Findings extractor

The NL evaluator runs once per run (or run-set comparison) at completion. It reads:
- Run's metrics summary
- Trade tape (decisions + fills)
- Equity curve sampled
- Scenario regime tags
- (For comparisons) the same data across all runs in the set

Emits structured findings, one JSON line per finding:

```json
{
  "id": "01H8N...",
  "run_id": "01H8N...",
  "kind": "regime_fit_mismatch | drawdown_concentration | overtrading | underperformance | risk_violation | divergence | ...",
  "severity": "info | warning | critical",
  "summary": "Strategy underperformed in chop sub-regime within bull window",
  "evidence": {
    "metric_name": "sharpe_in_chop_subwindow",
    "value": 0.3,
    "vs_baseline": "buy_and_hold_btc"
  },
  "affected_runs": ["01H8N..."],
  "ts": "..."
}
```

**Schema is versioned** (`schema_version: "1"`) so the autoresearcher loop can rely on stable record shape.

**Prompt + model for v1:** the extractor uses the user's LLM key (NOT a xvn-issued key). Default model is the same model the strategy used for its slot agents (rationale: the user has already authorized that model). Override available via `xvn eval extract-findings --model <provider:model>`.

The prompt template lives at `xianvec-engine/src/eval/findings/prompts/extractor-v1.md` and is OSShip-style markdown so it can be versioned, signed, and updated independently of the binary.

## 12. Pre-computed published evals

When a strategy is published to the marketplace (per Strategy Creation Engine spec §13), the publish flow runs the strategy across the **canonical scenario set** with deterministic seeds and produces signed eval attestations:

```json
{
  "strategy_bundle_hash": "...",
  "scenario_id": "crypto-bull-q1-2025",
  "metrics": { "total_return_pct": 18.4, "sharpe": 1.62, "max_drawdown_pct": 7.1, "win_rate": 0.58, "n_trades": 47 },
  "tokens_used": 51200,
  "ran_at": "2026-05-08T12:00:00Z",
  "signed_by": "<author_8004_identity>",
  "signature": "<ed25519>"
}
```

Buyers see these attestations on the marketplace listing **without spending any LLM tokens of their own.** Custom evals on buyer-defined scenarios still cost the buyer's tokens.

The Marketplace + Identity engines own the storage, on-chain pinning, and reputation aggregation of these attestations; the eval engine produces and signs them.

## 13. CLI surface

```
xvn eval run <strategy_id> --scenario <scenario_id>          # single run
xvn eval run <strategy_id> --scenario <scenario_id> --mode paper
xvn eval run <strategy_id> --scenario <scenario_id> --estimate-only   # token estimate, no run
xvn eval status <run_id>                                     # status + metrics
xvn eval ls                                                  # list runs
xvn eval ls --strategy <id>                                  # filter
xvn eval compare <run_id> <run_id> [<run_id>...]             # opens comparison view in dashboard
xvn eval extract-findings <run_id>                           # re-run findings extractor
xvn eval batch --grid <params.json>                          # paid-tier sweep
xvn eval batch --strategies <a,b,c> --scenario <id>          # paid-tier multi-strategy
xvn eval scenarios ls                                        # list registered scenarios
xvn eval scenarios new <file.json>                           # register a custom scenario
xvn eval publish-attestation <run_id>                        # produce signed attestation for marketplace
```

## 14. MCP surface

Mirrors the CLI for external AI agents. Same verb surface as listed in Strategy Creation Engine spec §10 under "Eval lifecycle":

- `run_eval(strategy_id, scenario_id, mode?, params_override?, estimate_only?) -> { run_id | estimate }`
- `eval_status(run_id) -> { status, progress, metrics_partial }`
- `eval_metrics(run_id) -> MetricsSummary`
- `compare_runs(run_ids[]) -> ComparisonReport`
- `list_findings(run_id | run_ids[]) -> Finding[]`
- `extract_findings(run_id, model_override?) -> Finding[]`
- `list_scenarios(filter?) -> Scenario[]`
- `register_scenario(scenario_json) -> { scenario_id }`
- `eval_batch(grid | strategy_ids[], scenario_id, concurrency?) -> { batch_id, run_ids }`
- `publish_attestation(run_id) -> SignedAttestation`

## 15. Migration plan from `xianvec-eval`

The existing `crates/xianvec-eval/` has working: ab-compare, baselines (always_long, ma_crossover, macd_momentum, rsi_mean_reversion, etc.), bootstrap, gate, harness, metrics, report, result. None of it is wasted.

**Migration approach:** port modules into `xianvec-engine/src/eval/` rather than rewrite. The Strategy trait surface that the existing baselines implement becomes a thin adapter over the new bundle-based execution path so existing baselines keep working as L1 marketplace seed listings.

| Current `xianvec-eval` module | New home |
|---|---|
| `ab_compare.rs` | `eval/compare.rs` (extended for arbitrary run sets, not just A/B) |
| `backtest.rs` | `eval/executor.rs` (backtest dispatch path) |
| `baselines/` | `strategy/templates/baselines/` — converted to bundles (mechanical-only baselines wrapped in single-LLM-decision shim per "all strategies LLM-required" rule) |
| `bootstrap.rs` | `eval/metrics.rs` (CI computation) |
| `gate.rs` | `eval/findings.rs` (gate is one finding kind: "headline_passes_ci") |
| `harness.rs` | `eval/executor.rs` (per-run loop) |
| `lib.rs` | dissolved into `xianvec-engine/src/eval/mod.rs` |
| `metrics.rs` | `eval/metrics.rs` |
| `report.rs` | `dashboard/` (HTML report generator) |
| `result.rs` | `eval/run.rs` (RunResult type) |
| `strategy.rs` | adapter shim in `strategy/legacy.rs` |

**The "all-LLM" rule for baselines:** the existing pure-rules baselines (always_long, ma_crossover, etc.) become "LLM-trivial" templates — a single-slot decision agent that's prompted with the rule outcome and asked to confirm or veto. This satisfies the "all strategies LLM-required" rule without throwing away the baselines as eval anchors.

## 16. Open questions

- **Canonical scenario set finalization.** Pick exact date ranges, regime labels. Coordinate with `decisions/0011-...` if it specifies them.
- **Token estimator accuracy.** First pass uses static estimates per slot prompt. Refine after first batch of real runs.
- **Scenario fixture pinning.** Should canonical scenarios pin actual historical OHLCV data (Parquet snapshot) or rely on Alpaca's API being deterministic? Pinning is more reproducible; Alpaca-call is lighter. Lean toward pinning at publish time.
- **Findings extractor prompt variants.** v1 ships one prompt. Should there be specialized prompts per regime / asset type / strategy template?
- **Comparison view performance with N=20+ runs.** Equity curve overlay readability degrades; need to think about color palette + selectability.
- **Receipt issuance cadence for live runs** (when the buyer goes live, when do attestations flow to 8004 identity? per-trade? per-day? per-week?). Coordinate with Identity engine spec.
- **Eval engine's role in the autoresearcher Karpathy loop.** Findings are the substrate; how does the loop *consume* them — pull via MCP, push via SSE, or batch-export?

## 17. Out of scope

- Synthetic stress scenarios (gaps, dropouts, flash crashes) — deferred per brainstorm.
- Q&A surface over findings — v2 per brainstorm.
- Native desktop wrapper (Tauri/Dioxus) — web dashboard at localhost is the v1 surface.
- Postgres + object storage migration — deferred until marketplace ships.
- Multi-tenant eval-as-a-service hosting — Xianvec doesn't run buyers' evals.
- Drawdown overlay, regime-shaded background, NL Q&A box in comparison view — deferred to v2.
- The Karpathy autoresearcher improvement loop itself (consumes findings, doesn't constrain this spec).

## 18. Decision log (this brainstorm, 2026-05-08)

- **Modes:** backtest + paper toggleable.
- **Scenario knobs:** time/asset/regime + capital/risk + slippage/fee/latency. No synthetic stress.
- **Comparison axis:** runs are arbitrary (strategy, params, scenario) triples; comparison is over picked subsets.
- **Viz host:** Rust axum + SPA, SSE for live progress.
- **Chart lib:** TradingView Lightweight Charts (Apache 2.0). Advanced post-hackathon.
- **NL evaluator v1:** structured findings extractor only. Q&A v2.
- **Concurrency tiering:** free = 1 scenario, paid = batch sweeps.
- **Persistence:** SQLite + JSONL tapes + Parquet equity. ULID run ids.
- **Architecture:** folded into `xianvec-engine` as `eval/` module. Old `xianvec-eval` deprecated, modules ported.
- **Cost preview:** estimated tokens (not dollars).
- **Findings extractor model:** uses user's LLM key, defaults to same model as strategy slots.
- **Pre-computed published evals:** sellers run the canonical scenario set at publish time, sign attestations, marketplace surfaces without buyer-token cost.
- **All baselines preserved** as single-LLM-decision shim wrappers to satisfy "all strategies LLM-required" rule without throwing away working code.
