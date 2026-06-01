# XVISION Implementation Plan (Rust)

> **2026-05-07: Plan reshaped per ADR 0011.** Original CV-driven phases
> (Phase 0 spike, Phase 4 vector ops, Phase 8 probe runner with
> introspection) have been removed. Strategy Loom + ERC-8004 marketplace
> work continues per the SLF queue in FOLLOWUPS.md and the surviving phase
> structure documented below.

> Multistrategy population evaluated through a deterministic loom. Hackathon claim: on a fixed set of trading setups, a population of N strategies (classical TA + onchain + LLM-driven) evaluated through the loom produces an on-chain ranking that distinguishes strategies beyond noise on Δ-Sharpe, with reputation and validation receipts visible on Mantle.

The runtime is Rust. Plotting and offline analysis use Python notebooks; the production binary has no Python in its process tree.

**See also:** `architecture.md` for the canonical architectural decisions; `decisions/0011-cv-extraction.md` for the CV substrate extraction.

---

## File structure (Cargo workspace)

**v1 scope decision (2026-05-03):** the workspace is a **single `crates/xvision-*` tree** for the hackathon. The lodestar / xvision subtree split documented in earlier drafts is **deferred to v2** (see "Future additions"). The same is true for several other items previously in v1 — see "v1 scope cuts" below the file tree.

```
xvision/
├── Cargo.toml                    # workspace root
├── rust-toolchain.toml
├── .pre-commit-config.yaml       # cargo fmt / clippy / test
├── Cargo.lock
│
├── crates/
│   ├── xvision-core/             # types, schemas, config loader, SQLite persistence
│   ├── xvision-data/             # OHLCV ingest, indicators, onchain signals
│   ├── xvision-intern/           # Stage 1 (any OpenAI- or Anthropic-compatible endpoint)
│   ├── xvision-trader/           # Stage 2 (TraderBackend HTTP trait)
│   ├── xvision-risk/             # deterministic risk layer
│   ├── xvision-execution/        # Stage 3: alpaca + orderly executors
│   ├── xvision-identity/         # ERC-8004 manifest + reputation/validation receipts
│   ├── xvision-eval/             # backtest harness, baselines, Δ-Sharpe
│   ├── xvision-harness/          # boundary probes (minimal v1 corpus)
│   └── xvision-cli/              # clap-based CLI; installed binary is `xvn`
│
├── config/
│   ├── default.toml              # runtime config
│   ├── whitelist.toml            # tradeable assets (BTC only for v1)
│   └── risk.toml                 # risk layer rules
│
├── data/
│   ├── decisions.db
│   └── probes/                   # boundary probe corpus (minimal v1 set, JSON)
│
├── identity/                     # ERC-8004 agentURI manifests
├── notebooks/                    # Python: eval plotting (offline)
├── .claude/skills/mantle/        # mantle-skills git submodule
├── decisions/                    # ADR-style decision records
│
└── docs/
    ├── architecture.md
    └── implementation-plan.md    # this file
```

### v1 scope cuts (deferred to v2)

The following items appeared in earlier drafts and are **explicitly out of v1**. Each lives in "Future additions" below with its trigger condition. Cuts made because the unconstrained scope was a 90-day plan being attempted in a 45-day window with one developer:

- **Multi-asset basket** — v1 runs on **BTC only** (PERP_BTC_USDC on Mantle via Orderly; BTC-USD on Alpaca paper). ETH/SOL return when the cluster-cap rule needs exercising.
- **Telemetry crate + OTel/Langfuse** — v1 writes a `traces` table in SQLite (§9.4 flight recorder is sufficient for replay). `tracing` + console output for live dev. OTel export, GenAI semantic conventions, and self-hosted Langfuse return post-v1.
- **Telegram bot (`xvision-bot`)** — v1 demo is CLI + report markdown + plots. Telegram is post-v1 polish.
- **xStocks integration** — Mantle tokenized equities are out of v1 entirely. PERP_BTC_USDC on Mantle via Orderly is the on-chain trade artifact; ERC-8004 NFT mint on Mantle is the on-chain identity artifact (same chain).
- **`mantle-risk-evaluator` LLM pre-flight gate** — v1 trusts the deterministic risk layer for the small forward run. Re-add when Orderly trade volume justifies a second LLM-mediated gate.

What stays in v1: TraderArm as one Strategy variant in the loom, classical TA + onchain baseline strategies, Alpaca paper for plumbing validation, ERC-8004 identity registration on Mantle, Orderly executor for live Mantle trades (single-chain audit trail), Byreal Agent Skills vendored for the Stage 1 Intern's context, the structural-review Tier 1 fixes, the boundary-probe runner (minimal corpus).

---

## Structural review (2026-05-02) — fixes baked into the tasks below

A pre-build review surfaced structural issues that would have suppressed the magnitude or invalidated the credibility of the headline Δ-Sharpe. Every fix is folded into the relevant task; this list is the manifest so the rationale is traceable.

**Tier 1 — material to Δ-Sharpe / CI / divergence credibility**

1. **Intern non-determinism breaks pairing.** Per-arm Claude calls produced different briefings for the same setup. Fix: cache briefings keyed by `setup_id` and run all paired strategy arms against the same cached briefing; set Intern `temperature=0`. *(Phase 2.2, 8.3, 9.2)*
2. **Trader temperature jitter inflates noise.** `temperature=0.4` makes the LLM-driven arms non-deterministic, polluting both PnL variance and decision-divergence rate. Fix: greedy decoding (`temperature=0`) for paired arms in the controlled backtest; sampled decoding only for forward paper. *(Phase 3.1, 9.2)*
3. **Backtest portfolio is frozen — risk layer is a no-op.** A fresh `{nav: 10000, open_positions: [], daily_pnl_pct: 0}` per setup means the risk rules are inert. Fix: stateful portfolio tracker in `iter_setups`/`run_backtest` updating NAV, open positions, daily PnL window, loss streak, and ATR across the test window. *(Phase 8.3)*
4. **Setup overlap inflates effective n.** `step=8` with `horizon=16` shares half the forward window across consecutive setups. Fix: `step >= horizon` (default 24); add a block-bootstrap option for time-series-correct CIs. *(Phase 8.2, 8.3)*

