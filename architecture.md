# XIANVEC — Architecture

> Multistrategy trading agent with on-chain reputation. Hackathon scope: prove that a population of strategies evaluated through a deterministic loom, with ERC-8004 reputation/validation receipts, produces a credibly auditable ranking of strategy variants.

---

## 1. Thesis

A multistrategy population evaluated through a deterministic loom, with
on-chain reputation and validation receipts via ERC-8004, produces a
credibly auditable ranking of trading strategy variants. The system is a
*marketplace* in shape — strategies have provenance, performance history,
and fork lineage as first-class on-chain artifacts.

The hackathon claim is narrower than the long-term thesis. We are not yet
claiming the loom can self-improve indefinitely (that's the deferred
Karpathy autoresearch direction). The hackathon claim is:

> On a fixed set of trading setups, a population of N strategies (classical
> TA + onchain + LLM-driven) evaluated through the loom produces an
> on-chain ranking that distinguishes strategies beyond noise on a
> pre-committed risk-adjusted return metric, with reputation and validation
> receipts visible on Mantle.

Everything in this document is in service of evaluating that claim cleanly.

---

## 2. System overview

A four-stage pipeline with two named LLM roles: **Intern** (Stage 1) and **Trader** (Stage 2). The Intern prepares neutral, balanced evidence — bull case, bear case, flat case, signal inventory, regime — but commits to no action. The Trader receives the briefing and produces the actual decision, with disposition control vectors active on its hidden states. Vectors live in exactly one place. The risk layer between the Trader and the Execution stage is deterministic code, no model in the loop.

```
                 ┌────────────────┐
   Setup ──────► │  Stage 1       │  Intern
                 │  Intern        │  • neutral evidence prep
                 │  (no vectors)  │  • bull/bear/flat cases
                 └───────┬────────┘  • NO candidate decision
                         │ Briefing (JSON)
                         ▼
                 ┌────────────────┐
                 │  Stage 2       │  Trader (the experiment)
                 │  Trader        │  • local quantized model
                 │  (vectors ON)  │  • makes the actual call
                 └───────┬────────┘
                         │ Decision JSON
                         ▼
                 ┌────────────────┐
                 │  Risk Layer    │  Deterministic veto
                 │  (rules code)  │  • position/loss/whitelist limits
                 └───────┬────────┘
                         │ Approved decision (or veto)
                         ▼
                 ┌────────────────┐
                 │  Stage 3       │  Execution
                 │  Execution     │  • Alpaca paper API
                 │  (no vectors)  │  • strict tool calls only
                 └────────────────┘
```

Why the split is structured this way: a previous draft had the Intern emit a candidate direction and size. That made Stage 2 a calibrator in disguise — vectors could only nudge sizing because the textual prompt had already committed Stage 2 to the Intern's recommendation. Vectors operate on hidden-state geometry; prompt conditioning operates on token attention. The latter generally wins. To give vectors real room to drive the decision, the Intern must hand off *evidence*, not *recommendations*. Bull case / bear case / flat case is symmetric by construction. The Trader sees balanced inputs and the disposition vectors get clean influence over what the model actually decides.

Vectors cannot influence tool call formatting, schema enforcement, or risk rules. They only shape the decision content emitted by the Trader. Schema validation guarantees output shape; the risk layer guarantees safety. Vectors are free to express disposition within those bounds.

### 2.1 Full system diagram

Renders inline on GitHub. Standalone source: `architecture-diagram.mermaid`. Yellow blocks indicate where control vectors are active; blue is deterministic rule code; green is external services; purple is storage; orange is orchestrator + tool-surface code; pink is on-chain ERC-8004 registries on Mantle; cyan is eval. Red dashed remains reserved for future v2-deferred nodes.

