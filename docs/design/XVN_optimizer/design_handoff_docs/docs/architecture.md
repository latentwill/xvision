# xvn — Architecture

> **ARCHIVED** — two-stage Intern→Trader architecture superseded by the single-stage agent model (2026-06).

> A multistrategy loom with on-chain reputation.

**Pipeline** · 4 stages · **LLM roles** · Intern + Trader · **Risk** · deterministic · **Headline metric** · Δ-Sharpe · **Updated** 2026-05-20.

## Context for AI agents

- **route**: `/docs/architecture`
- **summary**: Thesis is that a multistrategy population evaluated through a deterministic loom, with ERC-8004 reputation and validation receipts, produces a credibly auditable ranking of trading strategy variants. Hackathon claim is narrower than long-term thesis.
- **stages**: Stage 1 · Intern · Stage 2 · Trader · Risk Layer · Stage 3 · Execution
- **key terms**: `InternBriefing`, `TraderDecision`, `TraderArm`, `BacktestExecutor`, Δ-Sharpe, `PassesBothRegimes` gate, flight recorder, ERC-8004, Mantle
- **do not**: treat the Intern as a recommender (it emits balanced evidence only) · run a strategy variant to paper without the anti-overfit gate · assume `xvision-identity` is in the default build (it is opt-in)

## Thesis

A multistrategy population evaluated through a deterministic loom, with on-chain reputation and validation receipts via ERC-8004, produces a **credibly auditable ranking of trading strategy variants**. The system is a marketplace in shape — strategies have provenance, performance history, and fork lineage as first-class on-chain artifacts.