**Tier 2 — credibility and statistical power**

5. **`returns_from_pnl` is path-dependent.** Dividing by trailing equity makes the return series order-dependent; bootstrap permutations corrupt Sharpe. Fix: `pnl_i / nav_initial` (constant denominator); order-invariant. *(Phase 8.1)*
6. **Single-asset eval halves statistical power.** Hardcoded `BTC-USD` while architecture and risk layer assume a basket. Fix: iterate over the whitelist (BTC + ETH + SOL); concatenate paired returns across assets for the bootstrap. Also exercises the cluster-cap path. *(Phase 9.2)*

**Tier 3 — cleanup**

- Risk layer runs twice (pipeline + harness) — pipeline owns risk, harness trusts the decision. *(Phase 8.3, 9.2)*
- Decision divergence defined on `action` only — extend to `(action, direction, size_bucket)`. *(Phase 9.2)*
- Briefing log uses literal `setup_id="ab"` — fix to use real setup_id. *(Phase 9.2)*
- Walk-forward `train` slice generated but unused — v1 takes the delete path; document it. *(Phase 8.4)*
- Δ-Sharpe is the only inferential test; secondary metrics (MDD, PF, WR) are descriptive and not multiple-comparisons-corrected. State this in the report. *(Phase 10.2)*

---

## Mantle hackathon integration (mandatory)

The Turing Test hackathon runs on Mantle. Two integrations move from "v2 deferred" to "v1 required":

1. **Orderly Network** as the on-chain perpetual-futures execution path on Mantle (chain_id 5000).
2. **ERC-8004 identity + reputation + validation registries on Mantle** as the public anchor for the experimental comparison.

Adding these *before* Phase 9's A/B run produces a meaningfully better artifact: the experimental claim becomes trustless and publicly verifiable, not just a SQLite table on a laptop. With Orderly on Mantle, identity / trades / reputation / validation all live on the same chain — single-chain audit trail.

**Venue choice (2026-05-03).** The day went through three candidates before settling: Byreal Perps on Mantle (turned out to be Hyperliquid, not Mantle), Vertex Protocol (operationally dead), Byreal Perps CLI on Hyperliquid (worked but cross-chain), and finally Orderly on Mantle (Mantle-native + Rust-native + bigger liquidity). Full rationale and decision matrix at `decisions/0006-executor-choice.md`. The hackathon's Path 1 endorsement of Byreal tooling is satisfied by vendoring **Byreal Agent Skills** as the Stage 1 Intern's skill catalog (M4) — the named-tool endorsement is met through context, not execution. The Byreal Perps CLI executor path is preserved as a verified fork option (M0 at `probes/m0-byreal/` passed) if a stricter reading of the brief turns out to require it.

**Two execution paths run side by side:**

- **Alpaca paper** — pre-launch testing path. Verifies Stage 1→2→3 plumbing, pipeline determinism, risk-layer behaviour against a battle-tested broker simulator before on-chain capital is touched. Required.
- **Orderly Network on Mantle** — hackathon submission path. Real on-chain execution against Mantle vault `0x816f722424B49Cf1275cc86DA9840Fbd5a6167e9`.

The capital bridge (`@mantleio/sdk`) is **explicitly out of scope** — funds are pre-funded on Mantle by the user before any forward run. The agent only ever sees on-Mantle balances.

### M0. Pre-skeleton venue verification ✅ (2026-05-03)

Two probes verified the executor path end-to-end. Both passed.

**Primary probe — `probes/m0-orderly/`** (the v1 path). Constructs `OrderlyService::with_base_url("https://api-evm.orderly.org", Some(10))` via `orderly-connector-rs = "0.4.15"`, calls `get_system_status` and `get_futures_info("PERP_BTC_USDC")` against the live EVM gateway. Verifies Mantle (chain_id 5000) is a registered deposit chain with vault `0x816f722424B49Cf1275cc86DA9840Fbd5a6167e9`. **Result: PASS.** System status 0, BTC-PERP mark $78,382 / index $78,419 live, 99 perp markets, all Phase 6.3 SDK methods resolve.

**Fork-option probe — `probes/m0-byreal/`** (preserved as the verified alternate). Shells out via `tokio::process::Command` to `npx -y @byreal-io/byreal-perps-cli@latest catalog -o json` and parses the `{success, meta, data.capabilities}` envelope. **Result: PASS.** CLI v0.3.7 returns 20 capabilities (5 query, 13 execute, 2 update). One naming note: `position.close` is split into `close-market` / `close-limit` / `close-all`. Retained because forking the executor from `orderly.rs` to `byreal.rs` is mechanical if Path 1 turns out to require it.

Both probe directories stay in-tree until Phase 6.3 lands; then they can be deleted (or kept as smoke tests in CI).

### M1. ERC-8004 identity registration (per strategy)

Each strategy variant in the loom gets its own identity NFT on Mantle, and each posts performance updates to the same reputation registry — the comparison is a publicly auditable single-chain experiment.

- Per-strategy `agentURI` manifests live in `identity/` (JSON metadata: agent_id, strategy_name, code_commit, strategy_adapter_type, risk_preset). Pin to IPFS or HTTPS.
- Mint via the Identity Registry contract (Mantle mainnet) using `alloy`.
- After every closed Orderly position on Mantle, post a validation update keyed by `(setup_id, strategy_id, outcome)`.
- All NFTs and reputation history become demo evidence.