```mermaid
flowchart TD
    A1[Alpaca OHLCV<br/>price + volume]
    A2[Exchange APIs<br/>funding rate, OI]
    A3[Nansen<br/>smart-money flows]

    IND[Technical Indicators<br/>RSI · MA · Bollinger<br/>MACD · Donchian · ATR]

    HA[<b>Ops</b><br/>pipeline orchestrator<br/>state assembly · scheduling]

    S1[<b>Stage 1 · Intern</b><br/>OpenAI- / Anthropic-compat<br/>or ACPX agent harness<br/>━━━━━━━━━━━━<br/>bull · bear · flat cases<br/>evidence inventory · regime<br/>━━━━━━━━━━━━<br/>NO candidate decision<br/>NO vectors]

    MCP[<b>xvn-mcp</b><br/>stdio MCP server · stateless<br/>━━━━━━━━━━━━<br/>indicator tools<br/>rsi · sma · ema · macd<br/>bollinger · atr · donchian<br/>fib_retracements · health<br/>━━━━━━━━━━━━<br/>active when INTERN=acpx]

    S2[<b>Stage 2 · Trader</b><br/>Qwen3-32B local quantized<br/>━━━━━━━━━━━━<br/>action · size · direction<br/>stop · take-profit<br/>━━━━━━━━━━━━<br/>VECTORS ACTIVE]

    CV[<b>Control Vectors</b><br/>━━━━━━━━━━━━<br/>conviction (v1 active)<br/>patience · risk · trend<br/>(extracted, v2 active)<br/>━━━━━━━━━━━━<br/>confidence-gated · entropy v1]

    R[<b>Risk Layer</b><br/>deterministic rules · no LLM<br/>━━━━━━━━━━━━<br/>position limits<br/>daily-loss circuit breaker<br/>correlation cluster cap<br/>asset whitelist]

    S3[<b>Stage 3 · Execution</b><br/>idempotent tool calls<br/>━━━━━━━━━━━━<br/>NO vectors]
    AP[Alpaca Paper<br/>bracket orders<br/>v1 testing path]
    OR[Orderly Network<br/>perpetual futures · Mantle<br/>orderly-connector-rs<br/>v1 hackathon executor]

    subgraph ERC8004 [ERC-8004 · Mantle]
        ID[Identity Registry<br/>agent NFT<br/>+ vector_manifest_cid]
        RP[Reputation Registry<br/>per-run feedback<br/>Δ-Sharpe + manifest hash]
        VL[Validation Registry<br/>per-trade stance proof<br/>active alphas + result hash]
    end

    DB[(SQLite<br/>decisions · briefings<br/>market state<br/>vectors_enabled flag)]

    M[<b>Metrics · Eval</b><br/>━━━━━━━━━━━━<br/>Δ-Sharpe primary<br/>max drawdown<br/>profit factor · win rate<br/>decision divergence rate<br/>━━━━━━━━━━━━<br/>paired bootstrap 95% CI]

    BL[Baselines<br/>━━━━━━━━━━━━<br/>buy-hold · random<br/>RSI · MA-cross · Bollinger<br/>MACD · Donchian · Fibs<br/>smart-money copy<br/>funding-rate fader<br/>━━━━━━━━━━━━<br/>vectors-OFF · vectors-RANDOM<br/>vectors-ORTHOGONAL]

    A1 --> IND
    A1 --> HA
    A2 --> HA
    A3 --> HA
    IND --> HA

    HA --> S1
    S1 -.->|tool calls<br/>when INTERN=acpx| MCP
    MCP -.->|computes at agent-supplied<br/>parameters| IND
    S1 -->|JSON: InternBriefing<br/>neutral evidence only| S2
    CV -.->|injected at<br/>mid-late layers| S2
    S2 -->|JSON: TraderDecision| R

    R -->|approved or modified| S3
    R -.->|vetoed| DB

    S3 --> AP
    S3 --> OR
    S3 -.->|after closed trade| VL

    HA -.->|once at agent mint| ID
    M -.->|after each run| RP

    S1 -.-> DB
    S2 -.-> DB
    R -.-> DB
    S3 -.-> DB

    DB --> M
    BL --> M

    classDef vectorOn fill:#fef3c7,stroke:#d97706,stroke-width:2px,color:#000
    classDef deterministic fill:#dbeafe,stroke:#2563eb,stroke-width:2px,color:#000
    classDef storage fill:#f3e8ff,stroke:#7c3aed,color:#000
    classDef external fill:#dcfce7,stroke:#16a34a,color:#000
    classDef orchestrator fill:#ffedd5,stroke:#ea580c,stroke-width:2px,color:#000
    classDef eval fill:#cffafe,stroke:#0891b2,color:#000
    classDef onchain fill:#fce7f3,stroke:#db2777,stroke-width:2px,color:#000

    class S2,CV vectorOn
    class R deterministic
    class DB storage
    class A1,A2,A3,AP,OR external
    class HA,MCP orchestrator
    class M,BL eval
    class ID,RP,VL onchain
```

---

## 3. Stage 1 — Intern

**Purpose:** Produce a structured, neutral evidence briefing. The Intern researches; it does not recommend. The output is symmetric by construction so the Trader's vectors get clean steering room.

**Model choice:** Backend-agnostic — picked at runtime via config (`provider`, `base_url`, `model`, `api_key_env`). Three backends behind one `InternBackend` trait:
- **OpenAI-compatible HTTP** (default for non-Anthropic models). One implementation covers OpenAI, OpenRouter, Together, Groq, DeepSeek, xAI, Mistral, plus any self-hosted server speaking the Chat Completions wire format — vLLM, Ollama (`/v1`), LM Studio, llama.cpp, TGI. Swap models or providers by editing config; no recompile.
- **Anthropic Messages API.** Used for Claude models (`claude-haiku-4-5` for speed, `claude-sonnet-4-6` for higher-quality analysis) and any Anthropic-API-compatible gateway. Called via `anthropic-sdk` or raw `reqwest`.
- **Local candle (optional, deferred).** Direct in-process inference via `candle` for fully air-gapped runs without an HTTP hop. Lower priority than the HTTP path because OpenAI-compat against a localhost vLLM/Ollama gives the same air-gap property with vastly more model coverage.

Reasoning models (o-series, DeepSeek-R1, Qwen-thinking, gpt-oss reasoning) are first-class — the backend strips provider-native reasoning fields and inline `<think>` blocks before JSON validation, and forwards `reasoning_effort` when supported.

**Input:** Market state object containing technical indicators (RSI, MAs, Bollinger, ATR, recent OHLCV), onchain signals (Nansen smart money flows, funding rate, exchange flows for the asset), and current portfolio state (open positions, unrealized P&L, available capital).

No news, no fundamentals (out of scope by user decision).

**Output (JSON):**

```json
{
  "setup_id": "uuid",
  "asset": "BTC-PERP",
  "bull_case": "strongest argument for going long",
  "bear_case": "strongest argument for going short",
  "flat_case": "strongest argument for sitting this one out",
  "evidence_long": ["rsi_oversold", "smart_money_inflow", "funding_rate_neg"],
  "evidence_short": ["volume_declining", "lower_high_lower_low"],
  "evidence_flat": ["chop_in_5pct_range_3d", "low_signal_quality"],
  "regime": "trending | choppy | high_vol | low_vol",
  "signal_quality": 0.62,
  "horizon_hours": 4
}
```

The Intern's prompt explicitly instructs: *"Present balanced cases on all three sides. Do not recommend an action. Your job ends with the briefing — the Trader will decide."* No `candidate_direction` field, no `candidate_size_bps`. Those would commit the decision before vectors get to express disposition.

`signal_quality` is the analyst's estimate of *how clean the setup is* — a quality signal, not a directional signal. It feeds into the confidence-gating mechanism (§7.3), where low-quality setups dampen vector magnitude so vectors don't push the model into confidently-wrong territory on noisy inputs.

`regime` drives the choice of disposition weights at the Trader (regime-conditioned vector configuration, §7.4) and is itself directionally neutral — knowing the market is "choppy" doesn't tell you which way it'll resolve.

This object is the contract between Intern and Trader. It is validated by `serde` + `garde` (Rust) before handoff — schema violations produce a typed error rather than a silently malformed briefing.

---

## 4. Stage 2 — Trader

**Purpose:** Make the final trading decision, shaped by the agent's current dispositional state via active control vectors. This is where the experiment lives.

**Naming:** "Trader" replaces earlier candidates ("Stance," "Decision Agent"). The role is characterological — this model carries the disposition. The Intern hands it neutral evidence; the Trader decides.