The hackathon claim is narrower than the long-term thesis. We are *not* claiming the loom can self-improve indefinitely (that's the deferred Karpathy autoresearch direction). The hackathon claim is:

> On a fixed set of trading setups, a population of N strategies (classical TA + onchain + LLM-driven) evaluated through the loom produces an on-chain ranking that distinguishes strategies beyond noise on a pre-committed risk-adjusted return metric, with reputation and validation receipts visible on Mantle.

## Pipeline · Intern → Trader → Risk → Execution

Four stages. Two named LLM roles. The **Intern** prepares neutral, balanced evidence — bull case, bear case, flat case, signal inventory, regime — but commits to no action. The **Trader** receives the briefing and produces the actual decision via a vanilla LLM call. The **risk layer** between the Trader and Execution stage is deterministic code: no model in the loop.

```
                 ┌────────────────┐
   Setup ──────► │  Stage 1       │  Intern
                 │  Intern        │  • neutral evidence prep
                 │                │  • bull / bear / flat cases
                 └───────┬────────┘  • NO candidate decision
                         │  Briefing (JSON)
                         ▼
                 ┌────────────────┐
                 │  Stage 2       │  Trader · one Strategy variant
                 │  Trader        │  in the loom
                 │                │  • LLM judgment on briefing
                 └───────┬────────┘
                         │  Decision JSON
                         ▼
                 ┌────────────────┐
                 │  Risk Layer    │  Deterministic veto
                 │  (rules code)  │  • position / loss / whitelist
                 └───────┬────────┘
                         │  Approved decision (or veto)
                         ▼
                 ┌────────────────┐
                 │  Stage 3       │  Execution
                 │  Execution     │  • Alpaca paper · Orderly
                 │                │  • strict tool calls only
                 └────────────────┘
```

Why the split is structured this way: an earlier draft had the Intern emit a candidate direction and size. That collapsed Stage 2 into a calibrator that simply rubber-stamped the Intern's anchor. Asking the Intern to hand off *evidence*, not *recommendations*, gives the Trader a real decision to make.

## Stage 1 · Intern

**Purpose.** Produce a structured, neutral evidence briefing. The Intern researches; it does not recommend. The output is symmetric by construction so the Trader makes a clean judgment from balanced inputs.

### Backends

- **OpenAI-compatible** · default for non-Anthropic models. One impl covers OpenAI, OpenRouter, Together, Groq, DeepSeek, xAI, Mistral, vLLM, Ollama, LM Studio, llama.cpp, TGI.
- **Anthropic** · Claude via Messages API. `claude-haiku-4-5` for speed, `claude-sonnet-4-6` for quality.
- **Local candle** · optional, deferred. Direct in-process inference for air-gapped runs without an HTTP hop.
- **Reasoning models** · first-class. Backend strips provider-native reasoning fields and inline `<think>` blocks before JSON validation; forwards `reasoning_effort` when supported.

### Briefing schema · the Intern → Trader contract

```json
{
  "cycle_id": "uuid",
  "asset": "BTC-PERP",
  "bull_case": "strongest argument for going long",
  "bear_case": "strongest argument for going short",
  "flat_case": "strongest argument for sitting this one out",
  "evidence_long":  ["rsi_oversold", "smart_money_inflow", "funding_rate_neg"],
  "evidence_short": ["volume_declining", "lower_high_lower_low"],
  "evidence_flat":  ["chop_in_5pct_range_3d", "low_signal_quality"],
  "regime": "trending | choppy | high_vol | low_vol",
  "signal_quality": 0.62,
  "horizon_hours": 4
}
```

The Intern's prompt explicitly instructs: *"Present balanced cases on all three sides. Do not recommend an action. Your job ends with the briefing — the Trader will decide."* No `candidate_direction` field. No `candidate_size_bps`. Those would commit the decision before the Trader gets to evaluate the evidence.

`signal_quality` is the analyst's estimate of how clean the setup is — a quality signal, not a directional signal. `regime` is directionally neutral.

## Stage 2 · Trader

**Purpose.** Make the final trading decision based on the Intern's neutral evidence briefing. Stage 2 is one Strategy variant in the loom; other strategies are classical TA, onchain-signal, or hybrid implementations. The Trader is wrapped as a `Strategy` adapter (`TraderArm`) so it competes on identical setups.

```json
{
  "cycle_id": "uuid",
  "action": "buy | sell | flat | close",
  "size_bps": 75,
  "direction": "long | short | flat",
  "stop_loss_pct": 2.5,
  "take_profit_pct": 5.0,
  "trader_summary": "one-line rationale"
}
```

## Risk layer · the deterministic gate

Pure rule evaluation between Stage 2 and Stage 3. No LLM. The risk layer either passes the decision through unchanged, modifies sizing downward, or vetoes the decision entirely. **It never increases size or flips direction.**

| Rule | v1 default |
|---|---|
| Max position size | No single position larger than **20%** of portfolio NAV. |
| Max total exposure | Sum of absolute position sizes ≤ **100%** of NAV (no leverage in v1). |
| Asset whitelist | Only assets in `config/whitelist.yaml` are tradeable. |
| Daily loss circuit breaker | If realized + unrealized loss for the day exceeds **5%** of starting NAV, all new entries are vetoed until rollover. |
| Max open positions | ≤ **5** concurrent positions. |
| Correlation cap | No more than two positions in the same correlation cluster (BTC-cluster, ETH-cluster, SOL-cluster). |
| Stop loss required | Every entry must specify a stop loss; decisions without one are rejected. |

**Vetoes are signal.** Every veto is logged with reason. They tell us when a strategy pushes the agent into regions a human risk manager would also reject.

## Stage 3 · Execution

| Path | Notes |
|---|---|
| **Backtest** | Default eval path. `BacktestExecutor` replays cached OHLCV bars, simulates fills, persists decisions / equity / metrics, and emits chart events for live dashboard streams. |
| **Alpaca paper** | Broker-surface path for paper evals and manual smoke commands. Uses `AlpacaPaperSurface` / `AlpacaExecutor`. `cycle_id` is forwarded as `client_order_id` so duplicate retries collapse at the venue boundary. |
| **Orderly · Mantle** | Lives in `xvision-execution::OrderlyExecutor` for live perps experiments. Not the default engine eval broker today; the enum keeps `OrderlyLive` as a stub until live UX is wired. |

## Eval framework

The eval framework is the most important non-obvious piece of this project. Without it, strategy comparisons cannot be measured and the long-term autoresearch loop has nothing to learn from.

### Pre-committed primary metric

**Δ-Sharpe** — annualized Sharpe of Strategy A minus annualized Sharpe of Strategy B, evaluated on the same set of setups, paired. The single number the demo lives or dies on.

### Secondary metrics

| Metric | Read |
|---|---|
| Max drawdown | Peak-to-trough loss, %. Risk profile. Must not be catastrophic for either strategy. |
| Profit factor | Gross wins / gross losses. Intuitive, demo-friendly. |
| Win rate | % of trades profitable. Caveat: high win rate with bad profit factor is a warning sign. |
| Decision divergence | % of setups where Strategy A and Strategy B produced different actions. |

### Statistical significance

- Minimum 30 paired trades for any signal interpretation.
- Target 100+ paired trades for the headline number.
- Report 95% confidence interval on Δ-Sharpe via paired bootstrap (10k resamples).

### Anti-overfit gate

No strategy variant advances to paper trading unless it shows positive Δ-Sharpe vs a pre-committed baseline in **at least one pre-2023 bear regime** *and* **at least one 2023–2024 bull regime**. v1 reports a named verdict (`PassesBothRegimes` / `SingleRegimeEvidence` / `Fails`); the gate re-tightens to blocking when an automated optimizer ships.

### Structured traces · flight recorder

Every Stage 1 and Stage 2 call produces a structured trace record persisted to SQLite alongside the briefing and decision.

| Field | Value |
|---|---|
| per call | `run_id` · `cycle_id` · `stage` (intern \| trader) · `arm_name` |
| model | backend identifier · provider · model |
| input | full system prompt + user content |
| output | raw model output (full JSON string, pre-parse) |
| validation | parse success · garde validation errors |
| timing | tokens (prompt + completion) · latency (ms) |
| errors | any exception with traceback |

## ERC-8004 · on-chain identity

ERC-8004 identity support is **optional** in the current codebase. The `xvision-identity` crate is excluded from default workspace builds, ships draft Identity/Reputation registry bindings, and is intentionally not required for the dashboard, CLI, or eval pipeline to run.

| Registry | Status |
|---|---|
| Identity Registry | ERC-721 agent NFT. `agentURI` manifest for a strategy or agent identity. Manifest construction and client calls exist; production wiring is gated by ADR 0008. |
| Reputation Registry | Signed outcome posts keyed by agent token id. Eval attestations are persisted locally today; on-chain posting is a follow-on integration. |
| Validation Registry | Per-trade validation proofs. Implementation has local `EvalAttestation` signing and persistence; on-chain submission not wired into default Stage 3. |

### On-chain footprint

| Artifact | Size | Location | When |
|---|---|---|---|
| Strategy manifest (JSON sidecar) | ~500 B | IPFS / Arweave | Once per strategy mint / fork |
| `strategy_manifest_cid` in agent NFT metadata | 32–64 B | Mantle · Identity Registry | Once at strategy mint |
| `agent_id` + receipt fields per trade | ~32 B | Mantle · Validation Registry | After every closed position |
| Reputation entry per experiment run | ~64 B | Mantle · Reputation Registry | After each backtest / paper run |

EVM storage at 20K gas per 32-byte slot keeps per-trade and per-run costs bounded even on Mantle.

## Cargo workspace

```
xvision/
├── Cargo.toml                    # workspace root
├── rust-toolchain.toml
│
├── crates/
│   ├── xvision-core             # types · schemas · config · SQLite · manifests
│   ├── xvision-data             # OHLCV ingest · indicators · onchain signals
│   ├── xvision-intern           # Stage 1 (OAI- or Anthropic-compat HTTP)
│   ├── xvision-trader           # Stage 2 (TraderBackend trait · optional candle)
│   ├── xvision-risk             # deterministic risk layer
│   ├── xvision-execution        # Broker/executor surfaces: Alpaca + Orderly
│   ├── xvision-engine           # Shared API: agents, strategies, scenarios, eval
│   ├── xvision-dashboard        # Axum API + embedded Vite SPA
│   ├── xvision-identity         # Optional · ERC-8004 manifest + reputation
│   ├── xvision-eval             # Legacy backtest harness · baselines · Δ-Sharpe
│   ├── xvision-harness          # boundary probes (minimal v1 corpus)
│   └── xvision-cli              # clap-based CLI; installed binary is `xvn`
│
├── config/                       # TOML configs (whitelist, risk)
├── data/probes/                  # boundary probe corpus
├── notebooks/                    # eval plotting (Python, offline)
└── docs/
```

Each crate's public API is a typed surface; cross-crate calls fail to compile if the contract doesn't match. `xvision-data` cannot reach into `xvision-eval`'s internals.

---

Reconciled with `architecture.md` (post-ADR-0011 slim-down) at commit `a73b18f` on 2026-05-20.