Implemented in **Phase 6.5**. Must be in place before any forward Orderly run.

### M2. Orderly Network as the on-chain execution path

`orderly-connector-rs = "0.4"` (ranger-finance, MIT, last published 2025-06; M0 confirms it works against the current API). Stage 3 gets a *second* executor alongside Alpaca paper — same `RiskDecision → Stage 3` contract, different downstream tool. A `--executor {alpaca,orderly}` CLI flag selects between them.

Implementation in `crates/xvision-execution/orderly.rs` constructs an `OrderlyService` against `https://api-evm.orderly.org`, holds `Credentials { orderly_key, orderly_secret, orderly_account_id }` for signed calls, and surfaces SDK methods (`create_order`, `cancel_order`, `get_holding`, `get_positions`, `get_account_info`) through the `Executor` trait. No Node.js runtime dependency, no subprocess shellout.

Implemented in **Phase 6.3** (parallel to Phase 6.2 Alpaca).

### M3. On-chain decision logging

Every Stage-1 → Stage-2 → Stage-3 cycle that completes a trade via Orderly emits a reputation- and validation-registry post on Mantle, tagged with the agent NFT, the setup_id, the action signature, and the realized PnL. SQLite remains for fast local replay; the on-chain log is the authoritative public record. Alpaca paper trades persist locally only.

Implemented in **Phase 11.5**.

### M4. Skill catalogs (Byreal Agent Skills + mantle-skills)

The hackathon's Path 1 names *Byreal Agent Skills* among its winning tooling. Even though we don't execute through Byreal, the Stage 1 Intern still loads Byreal Agent Skills as Claude-context, satisfying that endorsement and giving the Intern domain knowledge about perpetual-futures trading patterns and risk shapes (the skills travel cleanly even when the execution venue is different).

- **`byreal-git/byreal-agent-skills`** — vendor as a git submodule under `.claude/skills/byreal/`.
- **`github.com/mantle-xyz/mantle-skills`** — vendor under `.claude/skills/mantle/` (Mantle-host context for the ERC-8004 work and for any Mantle-specific Stage-1 reasoning).

Implemented in **Phase 0.4** (vendor) and consumed by Stage 1 Intern config + Phase 11.5 forward runner.

### Priority sequencing for the hackathon

1. **M0 venue verification** — ✅ done 2026-05-03 via `probes/m0-orderly/` (primary) and `probes/m0-byreal/` (fork option).
2. **Phase 0–8** as planned (structural fixes are venue-independent). Phase 0.4 vendors both skill catalogs.
3. **Phase 6.5** ERC-8004 — must precede the forward Orderly run. Develop in parallel with Phase 6.3.
4. **Phase 6.3** Orderly executor — alongside Phase 6.2 Alpaca, not replacing it.
5. **Phase 9** unchanged: backtest, no on-chain dependency.
6. **Phase 11.1** Alpaca paper forward run — first; validates Stage 1→2→3 against a battle-tested broker.
7. **Phase 11.5** Orderly forward run on Mantle — second; small N (5–20 paired trades) suffices for on-chain proof. Headline statistical claim still rides on Phase 9.
8. **Phase 12** acceptance criteria include the on-chain items.

**v1 cuts to this section:** xStocks integration (Mantle tokenized equities — out, no execution venue) and `mantle-risk-evaluator` LLM pre-flight gate. Both documented in "Future additions" with re-add triggers.

---

## Phase 0 — Foundation

Workspace scaffolding + vendored skill catalogs. Per ADR 0011, the original
vector validation spike (CRITICAL GATE) is gone — the CV substrate moved to
xvision-play.

### Task 0.1: Cargo workspace init (single tree)

Create the workspace from the file structure above as a single `crates/xvision-*` tree. Each crate starts as a stub with `lib.rs` and one passing test. No lodestar split, no `deny.toml` boundary check — both deferred to v2.

**Acceptance:**
- `cargo build --workspace` succeeds on stable Rust
- `cargo test --workspace` passes with stub tests
- `pre-commit` config runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`
- `rust-toolchain.toml` pins the Rust version

**Key crates pulled in workspace `Cargo.toml`** (versions verified against crates.io 2026-05-02; pin minor versions when the build settles):

```toml
[workspace.dependencies]
candle-core      = "0.10"
candle-nn        = "0.10"
candle-transformers = "0.10"
tokio            = { version = "1", features = ["full"] }
serde            = { version = "1", features = ["derive"] }
serde_json       = "1"
garde            = { version = "0.22", features = ["derive"] }
sqlx             = { version = "0.8", features = ["runtime-tokio", "sqlite", "macros"] }
arc-swap         = "1"
polars           = { version = "0.53", features = ["lazy", "parquet"] }
ndarray          = "0.16"
tracing          = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-opentelemetry = "0.32"
opentelemetry    = "0.31"
opentelemetry-otlp = "0.31"
opentelemetry_sdk = "0.31"
clap             = { version = "4", features = ["derive"] }
reqwest          = { version = "0.13", features = ["json"] }
alloy            = { version = "2", features = ["full"] }   # 2.x is a major rewrite vs 0.x
teloxide         = "0.17"
proptest         = "1"
criterion        = "0.8"
thiserror        = "2"
anyhow           = "1"
async-trait      = "0.1"
chrono           = { version = "0.4", features = ["serde"] }
uuid             = { version = "1", features = ["v4", "serde"] }
apca             = "0.30"          # mature Alpaca client (alpaca-rs is a stub)
```

### Task 0.4: Vendor mantle-skills

Add `github.com/mantle-xyz/mantle-skills` as a git submodule under `.claude/skills/mantle/`. Verify the skill catalog is loadable and contains the expected skills (network primer, address registry navigator, risk evaluator, portfolio analyst, defi operator, tx simulator, openclaw competition).

**Acceptance:**
- Submodule present at correct path
- `git submodule status` clean
- README documents which skills are loaded into Claude project context for which tasks

---

## Phase 1 — Schemas, config, persistence

### Task 1.1: Schema crate (`xvision-core`)

Trading types live in `xvision-core::trading` (`Action`, `Direction`, `AssetSymbol`, `Regime`, `EvidenceTag`, `InternBriefing`, `TraderDecision`, `RiskDecision`, `VetoReason`).

Stage handoff types as `serde` + `garde` structs. Type-level enforcement everywhere it works; runtime validation at the boundaries.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct InternBriefing {
    pub setup_id: Uuid,
    pub asset: AssetSymbol,
    #[garde(length(min = 20, max = 2000))]
    pub bull_case: String,
    #[garde(length(min = 20, max = 2000))]
    pub bear_case: String,
    #[garde(length(min = 20, max = 2000))]
    pub flat_case: String,
    pub evidence_long:  Vec<EvidenceTag>,
    pub evidence_short: Vec<EvidenceTag>,
    pub evidence_flat:  Vec<EvidenceTag>,
    pub regime: Regime,
    #[garde(range(min = 0.0, max = 1.0))]
    pub signal_quality: f32,
    pub horizon_hours: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct TraderDecision {
    pub setup_id: Uuid,
    pub action: Action,                 // enum: Buy | Sell | Flat | Close
    #[garde(range(min = 0, max = 2000))]
    pub size_bps: u32,
    pub direction: Direction,           // enum: Long | Short | Flat
    #[garde(range(min = 0.1, max = 20.0))]
    pub stop_loss_pct: f32,
    #[garde(range(min = 0.1, max = 50.0))]
    pub take_profit_pct: f32,
    #[garde(length(min = 10, max = 500))]
    pub trader_summary: String,
}

#[derive(Debug, Clone)]
pub enum RiskDecision {
    Approved(TraderDecision),
    Modified { original: TraderDecision, modified: TraderDecision, reason: VetoReason },
    Vetoed   { original: TraderDecision, reason: VetoReason },
}
```