**Model choice:** Qwen3.6-27B (no-thinking mode) is the primary model. Precision floats between **4-bit and 16-bit depending on available resources** — local M-series dev runs at 4-bit (Q4_K_M) for velocity; rented-GPU headline runs step up to 8-bit or 16-bit when memory permits, since heavier quantization adds noise to hidden states and may degrade vector effects unpredictably. No-thinking mode is mandatory — chain-of-thought tokens before the decision dilute the vector's influence at the action choice point and inflate latency without buying signal. **A one-day spike (Phase 0, Task 2) validates vector behavior on toy axes before committing the architecture to a specific precision.**

**Inference path:**
1. Receive Intern Briefing JSON.
2. Render the briefing as a prompt that requests a structured decision. The prompt presents bull/bear/flat cases in parallel structure with no anchored recommendation.
3. Run forward pass via `candle` with steering hooks injected at selected layers (mid-to-late, per SEAL and Mitra findings). The hook receives the residual stream at layer N and returns `residual + Σ alpha_i * vector_i`. Different vectors can apply at different layers (Weij et al.) with confidence gating modulating each magnitude.
4. Parse output as JSON via `serde_json` with `garde` validation; on parse failure, retry once with a corrective system message before falling back to a parse-error path.

`candle` exposes the hidden-state hooks needed for fine-grained steering — strictly more flexible than the static `--control-vector` path llama.cpp exposes. CAST-style projection-based gating, PID-controlled alpha, and probe-gated firing all live naturally in this hook.

**Output (JSON):**

```json
{
  "setup_id": "uuid",
  "action": "buy | sell | flat | close",
  "size_bps": 75,
  "direction": "long | short | flat",
  "stop_loss_pct": 2.5,
  "take_profit_pct": 5.0,
  "trader_summary": "string — one-line dispositional rationale",
  "active_vectors": {"conviction": 0.8, "patience": -0.3, "risk_appetite": 0.5}
}
```

`active_vectors` is logged for offline analysis — it records which dispositional axes were applied and at what magnitude during this decision.

**Vectors-off mode:** The same code path runs with all vector magnitudes set to 0. This is the experimental control. A single config flag toggles it.

---

## 5. Risk Layer

**Purpose:** Deterministic safety net between Stage 2 and Stage 3. No LLM, no vectors. Pure rule evaluation.

The risk layer either passes the decision through unchanged, modifies sizing downward, or vetoes the decision entirely. It never increases size or flips direction.

**Rules (initial set):**
- **Max position size:** No single position larger than 20% of portfolio NAV.
- **Max total exposure:** Sum of absolute position sizes ≤ 100% of NAV (no leverage in v1; perps come later).
- **Asset whitelist:** Only assets in `config/whitelist.yaml` are tradeable.
- **Daily loss circuit breaker:** If realized + unrealized loss for the day exceeds 5% of starting NAV, all new entries are vetoed until rollover.
- **Max open positions:** ≤ 5 concurrent positions.
- **Correlation cap:** No more than two positions in the same correlation cluster (BTC-cluster, ETH-cluster, SOL-cluster).
- **Stop loss required:** Every entry must specify a stop loss; reject decisions that omit it.

**Output:** `RiskDecision { approved: bool, original: Decision, modified: Decision | None, veto_reason: str | None }`

The risk layer logs every veto with reason. Vetoes are valuable signal — they tell us when vectors push the agent into regions a human risk manager would also reject.

---

## 6. Stage 3 — Execution

**Purpose:** Translate approved decisions into Alpaca paper trading API calls. No model in the loop.

**Library:** `apca` (mature Alpaca client on crates.io; `alpaca-rs` is a 0.1.0 stub). Fall back to a thin `reqwest`-based wrapper if `apca` is missing endpoints we need — Alpaca's REST/WS surface is small.

**Operations supported:**
- Submit market order (entry).
- Submit bracket order (entry + stop + take-profit).
- Close position.
- Query portfolio state.

**Idempotency:** Each decision carries a `setup_id` used as client order ID to prevent duplicate execution if Stage 3 is retried.

**State sync:** Portfolio state is read from Alpaca after every action and cached for the next Stage 1 input.

**Two execution paths run in parallel for v1 (Mantle Turing Test hackathon):**
- **Alpaca paper** is the pre-launch testing path. Validates Stage 1→2→3 plumbing against a battle-tested broker simulator before any on-chain capital is touched.
- **Orderly Network on Mantle** is the hackathon submission path. Orderly is shared-orderbook infrastructure that 340+ brokers (FusionX, Ranger, Aark, Ascendex, Kai, …) front-end onto; trades execute against Mantle vault `0x816f722424B49Cf1275cc86DA9840Fbd5a6167e9` (chain_id 5000). Capital is pre-funded on Mantle by the user; the agent never bridges. The integration is native Rust via `orderly-connector-rs = "0.4"` — no Node.js dependency, no subprocess shellout.

A `--executor {alpaca,orderly}` flag selects between them at runtime. The Orderly executor (`crates/xianvec-execution/orderly.rs`) holds an `OrderlyService` instance plus signed-request `Credentials` and surfaces the SDK's `create_order` / `cancel_order` / `get_holding` / `get_positions` methods through the `Executor` trait.

> **Venue history (2026-05-03).** Three iterations in one day:
> 1. Earliest drafts named "Byreal Perps on Mantle" — wrong on its face: Byreal CLMM is Solana, Byreal Perps CLI is Hyperliquid, the "Byreal-on-Mantle" association is a Mantle Super Portal bridge into Byreal's *Solana* liquidity.
> 2. Pivoted to Vertex Protocol; M0 found Vertex operationally dead (all gateways 404, repos ~1 year stale).
> 3. Fell back to Byreal Perps CLI on Hyperliquid (M0 passed, committed at `1703b71`); then discovered Orderly via FusionX's `fusionx_pro` broker_id. M0' for Orderly also passed, and Orderly's Mantle-native + Rust-native integration shape strictly dominates the cross-chain Byreal CLI path.
>
> The hackathon Path 1 ("DeFi Deep Dive") names *Byreal Agent Skills / Byreal Perps CLI / RealClaw* as winning tooling. v1 keeps **Byreal Agent Skills** vendored as the Stage 1 Intern's skill catalog (M4) so the Path 1 endorsement is satisfied via context, not execution. The Byreal Perps CLI path is preserved as a verified fork option — see `decisions/0006-executor-choice.md` and `probes/m0-byreal/`. Earlier-considered Mantle perps alternates (KTX.Finance — DNS gone; TsunamiX — app/docs NXDOMAIN; IntentX — alive but Base-leaning) are off the table until Mantle's perps ecosystem matures.