**Acceptance:**
- Round-trip `serde_json` for every type
- `garde` validation rejects out-of-range values with structured errors
- `proptest` generators for fuzz tests of downstream code

### Task 1.2: Config loader (`crates/xvision-core/src/config.rs`)

TOML-backed config (we use TOML over YAML — it integrates with Cargo idioms and `serde` parsing is first-class).

**`config/default.toml`:**
```toml
[runtime]
mode = "backtest"           # backtest | paper | live
executor = "alpaca"         # alpaca | orderly
random_seed = 42

[intern]
provider = "anthropic"      # anthropic | openai | local-candle
base_url = "https://api.anthropic.com"   # any Anthropic- or OpenAI-compatible endpoint
model = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"        # env var name; empty for local servers without auth
temperature = 0.0           # MUST be 0 for backtest pairing (Tier 1 fix #1)
reasoning_effort = "low"    # forwarded when provider supports it (o-series, R1, Qwen-thinking)
# Examples (drop-in swaps):
#   OpenAI:      provider = "openai",   base_url = "https://api.openai.com/v1",        model = "gpt-5",                 api_key_env = "OPENAI_API_KEY"
#   OpenRouter:  provider = "openai",   base_url = "https://openrouter.ai/api/v1",     model = "deepseek/deepseek-r1",  api_key_env = "OPENROUTER_API_KEY"
#   vLLM (loc):  provider = "openai",   base_url = "http://localhost:8000/v1",         model = "Qwen/Qwen3-32B",        api_key_env = ""
#   Ollama:      provider = "openai",   base_url = "http://localhost:11434/v1",        model = "qwen3:32b",             api_key_env = ""
#   LM Studio:   provider = "openai",   base_url = "http://localhost:1234/v1",         model = "<loaded-model-name>",   api_key_env = ""

[trader]
provider = "openai"          # openai | anthropic | local-candle
base_url = "https://api.openai.com/v1"
model = "gpt-5"
api_key_env = "OPENAI_API_KEY"
temperature = 0.0            # MUST be 0 for backtest (Tier 1 fix #2)
forward_paper_temperature = 0.4

[backtest]
step = 24                   # >= horizon (Tier 1 fix #4)
horizon = 16
```

**`config/whitelist.toml`** — assets and per-venue symbol mappings (BTC only in v1; xStocks deferred).
**`config/risk.toml`** — risk layer thresholds.

Acceptance: round-trip + validation tests; bad configs produce structured errors not panics.

### Task 1.3: SQLite persistence (`crates/xvision-core/src/store.rs`)

`sqlx` with compile-time-checked queries. Tables: `setups`, `briefings`, `decisions`, `risk_outcomes`, `executions`, `traces`.

**Key invariants:**
- `briefings` keyed on `setup_id` only (Tier 1 fix #1: same briefing serves all paired arms)
- `decisions` keyed on `(setup_id, arm_name)` — each strategy persists independently
- `traces` mirrors the OTel span structure for offline replay (§9.4 flight recorder)

Acceptance: migrations run cleanly, `sqlx::query!` macros compile-check against the schema, round-trip inserts/queries for every type.

### Task 1.4: Technical indicators (`crates/xvision-data/src/indicators.rs`)

RSI(14), SMA(20/50/200), EMA(12/26), Bollinger Bands(20, 2σ), ATR(14), MACD(12/26/9), Donchian(20), Fibonacci retracements with rolling-window peak detection.

Use `polars` lazy frames where possible; hand-code the few indicators not in the `ta` crate.

Acceptance: per-indicator unit tests against canonical fixture data (e.g. RSI on a published worked example agrees to 1e-6).

---

## Phase 2 — Stage 1 Intern (`crates/xvision-intern/`)

### Task 2.1: Intern prompt builder

The Intern emits balanced bull/bear/flat cases — never recommends. The prompt explicitly forbids `candidate_direction` so the Trader makes a clean judgment from balanced inputs (§2 architecture).

```rust
pub fn build_intern_prompt(state: &MarketState, mantle_skills: &[Skill]) -> String { ... }
```

Acceptance: snapshot tests against fixed market state inputs; output prompts are deterministic.

### Task 2.2: Intern via interchangeable HTTP backends (OpenAI- or Anthropic-compatible)

One trait, three concrete backends — picked at runtime via config so users can point any reasoning model, hosted API, or self-hosted inference server at Stage 1 without a recompile.

```rust
#[async_trait]
pub trait InternBackend: Send + Sync {
    async fn brief(&self, prompt: &str) -> Result<InternBriefing>;
}
```

Implementations:

- **`OpenAICompatIntern`** — speaks the OpenAI Chat Completions wire format. Configurable `base_url` covers the entire OpenAI-compatible ecosystem with a single code path: OpenAI proper, OpenRouter, Together, Groq, DeepSeek, xAI, Mistral, vLLM, Ollama (`/v1` endpoint), LM Studio, llama.cpp's server, TGI, etc. Optional `api_key_env` (skip for local servers without auth).
- **`AnthropicIntern`** — speaks the Anthropic Messages API. Used for Claude models and any Anthropic-API-compatible gateway.
- **`LocalCandleIntern`** *(optional, deferred)* — direct in-process candle inference for fully air-gapped runs. Lower priority than the HTTP path because OpenAI-compat against a local vLLM/Ollama server gives the same air-gap property with vastly more model coverage.

All backends set `temperature=0` for the backtest path (Tier 1 fix #1). Output is parsed via `serde_json` + `garde`; on parse failure, retry once with a corrective system message.

**Reasoning-model handling.** Reasoning models (o-series, DeepSeek-R1, Qwen-thinking, gpt-oss with reasoning) emit thinking tokens before the JSON. The backend strips two shapes before validation: provider-native reasoning fields (`response.choices[].message.reasoning_content`, Anthropic `thinking` blocks) and inline `<think>...</think>` blocks. When the provider exposes `reasoning_effort`, the backend forwards a configured value; otherwise the strip step alone is sufficient.

**Briefing cache:** keyed on `setup_id`. All paired strategy arms read the same cached briefing (Tier 1 fix #1 — pairing). The cache key includes `(provider, model)` so swapping backends invalidates cleanly.

Acceptance:
- Live call against each of {Anthropic Claude, OpenAI gpt-style, a local OpenAI-compat server (Ollama or vLLM)} returns a valid `InternBriefing` for a fixture market state
- Reasoning-model fixture (one of: o-series, R1, Qwen-thinking) parses correctly with thinking tokens stripped
- Cached briefing reused across paired arms; cache key invalidated when `(provider, model)` changes
- Mantle-skill context loaded into the prompt for Mantle-touching setups, regardless of backend

---

## Phase 3 — Stage 2 Trader

### Task 3.1: Trader backend (`crates/xvision-trader/`)

`TraderBackend` HTTP trait abstracts over OpenAI-compatible endpoints; `OpenAiCompatBackend` is the default impl. Optional local `candle` inference is available as a separate path for fully air-gapped runs.

`temperature=0` (greedy) for backtest paths (Tier 1 fix #2). Sampled decoding only for forward-paper.

### Task 3.2: Trader prompt + JSON-constrained generation

The Trader receives an `InternBriefing` and emits a `TraderDecision`. Use a constrained-generation grammar (or schema validation with single retry) to keep output parseable.

```rust
pub async fn run_trader(
    backend: &dyn TraderBackend,
    briefing: &InternBriefing,
    portfolio: &PortfolioState,
    params: &TraderParams,
) -> Result<TraderDecision>;
```

Acceptance:
- 95%+ first-pass JSON parse rate on fixture briefings
- 99%+ after one retry
- Output validates against the `garde` schema
- Decision is logged to `decisions` table keyed on `(setup_id, arm_name)`

### Task 3.3: Smoke pipeline (Intern → Trader)

End-to-end test that runs Stage 1 + Stage 2 on a fixture setup. Confirms plumbing before downstream phases.

---

## Phase 5 — Risk Layer (`crates/xvision-risk/`)

Deterministic, no LLM. Pure rule evaluation.

```rust
pub struct RiskLayer { rules: Vec<Box<dyn RiskRule>>, config: RiskConfig }

pub trait RiskRule: Send + Sync {
    fn evaluate(&self, decision: &TraderDecision, portfolio: &PortfolioState) -> RuleVerdict;
}

pub enum RuleVerdict {
    Pass,
    Modify(TraderDecision, VetoReason),
    Veto(VetoReason),
}

impl RiskLayer {
    pub fn evaluate(&self, decision: TraderDecision, portfolio: &PortfolioState) -> RiskDecision { ... }
}
```

Rules (initial set, from `architecture.md` §5):
- Max position size 20% NAV
- Max total exposure 100% NAV
- Asset whitelist
- Daily loss circuit breaker 5%
- Max 5 open positions
- Correlation cluster cap (≤2 per cluster)
- Stop-loss required

Vetoes are logged to `risk_outcomes` with reason. Vetoes are signal — they tell us when a strategy pushes the agent into territory a human risk manager would also reject.

---

## Phase 6 — Stage 3 Execution

### Task 6.1: Executor trait

```rust
#[async_trait]
pub trait Executor: Send + Sync {
    async fn submit(&self, decision: &RiskDecision) -> Result<ExecutionReceipt>;
    async fn close_position(&self, asset: AssetSymbol) -> Result<ExecutionReceipt>;
    async fn portfolio(&self) -> Result<PortfolioState>;
}
```

Idempotency: each decision carries `setup_id` used as client order ID.

### Task 6.2: Alpaca executor (`crates/xvision-execution/alpaca.rs`)

`apca` (mature Alpaca client; `alpaca-rs` on crates.io is a 0.1.0 stub). Submit market or bracket orders. Read portfolio state after every action and cache for next Stage-1 input.

### Task 6.3: Orderly executor (`crates/xvision-execution/orderly.rs`)

Native Rust async via `orderly-connector-rs = "0.4"` (`OrderlyService` + `Credentials`). Same `Executor` trait surface as `AlpacaExecutor`; different downstream tool. No Node.js dependency, no subprocess.

```rust
use orderly_connector_rs::rest::OrderlyService;
use orderly_connector_rs::rest::client::Credentials;

pub struct OrderlyExecutor {
    svc: OrderlyService,
    creds: Credentials<'static>,
    /// "PERP_BTC_USDC" for v1 BTC-only.
    symbol: String,
}

impl OrderlyExecutor {
    pub fn connect(creds: Credentials<'static>) -> Result<Self> {
        let svc = OrderlyService::with_base_url("https://api-evm.orderly.org", Some(10))?;
        Ok(Self { svc, creds, symbol: "PERP_BTC_USDC".into() })
    }
}

#[async_trait]
impl Executor for OrderlyExecutor {
    async fn submit(&self, decision: &RiskDecision) -> Result<ExecutionReceipt> { ... }
    async fn close_position(&self, asset: AssetSymbol) -> Result<ExecutionReceipt> { ... }
    async fn portfolio(&self) -> Result<PortfolioState> { ... }
}
```

**SDK method mapping** (verified against `probes/m0-orderly/` 2026-05-03):

| Trait surface | Orderly SDK call |
|---|---|
| `submit(decision)` (entry) | `svc.create_order(&creds, …)` with `OrderType::Market` (or `Limit` with `price`) |
| `submit(decision)` (TP/SL) | algo orders via `svc.create_algo_order(&creds, …)` after the entry fills |
| `close_position(asset)` | submit an opposing `OrderType::Market` of equal size (Orderly is order-based; "close" is a counter-trade, not a distinct primitive) |
| `portfolio()` | `svc.get_account_info(&creds)` + `svc.get_positions(&creds)` joined |
| Cancel an open order | `svc.cancel_order(&creds, order_id)` |
| Live mark price for stops | `svc.get_futures_info(Some(symbol))` (no creds) |

`setup_id` rides in the `client_order_id` field on `create_order`; we record `(setup_id, server_order_id)` pairs in SQLite for reconciliation.

**Credentials.** Orderly uses `(orderly_key, orderly_secret, orderly_account_id)` for signed calls. The `account_id` is derived from a brokered onboarding flow (one-time setup); keys are loaded from `op` (1Password CLI) per workspace convention. `xvn setup --orderly-onboard` runs the brokered onboarding once and writes the resulting account_id to local config; secrets stay in `op`.

**Acceptance:**
- M0' probe (✅ 2026-05-03) already verified the SDK reaches the live API on Mantle. Phase 6.3 builds the `Executor` impl on top of the proven SDK surface.
- Place + cancel a small `PERP_BTC_USDC` order against the live API with size below the caps in `risk.toml` (or testnet equivalent if Orderly exposes one — check `https://testnet-api-evm.orderly.org` during Phase 6.3).
- `get_account_info` + `get_positions` reads land in the same `PortfolioState` shape Alpaca produces.
- All SDK errors (`OrderlyError`) map cleanly into the executor's error enum without `unwrap()` in the hot path.

### Task 6.4: Backtest simulator

In-process executor that takes `RiskDecision` and walks forward through historical OHLCV applying realistic slippage and fees. Implements the same `Executor` trait so `xvision-eval` swaps it in transparently.

**Tier 1 fix #3:** Stateful portfolio tracker — NAV, open positions, daily PnL window, loss streak, ATR — updated across the test window. The risk layer must actually fire during backtest.

---

## Phase 6.5 — ERC-8004 identity registration (Mantle hackathon)

Per-strategy `agentURI` manifests in `identity/` (one per Strategy variant). Mint via `alloy` against the Identity Registry contract on Mantle mainnet.

```rust
pub struct IdentityClient { provider: Provider, registry: Address }

impl IdentityClient {
    pub async fn register(&self, agent_uri: &Url, signer: &PrivateKeySigner) -> Result<TokenId> { ... }
    pub async fn post_reputation(&self, agent: TokenId, setup_id: Uuid, outcome: TradeOutcome) -> Result<TxHash> { ... }
}
```

Acceptance: each strategy has its NFT minted; reputation posts succeed for fixture trades on Mantle testnet before main run.

---

## Phase 7 — Baselines (`crates/xvision-eval/baselines/`)

Each baseline implements a simple decision rule that consumes the same `MarketState` the Intern sees and emits a `TraderDecision`-shaped output (action + size + direction + stops). They are evaluated by the same backtest harness.

**Null baselines (must beat):** buy-and-hold, random direction with constant 1% sizing, always-long, always-short.

**Classical technicals:** RSI(14) 30/70 mean-reversion, MA(30/90) crossover, MA(30/60/90) triple-confirmation, Bollinger(20, 2σ) mean-reversion, MACD(12/26/9) momentum, Donchian(20) breakout, Fibonacci 38.2/50/61.8 retracements with peak detection.

**Onchain (the real bar):** Nansen smart-money copy-trader, funding-rate fader, stablecoin exchange-inflow risk-off, liquidation cascade fader.

**ML stretch:** XGBoost on technical + onchain features (use `xgboost-rs` or shell out to a Python script — XGBoost training/serving in Rust is workable but unergonomic; if it's a stretch baseline only, the Python escape hatch is fine).

Each baseline outputs to `data/baselines/{name}.parquet` consumed by the eval framework.

---

## Phase 8 — Eval framework (`crates/xvision-eval/`)

The most important non-obvious piece. Without it, strategy comparisons cannot be measured.

### Task 8.1: Returns + Sharpe machinery

```rust
pub fn returns_from_pnl(pnls: &[f32], nav_initial: f32) -> Vec<f32> {
    // Tier 1 fix #8: constant denominator, order-invariant
    pnls.iter().map(|p| p / nav_initial).collect()
}

pub fn sharpe_annualized(returns: &[f32], periods_per_year: f32) -> f32 { ... }
pub fn paired_bootstrap_sharpe_delta(
    returns_a: &[f32],
    returns_b: &[f32],
    n_resamples: usize,
    block_size: Option<usize>,    // Tier 1 fix #4: block-bootstrap option
) -> BootstrapResult { ... }
```

### Task 8.2: Backtest harness

Iterate setups across the whitelist (Tier 2 fix #10 — multi-asset), run paired arms against the cached briefing, route through the risk layer, settle via the in-process executor against historical OHLCV.

```rust
pub struct BacktestRunner {
    intern: Arc<dyn InternBackend>,
    trader_arms: Vec<TraderArm>,
    risk: Arc<RiskLayer>,
    executor: Arc<dyn Executor>,
    config: BacktestConfig,
}

pub struct TraderArm {
    pub name: String,
    pub temperature: f32,               // 0.0 in backtest (Tier 1 fix #2)
}

impl BacktestRunner {
    pub async fn run(&self, setups: &[SetupSpec]) -> Result<BacktestResult> { ... }
}
```

`step >= horizon` (Tier 1 fix #4) enforced by config validation.

### Task 8.3: Pre-committed metrics

```rust
pub struct PreCommittedMetrics {
    pub delta_sharpe: BootstrapResult,                  // primary
    pub max_drawdown_pct: HashMap<ArmName, f32>,
    pub profit_factor: HashMap<ArmName, f32>,
    pub win_rate: HashMap<ArmName, f32>,
    pub decision_divergence_rate: f32,                  // Tier 3: extended to (action, direction, size_bucket)
    pub regime_stratified: HashMap<Regime, RegimeMetrics>,
}
```

Min 30 paired trades; target 100+ for hackathon demo. 95% CI via paired bootstrap (10k resamples).

### Task 8.4: Anti-overfitting gate (REPORTABLE, NOT BLOCKING)

The gate is computed and **reported with explicit framing**, but in v1 it does **not block** the forward paper run. The strict "hard requirement" framing was right for a deployable trading agent and wrong for a 45-day hackathon: a strict gate combined with a 100-trade sample produces a high probability that *no* strategy advances, killing the demo even when honest single-regime evidence exists.

**v1 behaviour:** compute Δ-Sharpe stratified by regime (pre-2023 bear, 2023–2024 bull, plus any other detected regimes in the data). Surface three named verdicts in the report:

- **`PassesBothRegimes`** — positive Δ-Sharpe with CI excluding zero in both. Cleared for forward paper. Headline claim: "this strategy generalizes."
- **`SingleRegimeEvidence`** — positive in one regime only. Cleared for forward paper *with the caveat printed in the demo report*: "evidence is regime-specific; deployment requires further validation." Honest, hackathon-presentable.
- **`Fails`** — non-positive in every detected regime, OR positive estimate but CI crosses zero in every regime. Forward paper run is still *permitted* (the eval is not the final word) but the report leads with the failure.

```rust
pub enum GateVerdict {
    PassesBothRegimes,
    SingleRegimeEvidence { winning_regime: Regime, losing_regime: Regime },
    Fails { regimes: Vec<Regime> },
}

pub fn anti_overfit_verdict(result: &BacktestResult) -> GateVerdict { ... }
```

Rationale: the NexusTrade $676 warning still applies — a *self-improvement loop* (Karpathy autooptimizer v2) without the gate will hill-climb into single-regime optima. v1 is not running a self-improvement loop; v1 is one human picking strategy variants and reporting their limits honestly. The gate's epistemic role is preserved (the report frames the result truthfully); its scheduling role (blocking forward paper) is what gets relaxed.

Re-tightening trigger: any v2 work that adds an automated optimizer over strategy variants or strategy parameters — in that mode, the gate must block again to prevent Goodhart.


## Phase 9 — Pipeline orchestration + the A/B experiment

### Task 9.1: Ops (`crates/xvision-cli/src/ops.rs`)

Composes Stage 1 (cached briefing per setup) → Stage 2 (paired arms) → Risk → Executor. Logs everything via `tracing` with the GenAI semantic conventions (Phase T.1 telemetry).

### Task 9.2: A/B comparison runner

```bash
cargo run --release -p xvision-cli -- ab-compare \
  --setups data/setups/2022_2024_paired.parquet \
  --asset BTC-USD \
  --arms trader_arm,buy_hold,rsi_mean_reversion,ma_crossover \
  --output reports/ab_compare/$(date -Iseconds)
```

**v1 single-asset (BTC).** Runs the active strategy set with `temperature=0` (Tier 1 fix #2) against cached briefings (Tier 1 fix #1), risk layer fires at pipeline scope only (Tier 3 cleanup), decision divergence computed on `(action, direction, size_bucket)` (Tier 3 cleanup), real `setup_id` logged (Tier 3 cleanup). Tier 2 fix #6 (multi-asset basket) is deferred — see "Future additions."

Output: structured JSON consumed by the Python notebook for plots + summary statistics for the demo report.

---

## Phase 10 — Demo polish

### Task 10.1: Demo CLI commands

`xvn` gains a small set of demo-supporting subcommands that double as judge-reproducibility entry points:

- `xvn run-setup --setup-id <uuid>` — runs a single setup end-to-end, prints the briefing, the paired decisions across the active strategy set, the risk verdict, and the would-be execution.
- `xvn show-decision --setup-id <uuid> --arm <name>` — pretty-prints the cached decision with arm name and gate metadata.
- `xvn show-metrics --report <path>` — renders the latest A/B report's headline Δ-Sharpe and dashboard.

Telegram bot (`xvision-bot`) is deferred to v2 polish — see "Future additions."

### Task 10.2: Report generator

Renders the headline Δ-Sharpe with 95% CI, the secondary metrics dashboard, regime-stratified results with the named gate verdict (Task 8.4), and the divergence-rate table. Output is a single Markdown file plus the Python-notebook-rendered plots.

The report explicitly states which metrics are inferential (Δ-Sharpe) versus descriptive (MDD, PF, WR) and notes that secondary metrics are not multiple-comparisons-corrected. Where the gate verdict is `SingleRegimeEvidence` or `Fails`, the report leads with that framing rather than burying it.

---

## Phase 11 — Forward paper trading + onchain data

### Task 11.1: Alpaca paper forward run

Run the full pipeline live against Alpaca paper for at least 4–7 days (whatever fits in the schedule after the backtest is in the can — see premortem; this is one of the easiest tasks to lose to clock drift) before any Mantle capital is touched. The strategy population runs alternating setups so live data is paired.

### Task 11.5: Orderly forward run on Mantle (M3)

After Alpaca paper validation, switch the executor to `orderly`. Small N (5–20 paired live trades on `PERP_BTC_USDC`) suffices for the on-chain proof — the headline statistical claim still rides on Phase 9's backtest. Each closed Orderly trade emits an ERC-8004 reputation- and validation-registry post on the same chain (Mantle), tagged with the agent NFT, completing the single-chain audit trail.

`mantle-risk-evaluator` LLM pre-flight gate from earlier drafts is **deferred to v2** — v1 trusts the deterministic risk layer for the small forward run. Re-add when forward volume justifies a second LLM-mediated gate.

---

## Phase 12 — Self-review checklist

Acceptance criteria for hackathon submission:

- [x] M0 venue verification passed: Orderly primary (`probes/m0-orderly/`) + Byreal fork option (`probes/m0-byreal/`) — done 2026-05-03
- [ ] All Tier 1 structural fixes verified in code and tests
- [ ] Backtest harness produces stable results across 3 reruns on identical seeds
- [ ] Anti-overfit gate computed and verdict reported (gate is reportable, not blocking — Task 8.4)
- [ ] Δ-Sharpe with 95% CI reported for ≥100 paired trades on BTC-USD
- [ ] Per-strategy ERC-8004 identity NFTs minted on Mantle mainnet
- [ ] Byreal Agent Skills + mantle-skills loaded into Claude project context
- [ ] ≥1 Alpaca paper trade closed
- [ ] ≥1 Orderly trade closed on Mantle (`PERP_BTC_USDC`)
- [ ] ≥1 ERC-8004 reputation-registry post per strategy on Mantle, tied to a closed Orderly trade
- [ ] Demo report rendered with plots and reproducibility steps (single `cargo run --release` invocation reproduces the headline)

---

## Telemetry (v1: SQLite flight recorder only)

v1 ships the §9.4 SQLite flight recorder plus `tracing` with `tracing-subscriber` printing to stderr in dev. **OTel export, GenAI semantic conventions, and self-hosted Langfuse are deferred to v2** — appropriate for a deployable serving system, over-budget for a 45-day hackathon. The conflict between "autooptimizer loop without traces is just drift" (true, but the v1 scope explicitly does not run an autooptimizer loop) and "ship the headline number" (the v1 priority) resolves toward the latter.

### Task T.1: `tracing` console subscriber

Initialize `tracing_subscriber::fmt` with `EnvFilter::from_default_env()` early in `xvn`'s main. Every Intern and Trader call emits a structured span. SQLite `traces` table mirrors the same structure for replay (§9.4 covers the schema).

### Task T.2: SQLite trace verification

Spot-check after a backtest run: every row in `decisions` has a matching row in `traces` keyed on `(run_id, setup_id, stage)`. Mismatches are flight-recorder bugs; fail the demo build if found.

---

## Future additions (post-hypothesis-validation)

These are deferred until the headline Δ-Sharpe claim has been validated. Each is a real follow-on, not v1.

### Scope items cut from v1 (re-add triggers explicit)

- **Multi-asset basket (Tier 2 fix #6).** ETH, SOL, xStocks. Re-add trigger: BTC v1 result is positive and the cluster-cap risk rule needs cross-asset exercise. Concatenate paired returns across assets for the bootstrap.
- **xStocks integration (Mantle tokenized equities).** Re-add trigger: Mantle's xStocks have a programmatic surface that doesn't require a separate executor (current state is unverified for v1; check ecosystem registry).
- **Telemetry crate + OTel + Langfuse.** Re-add trigger: serving load justifies live observability, OR the Karpathy autooptimizer loop ships and needs honest cross-run traces.
- **Telegram bot (`xvision-bot`).** Re-add trigger: post-hackathon polish, demo audience extends beyond the judges' README walkthrough.
- **`mantle-risk-evaluator` LLM pre-flight.** Re-add trigger: Mantle forward-trade volume justifies a second LLM-mediated gate on top of the deterministic risk layer.

### Karpathy autooptimizer loop (deferred)

The Rust orchestrator proposes strategy mutations from per-strategy trade ledgers, validates the resulting variants against the boundary probe corpus, and admits survivors to the loom. Implementation in `crates/xvision-harness/src/karpathy_loop.rs`.

Trigger: anti-overfit gate verdict is `PassesBothRegimes` for at least one strategy variant in v1. Goodhart-resistance comes from the harness, not the loop. **The gate must re-tighten to blocking when this loop ships.**

---

*Document version: 2026-05-07 (post-ADR-0011 reshape). Lives at `/Users/edkennedy/Code/xvision/implementation-plan.md`.*