---

### 6.1 ERC-8004 — On-chain agent identity and stance provenance

ERC-8004 (deployed on Ethereum mainnet and Mantle mainnet, January–February 2026) defines three lightweight on-chain registries for autonomous agents: **Identity**, **Reputation**, and **Validation**. All three are load-bearing for xianvec's Mantle submission, and each maps cleanly onto the control-vector architecture.

**Identity Registry (ERC-721 agent NFT).** The agent is minted as an NFT at first run. The token's metadata includes a `vector_manifest_cid` — an IPFS/Arweave content hash of the full vector manifest (`model_version`, `layer_id`, `contrast_pair_set_hash`, `alpha_curve_hash`, `derivation_timestamp`). The manifest CID is 32–64 bytes on-chain; the full manifest file lives off-chain. This is exactly the ERC-721 metadata pattern. The NFT is the agent's permanent character definition: swapping the manifest hash means the agent has changed its dispositional configuration and starts a fresh reputation trace for that stance.

**Reputation Registry.** Feedback records accrue to the agent NFT after every closed experiment run — the vectors-on and vectors-off agents each receive a reputation entry recording their Δ-Sharpe, regime context, and manifest hash. Critically, reputation attaches to a specific manifest hash, not just an address. Two runs with the same manifest compound the same reputation. A new extraction run (new contrast pairs, new model version) starts fresh. This makes stance configurations composable trust primitives: well-performing vector configs can be forked and their reputation partially inherited.

**Validation Registry.** This is the "prove it" layer and the most important for xianvec's core claim. After every closed Orderly/Mantle trade, Stage 3 constructs and submits a validation proof to the Mantle Validation Registry — same chain as the trade, single-chain audit trail:

```json
{
  "setup_id": "uuid",
  "action": "buy | sell | flat | close",
  "active_vector_alphas": { "conviction": 0.8 },
  "vector_manifest_hash": "0x...",
  "vectors_enabled": true,
  "trade_result_hash": "keccak256(closed_pnl | timestamp | price)",
  "run_id": "uuid"
}
```

`active_vector_alphas` is one float in v1 (Conviction only) = 4 bytes; the schema accepts up to four when post-v1 work activates more axes. `vector_manifest_hash` is 32 bytes. The proof is cheap to post and gives anyone the ability to verify on-chain that a specific geometrically-defined stance produced a specific trade. The vectors-off control posts the same structure with `vectors_enabled: false` and an empty alpha map — the comparison is publicly auditable without trusting the operator's reporting.

**Why this matters for the thesis.** Most on-chain agent identity is retrospective: address + transaction history. The Validation proof is prospective — the stance is committed at inference time, embedded in the geometry that produced the decision, and recorded before the outcome is known. The trade is the *output* of the stance, not the definition of it. The on-chain record proves the causal chain: this manifested disposition → this decision → this outcome.

**On-chain footprint summary:**

| Artifact | Size | Location | When |
|---|---|---|---|
| Full control vectors (4 axes, fp32) | 80–640 KB | IPFS / Arweave | Once per vector extraction run |
| Vector manifest (JSON sidecar) | ~500 bytes | IPFS / Arweave | Once per extraction run |
| `vector_manifest_cid` in agent NFT metadata | 32–64 bytes | Mantle (Identity Registry) | Once at agent mint |
| `active_vector_alphas` + manifest hash per trade | ~48 bytes | Mantle (Validation Registry) | After every closed position |
| Reputation entry per experiment run | ~64 bytes | Mantle (Reputation Registry) | After each backtest / paper run |

The full vectors never live on-chain — EVM storage at 20K gas per 32-byte slot makes 80KB prohibitively expensive even on Mantle. The on-chain artifacts are hashes, commitments, and the tiny per-trade alpha configuration. This is not a compromise: the alpha config per trade is more informative than the raw vectors, because it records which magnitudes were actually active at decision time under confidence gating and regime conditioning.

**Implementation.** `xianvec-execution` constructs the Validation proof after each closed Orderly position on Mantle and submits via `alloy`. ERC-8004 contract addresses (Identity, Reputation, Validation registries on Mantle mainnet) live in `config/mantle.toml` alongside the Orderly vault address. The agent NFT is minted once during initial setup via `xvn setup --mint-agent-nft`; subsequent runs only post to Reputation and Validation. Trades, identity, and reputation all live on the same chain — no cross-chain handoff in the audit trail.

---

## 8. Data pipeline

**Sources:**
- **Price/OHLCV:** Alpaca data API (free with paper account).
- **Technicals:** Computed locally via `pandas-ta` from OHLCV.
- **Onchain / smart money:** Nansen API ($49/month plan).
- **Funding rates / open interest:** Direct from exchange APIs (Binance, Bybit) — public endpoints, no auth needed.

**Cadence:** Pull every 15 minutes during active sessions for v1. Higher-frequency loops are post-hackathon.

**Caching:** All raw data is logged to local SQLite for reproducibility of backtests. Stage 1 and Stage 2 inputs/outputs are persisted with timestamps so any decision can be replayed.

---

## 9. Eval framework

The eval framework is the most important non-obvious piece of this project. Without it, vector improvements cannot be measured and the Karpathy loop has nothing to learn from.

### 9.1 Backtest harness

Replays historical setups through the full Stage 1 → Stage 2 → Risk → Stage 3 pipeline against historical price data. Stage 3 in backtest mode hits a simulated execution engine instead of Alpaca. Slippage and fee assumptions are configurable.

**Why this matters more than forward paper trading:** 500 backtested setups in an evening yields more statistical signal than 500 forward paper trades over weeks. Per-trade noise is brutal; you need population statistics to evaluate vector configurations.

### 9.2 Metrics — pre-committed

These are the metrics the hackathon demo will report. Picked now, before any results are run, so we can't backfit:

**Primary metric (the headline number):**
> **Sharpe ratio delta (Δ-Sharpe):** annualized Sharpe with vectors ON minus annualized Sharpe with vectors OFF, evaluated on the same set of setups, paired.

This isolates the vector contribution. It is the single number the demo lives or dies on.

**Secondary metrics (the dashboard):**
- **Max drawdown** (peak-to-trough loss, %): Risk profile. Must not be catastrophic for either condition.
- **Profit factor** (gross wins / gross losses): Intuitive, demo-friendly.
- **Win rate** (% of trades profitable): Caveat that high win rate with bad profit factor is a warning sign.
- **Decision divergence rate** (% of setups where vectors-on and vectors-off produced different actions): Confirms that vectors are actually changing behavior, not just nudging within the same decision.

**Statistical significance:**
- Minimum 30 paired trades for any signal interpretation.
- Target 100+ paired trades for hackathon demo.
- Report 95% confidence interval on Δ-Sharpe via paired bootstrap (10k resamples).

**Anti-overfitting gate (hard requirement):**
No vector configuration advances to paper trading unless it shows positive Δ-Sharpe in at least one pre-2023 bear regime *and* at least one 2023–2024 bull regime. A configuration that only beats vectors-OFF in trending markets is not evidence — it is a backtest artefact. This gate is explicit and checked programmatically before any paper-trading run is authorized. Single-regime wins, however large, are capped: a result that does not span at least two distinct regime types cannot be reported as a positive finding. Rationale: NexusTrade's $676 hill-climbing experiment showed exactly this failure mode — a rubric that rewarded peak-year returns drove the agent from a 71/100 Iron Condor (survived 2022 bear, 54% avg) to a 27/100 directional disaster (-6.3% avg, 92% drawdown) by Round 5, following evaluator feedback faithfully into a single-regime optimum.

### 9.3 Baselines

Beyond the critical vectors-on vs vectors-off comparison, the agent must beat external baselines to demonstrate edge.

**Null baselines (must beat):**
- Buy-and-hold the asset basket from t=0.
- Random direction, constant 1% sizing, same trade frequency.
- Always-long, always-short.

**Classical technical baselines:**
- RSI 14 with 30/70 thresholds, mean-reversion entries.
- MA crossover 30/90 (golden/death cross).
- MA triple-confirmation 30/60/90 (all three must align).
- Bollinger Bands 20/2 mean-reversion at the bands.
- MACD 12/26/9 momentum.
- Donchian 20-day breakout (Turtle baseline — surprisingly tough).
- Fibonacci retracements at 38.2/50/61.8 with swing detection via rolling-window peak finder.

**Onchain baselines (the real bar):**
- Nansen smart-money copy-trading: follow whale flows directly, no model.
- Funding rate fader: at funding-rate extremes, fade the crowd.
- Stablecoin exchange-inflow: large USDT/USDC moves to exchanges → reduce risk.
- Liquidation cascade fader: after large liquidation events, mean-revert.

**ML baseline (stretch):**
- XGBoost on technical + onchain features. Often surprisingly hard to beat.

**Experimental controls (the thesis-defining comparisons):**
- Same agent, vectors **OFF**: the critical control.
- Same agent, vectors **random** at same magnitude: controls for "any perturbation activates exploration."
- Same agent, vectors **orthogonal** to disposition axes: controls for representation impact vs direction-specific impact.

### 9.4 Structured traces (flight recorder)

Every Stage 1 and Stage 2 call produces a structured trace record persisted to SQLite alongside the briefing and decision. Without traces, a vector configuration that underperforms in backtest is a black box; with traces, the exact iteration where behaviour diverged is pinpointable.

**Minimum trace fields per call:**
- `run_id`, `setup_id`, `stage` (intern | trader)
- `model` and `vectors_enabled` flag + active magnitudes
- Full input (system prompt + user content + injected vector config)
- Raw model output (full JSON string, pre-parse)
- Parse success / validation errors
- Token count (prompt + completion) and latency (ms)
- Any exception with traceback

**Storage:** `traces` table in the existing SQLite store. Schema mirrors the existing `decisions` table structure; keyed on `(run_id, setup_id, stage)`.

**Why this is pre-Phase-8:** Traces must exist before any evaluation loop runs. An eval loop without traces cannot distinguish "the vector configuration was wrong" from "the prompt was wrong" from "the model produced a parse error and fell back." Traces are the diagnostic layer that makes every other eval result interpretable.

### 9.5 Forward paper trading

Forward Alpaca paper trading runs continuously after the backtest establishes baseline. It is deployment validation, not primary eval. The agent runs both vectors-on and vectors-off in parallel (alternating setups, or running two instances) so live paper trading produces paired data.

---

## 10. Tech stack

The runtime is Rust. The vector-extraction toolchain is Python, invoked offline as a subprocess. Python is a build tool, not a runtime dependency — the production binary has no Python in its process tree.

**Runtime (Rust):**
- Rust stable (current MSRV pinned in `rust-toolchain.toml`)
- Cargo workspace with one crate per architectural concern (see §10.1)
- macOS Apple Silicon (Metal) primary; Linux/CUDA for cloud runs

**Inference:**
- `candle` — HuggingFace's Rust ML framework, supports Qwen-3 with Q4/Q5 quantization, Metal and CUDA backends, and (critically) hidden-state hooks for steering injection
- `llama-cpp-rs` — fallback if candle's Qwen-3 quantization story has rough edges in practice; less flexible for fine-grained steering but well-tested
- `reqwest` — Stage 1 HTTP backend (covers OpenAI- and Anthropic-compatible endpoints uniformly); `anthropic-sdk` and `async-openai` are optional ergonomic wrappers

**Control vectors:**
- *Extraction (offline, Python):* `repeng` + `transformers` + `torch` in `tools/extract_vectors/`, invoked via subprocess
- *Storage:* FAISS-compatible `.index` files via `faiss-rs`, with contract manifest sidecars
- *Application:* candle hidden-state hooks in `crates/xianvec-inference/`
- *Gating:* `crates/xianvec-gating/` — entropy gate v1; CAST projection-based gating and PID-controlled alpha are deferred to v2

**Trading:**
- `apca` for Stage 3 Alpaca paper (`alpaca-rs` on crates.io is a stub)
- `orderly-connector-rs = "0.4"` for Stage 3 Mantle execution (native Rust async; verified by `probes/m0-orderly/`). Trades land on Mantle's Orderly vault `0x816f722424B49Cf1275cc86DA9840Fbd5a6167e9`
- `alloy` for direct Mantle / EVM interactions (ERC-8004 identity NFT mint + reputation/validation registry posts; same chain as the trades, no bridge)
- `ta` crate (or hand-rolled in `polars`) for technical indicators
- Custom thin clients for Nansen and exchange APIs via `reqwest`

**Data/eval:**
- `sqlx` (compile-time-checked queries) on SQLite for persistence
- `polars` for tabular data manipulation (faster and more ergonomic than pandas at our scale)
- `ndarray` for numerical work where polars isn't the right shape
- Eval results emit structured JSON; plotting via a small Python notebook (`notebooks/eval_plots.py`) consuming those JSONs — pragmatic concession, plotting is the one place Python remains genuinely better

**App layer:**
- `serde` + `garde` (or `validator`) for typed schema enforcement on stage handoffs — contract violations become compile errors where possible, runtime errors elsewhere
- `clap` for CLI
- `tracing` for structured logging (also drives observability — see telemetry block)
- `teloxide` for the Telegram demo bot

**Vector substrate & geometry:**
- `faiss-rs` for FAISS-compatible HNSW indexes
- `tokio` + `arc-swap` for async vector storage with snapshot reads
- `serde_json` for contract manifests and FAISS sidecars

**Introspection (opt-in, per §7.5.1):**
- `xianvec-introspect` crate — composes via the `LayerHook` trait, zero overhead when not installed
- Captures per-layer residual norms, activation diffs, vector–residual cosines, logit lens at every hooked layer, decision-token logits/probabilities/entropy at the gate point
- Output: structured JSON consumed by `notebooks/inspect_vector.py` for multi-panel plots

**Tracing & observability:**
- `tracing` + `tracing-subscriber` for structured spans
- `tracing-opentelemetry` + `opentelemetry-otlp` for OTLP export
- Self-hosted Langfuse as primary backend (Docker compose: Postgres + Clickhouse)
- OpenTelemetry GenAI semantic conventions throughout
- Dual-write: SQLite (§9.4 flight recorder) for replay; OTel for live observability
- Python extractor emits OTel spans via `opentelemetry-python` so subprocess invocations join the trace tree

**Dev:**
- `cargo test` + `proptest` for unit and property-based tests
- `criterion` for benchmarks (gating hot path, especially)
- `clippy` for lint
- `cargo fmt` for formatting
- `cargo deny` for license/CVE auditing
- `pre-commit` hooks calling `cargo fmt --check`, `cargo clippy --all-targets`, `cargo test`

**Secrets:** `op` (1Password CLI) per workspace convention. Never hardcode keys.

### 10.1 Cargo workspace layout

**v1 scope (2026-05-03):** the workspace is a **single `crates/xianvec-*` tree**. The lodestar / xianvec subtree split documented in earlier drafts is deferred to v2 — see §10.2 for the lift trigger. The implementation-plan.md "v1 scope cuts" block lists the full set of items (multi-axis disposition, multi-asset basket, full contract-layer crate, geometry crate, async substrate crate, telemetry crate + OTel/Langfuse, Telegram bot, xStocks, mantle-risk-evaluator) that move to v2 with this collapse.

```
xianvec/
├── Cargo.toml                    # workspace root
├── rust-toolchain.toml
│
├── crates/
│   ├── xianvec-core/             # types, schemas, config, SQLite persistence, manifest types
│   ├── xianvec-data/             # OHLCV ingest, indicators, onchain signals
│   ├── xianvec-inference/        # candle wrapper + steering hooks + inline FAISS load
│   ├── xianvec-gating/           # entropy gating, alpha schedule
│   ├── xianvec-introspect/       # OPTIONAL layer analytics (Phase 0.3 spike requires)
│   ├── xianvec-intern/           # Stage 1 (OpenAI- or Anthropic-compatible HTTP, optional local candle)
│   ├── xianvec-trader/           # Stage 2 (vectors active)
│   ├── xianvec-risk/             # deterministic risk layer
│   ├── xianvec-execution/        # Stage 3: Alpaca + Orderly
│   ├── xianvec-eval/             # backtest harness, baselines, Δ-Sharpe
│   ├── xianvec-harness/          # boundary probes (minimal v1 corpus)
│   └── xianvec-cli/              # clap-based CLI; installed binary is `xvn`
│
├── tools/
│   └── extract_vectors/          # Python: repeng-based contrast extractor
├── config/                       # TOML configs (whitelist, risk)
├── data/
│   ├── probes/                   # boundary probe corpus (minimal v1, versioned)
│   └── vectors/                  # FAISS .index files + manifests
├── notebooks/                    # eval plotting (Python, offline)
└── docs/
```

The workspace structure still makes the contract layer load-bearing: each crate's public API is a typed surface, and cross-crate calls fail to compile if the contract doesn't match. The discipline that motivated the Rust choice carries over even without the formal lodestar boundary — a `xianvec-data` function still cannot reach into `xianvec-gating`'s internals.

### 10.2 The lodestar / xianvec boundary (deferred to v2)

The original design extracts a domain-agnostic `crates/lodestar/` subtree (inference, vector substrate, geometry, gating, introspection, telemetry, CLI) so it can be forklifted into a sibling project (EditEngage, character/voice work, any other domain) without modification. **v1 collapses this into the single `xianvec-*` tree above** to compress workspace overhead during the 45-day hackathon window. The discipline survives — domain logic still does not reach into substrate internals — but the boundary is convention, not Cargo-enforced.

**Lift trigger:** a second domain consumer materializes, OR v1 ships with a positive headline Δ-Sharpe and there is a 2-week refactor window. The mechanical lift is `git mv crates/{xianvec-inference,xianvec-substrate,xianvec-contracts,xianvec-geometry,xianvec-gating,xianvec-introspect,xianvec-telemetry} crates/lodestar/lodestar-{...}` plus a path-to-git swap in `Cargo.toml`. Cost is small precisely because the v1 single-tree still respects the substrate-vs-domain split at the function-import level.

**What the lodestar surface will provide post-lift** (preserved for forward planning, since its existence shapes which v1 modules to keep clean):

- Load a model via candle (with pluggable backend trait for llama-cpp-rs)
- Async FAISS-compatible vector storage with snapshot reads, contract validation, priority queuing
- Steering hooks gated by entropy / CAST / PID-alpha
- Generic geometry primitives (Mint, Corridor, Probe) parametrized over domain types
- Optional layer introspection
- OpenTelemetry span schema (also v2)
- A generic `lodestar inspect-vectors` CLI

**A note on naming:** "lodestar" is the working name. If a different name lands better at extraction time (`polaris`, `prism`, etc.), rename then; the structural intent doesn't depend on the name.

---

## 11. Out of scope (deferred)

Explicit non-goals for hackathon. Each is a real follow-on but not v1:

- Karpathy self-improvement loop (vector training from agent's own trades)
- **Capital bridge** (`@mantleio/sdk` ETH↔Mantle): explicitly out of scope. Funds are pre-positioned on Mantle by the user; the agent only ever sees on-Mantle balances and never executes a bridge transaction itself.
- Options Greeks, derivatives strategy
- Multi-model evaluation tournament
- **Cross-run memory system (MemPalace):** Deferred until the vector hypothesis is validated — injecting memory into runs conflates two variables.
- Dashboard with historical data UI
- Telegram interactive command set beyond demo-supporting commands
- News, fundamentals, sentiment from social
- Auto-scaling / cloud deployment beyond a single Vast.ai/RunPod box for backtest acceleration

**v1 scope cuts (added 2026-05-03):** the items below appeared in earlier drafts as v1 commitments. Each is now deferred with an explicit re-add trigger documented in implementation-plan.md → "Future additions / Scope items cut from v1":

- lodestar / xianvec subtree split + `cargo deny` boundary (§10.2)
- 3 of 4 disposition axes active — v1 ships **Conviction only**
- Regime-conditioned vector configs (§7.4 hand-set magnitudes per regime)
- Multi-asset basket — v1 is BTC only
- xStocks / Mantle tokenized equities
- Async vector substrate as a separate crate (worker pool, snapshot reads, priority queue)
- Full contract layer crate with `Vector<L, M>` generics
- Geometry crate with first-class corridor abstractions
- Telemetry crate + OpenTelemetry export + self-hosted Langfuse
- Telegram demo bot
- `mantle-risk-evaluator` LLM pre-flight gate

**Note on previously-deferred items still in v1:** ERC-8004 identity + reputation + validation registries are v1-required, all on Mantle. On-chain trade execution runs on Mantle via **Orderly Network** (`orderly-connector-rs`, native Rust). **Byreal Agent Skills** stays vendored as the Stage 1 Intern's skill catalog, satisfying the hackathon Path 1 endorsement of Byreal tooling without forcing the trade venue. The Byreal Perps CLI executor path is preserved as a verified fork option (see `decisions/0006-executor-choice.md`). See §6 (Stage 3) and implementation-plan.md → "Mantle hackathon integration."

---

## 12. Open architectural questions resolved

For the record, the following were debated and decided:

| Question | Resolution |
|---|---|
| Stage 2 as decider vs calibrator? | **Decider.** User chose to maximize the experimental signal of vector influence. Risk layer compensates for safety. |
| Stage 2 name? | **Trader** (paired with Stage 1 = **Intern**). Characterological roles: Intern researches neutrally, Trader decides with disposition. |
| Does Intern recommend a candidate decision? | **No.** Intern emits balanced bull/bear/flat cases with parallel evidence inventories. Recommending would prompt-anchor the Trader and drown the vectors. |
| Local model for Stage 2? | **Qwen3.6-27B (no-thinking)** primary, precision 4-bit→16-bit depending on resources. Validated by toy-axis spike before lock-in. |
| Confidence gating? | **Yes**, via decision-token entropy. Lightweight stand-in for SVF. |
| Where does risk live? | **Between Stage 2 and Stage 3** as deterministic rule code. |
| Primary eval metric? | **Δ-Sharpe** (vectors-on minus vectors-off, paired). |
| Backtest or forward paper? | **Backtest first** for population statistics; forward paper for deployment validation. |
| Implementation language? | **Rust from day one** for the runtime. `candle` for inference (with `llama-cpp-rs` fallback). Python retained only as an offline build tool for vector extraction (`tools/extract_vectors/`). No runtime Python. See §10. |
| Vector extraction language? | **Python**, offline. `repeng` + `transformers` is the well-trodden path with no Rust equivalent worth the rewrite cost during v1. Invoked via subprocess from the Rust orchestrator. The Karpathy self-improvement loop calls the same utility — to the agent, vector extraction is a tool that produces a file. See §7.2. |
| Inference framework? | **`candle`**, primary. Provides hidden-state hooks for fine-grained steering (different vectors at different layers, CAST projection gating, PID alpha) that llama.cpp's static `--control-vector` API cannot express. `llama-cpp-rs` retained as fallback if candle's Qwen-3 quantization is rough in Phase 0 validation. |
| Vector file format? | **FAISS-compatible `.index`** with contract manifest sidecars. Both languages read/write the same format; this is the boundary between offline Python tooling and Rust runtime. |
| Telemetry backend (v2)? | **Self-hosted Langfuse** as primary, OpenTelemetry GenAI conventions throughout. **v1 ships SQLite flight recorder + `tracing` console only**; full OTel/Langfuse deferred to v2. See §7.6 and implementation-plan.md "Telemetry (v1)". |
| Adopt Glamin directly? | **No, adopt the patterns.** Corridors, contract layer, boundary probes, document/geometry separation, async-first storage, FAISS compatibility — rebuilt in Rust. Leave Fortran/C, hand-tuned SIMD, the YAML DSL, and the unfinished geometric-logic layer. See §7.5. |
| Reusable across projects? | **Yes, but deferred to v2.** Lodestar / xianvec subtree split was the design but is collapsed into a single `crates/xianvec-*` tree for the 45-day hackathon window. The mechanical lift (`git mv`) costs a few hours and triggers when a second domain consumer materializes or when v1 ships. See §10.2. |
| On-chain executor? | **Orderly Network on Mantle** via `orderly-connector-rs = "0.4"` (native Rust async). Decision rationale and the day's three-pivot history live in `decisions/0006-executor-choice.md`. Byreal Agent Skills stay vendored as the Stage 1 Intern's skill catalog so Path 1's named-tooling endorsement is satisfied through context, not execution. The Byreal Perps CLI path (Hyperliquid execution) is preserved as a fork option — M0 probe at `probes/m0-byreal/` passed. Vertex Protocol was eliminated on 2026-05-03 morning (operationally dead — gateways 404, repos ~1 year stale). See §6. |
| Active disposition axes in v1? | **One — Conviction.** Earlier drafts shipped four (Conviction / Patience / Risk-appetite / Trend-disposition). The other three are extracted to exercise the contrast pipeline but are not active in the headline experiment. Composition + regime-conditioned configs are v2. See §7.1. |
| Anti-overfit gate? | **Reportable, not blocking, in v1.** Original framing as a hard requirement was correct for a deployable trading agent and wrong for a hackathon — strict gate plus weak Q4 vectors plus a 100-trade sample makes "no config advances" too likely. v1 surfaces a named verdict (PassesBothRegimes / SingleRegimeEvidence / Fails) in the report. The gate must re-tighten to blocking when any automated optimizer over vector configs ships (Karpathy v2). See implementation-plan.md Phase 8.4. |

---

## 13. References

**Steering Vector Fields (SVF) — the core 2026 result on context-aware steering:**
- Li, Li, Huang. *Steering Vector Fields for Context-Aware Inference-Time Control in Large Language Models.* arXiv:2602.01654, Feb 2026. https://arxiv.org/abs/2602.01654

**SEAL — reasoning steering via hidden-state contrasts:**
- *SEAL: Steerable Reasoning Calibration of Large Language Models for Free.* arXiv:2504.07986. https://arxiv.org/abs/2504.07986
- *Self-Adapting Language Models* (related but separate — RL-driven self-edits). arXiv:2506.10943. https://arxiv.org/abs/2506.10943

**Practical state of the art — useful synthesis:**
- Mitra. *Activation Steering in 2026: A Practitioner's Field Guide.* https://subhadipmitra.com/blog/2026/activation-steering-field-guide/

**Adjacent work worth knowing:**
- *Steer2Adapt: Dynamically Composing Steering Vectors.* arXiv:2602.07276. https://arxiv.org/abs/2602.07276
- *From Steering Vectors to Conceptors: Compositional Affine Activation Steering.* OpenReview. https://openreview.net/forum?id=0Yu0eNdHyV
- *Reliable Control-Point Selection for Steering Reasoning.* arXiv:2604.02113. https://arxiv.org/abs/2604.02113

**Geometric / corridor framing inspiration:**
- Glamin (executable geometry). https://github.com/LynnColeArt/glamin

**Inference & ML (Rust):**
- candle (HuggingFace Rust ML framework). https://github.com/huggingface/candle
- llama-cpp-rs (fallback). https://github.com/utilityai/llama-cpp-rs
- mistralrs (candle-based serving). https://github.com/EricLBuehler/mistral.rs

**Vector extraction (Python, offline):**
- repeng (control vectors). https://github.com/vgel/repeng
- dialz (alternative steering toolkit). https://github.com/dialz/dialz
- transformers. https://github.com/huggingface/transformers

**Trading & onchain (Rust):**
- apca. https://github.com/d-e-s-o/apca
- alloy (modern Ethereum stack). https://github.com/alloy-rs/alloy
- ta (technical analysis). https://crates.io/crates/ta

**ERC-8004 — on-chain agent identity:**
- ERC-8004: Trustless Agents (EIP). https://eips.ethereum.org/EIPS/eip-8004
- Mantle ERC-8004 mainnet deployment. https://chainwire.org/2026/02/16/mantle-unlocks-autonomous-economy-with-erc-8004-deployment/
- ERC-8004 Identity and Reputation for AI Agents (Allium). https://www.allium.so/blog/onchain-ai-identity-what-erc-8004-unlocks-for-agent-infrastructure/
- ERC-8004 Developer Guide (QuickNode). https://blog.quicknode.com/erc-8004-a-developers-guide-to-trustless-ai-agent-identity/

**Observability & tracing:**
- OpenTelemetry GenAI semantic conventions. https://opentelemetry.io/docs/specs/semconv/gen-ai/
- Langfuse (self-hosted LLM observability). https://github.com/langfuse/langfuse
- Phoenix (Arize). https://github.com/Arize-ai/phoenix
- Pydantic Logfire (fallback via OTLP). https://logfire.pydantic.dev/
- Rust `tracing` crate. https://docs.rs/tracing/latest/tracing/
- `tracing-opentelemetry`. https://docs.rs/tracing-opentelemetry/

**Rust substrate:**
- `faiss-rs`. https://github.com/Enet4/faiss-rs
- `tokio`. https://tokio.rs/
- `arc-swap` (snapshot semantics). https://docs.rs/arc-swap/
- `serde` + `garde` (typed schemas with validation). https://serde.rs/ · https://github.com/jprochazk/garde
- `polars` (tabular data). https://pola.rs/
- `sqlx` (compile-time-checked queries). https://github.com/launchbadge/sqlx

**Companion design doc:**
- `steering-vector-architecture.md` — forward-thinking sibling, captures the May 2026 design conversation around Mitra, Glamin patterns, the Rust-from-day-one decision, and the offline Python extraction boundary.

---

*Document version: 2026-05-02. Lives at `/Users/edkennedy/Code/xianvec/architecture.md`.*
