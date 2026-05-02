# XIANVEC Implementation Plan (Rust)

> Disposition-encoding control vectors as the experimental subject. Hackathon claim: on a fixed set of trading setups, vectors-on outperforms vectors-off on a pre-committed risk-adjusted return metric (Δ-Sharpe), statistically beyond noise.

The runtime is Rust. The vector-extraction toolchain is Python, invoked offline as a subprocess. Python is a build tool, not a runtime dependency — the production binary has no Python in its process tree.

**See also:** `architecture.md` for the canonical architectural decisions, `steering-vector-architecture.md` for the forward-thinking sibling doc on Glamin patterns and the Karpathy bet, `implementation-plan-python-archive.md` for the previous Python-targeted plan (preserved for provenance).

---

## File structure (Cargo workspace)

**v1 scope decision (2026-05-03):** the workspace is a **single `crates/xianvec-*` tree** for the hackathon. The lodestar / xianvec subtree split documented in earlier drafts is **deferred to v2** (see "Future additions"). The same is true for several other items previously in v1 — see "v1 scope cuts" below the file tree.

```
xianvec/
├── Cargo.toml                    # workspace root
├── rust-toolchain.toml
├── .pre-commit-config.yaml       # cargo fmt / clippy / test
├── Cargo.lock
│
├── crates/
│   ├── xianvec-core/             # types, schemas, config loader, SQLite persistence
│   ├── xianvec-data/             # OHLCV ingest, indicators, onchain signals
│   ├── xianvec-inference/        # candle wrapper + steering hooks + inline FAISS load
│   ├── xianvec-gating/           # entropy gating, alpha schedule
│   ├── xianvec-introspect/       # OPTIONAL layer analytics — required by Phase 0.3
│   ├── xianvec-intern/           # Stage 1 (Claude API or local Qwen-7B)
│   ├── xianvec-trader/           # Stage 2 (vectors active here)
│   ├── xianvec-risk/             # deterministic risk layer
│   ├── xianvec-execution/        # Stage 3: alpaca + orderly executors
│   ├── xianvec-eval/             # backtest harness, baselines, Δ-Sharpe
│   ├── xianvec-harness/          # boundary probes (minimal v1 corpus)
│   └── xianvec-cli/              # clap-based CLI (binary)
│
├── tools/
│   └── extract_vectors/          # Python: repeng-based contrast extractor (offline)
│       ├── pyproject.toml
│       ├── extract_vectors.py
│       ├── tests/
│       └── README.md
│
├── config/
│   ├── default.toml              # runtime config
│   ├── whitelist.toml            # tradeable assets (BTC only for v1)
│   └── risk.toml                 # risk layer rules
│
├── data/
│   ├── decisions.db
│   ├── probes/                   # boundary probe corpus (minimal v1 set, JSON)
│   └── vectors/                  # FAISS .index files + manifest sidecars
│
├── identity/                     # ERC-8004 agentURI manifests
├── notebooks/                    # Python: eval plotting, vector inspection plots
├── .claude/skills/mantle/        # mantle-skills git submodule
├── decisions/                    # ADR-style decision records
│
└── docs/
    ├── architecture.md
    ├── steering-vector-architecture.md
    └── implementation-plan.md    # this file
```

### v1 scope cuts (deferred to v2)

The following items appeared in earlier drafts and are **explicitly out of v1**. Each lives in "Future additions" below with its trigger condition. Cuts made because the unconstrained scope was a 90-day plan being attempted in a 45-day window with one developer:

- **lodestar / xianvec subtree split** — single-tree v1; the lodestar lift is a `git mv` away if/when a second consumer materializes. No `cargo deny` boundary check, no `deny.toml`, no separate `lodestar-*` crates.
- **3 of 4 disposition axes** — v1 ships **Conviction only** as the active axis. Patience, Risk-appetite, and Trend-disposition are extracted (so the contrast pipeline gets exercised) but not active in the headline experiment. Composition, regime-conditioned configs, and `regime_vectors.toml` come post-v1.
- **Multi-asset basket** — v1 runs on **BTC only** (PERP_BTC_USDC on Mantle via Orderly; BTC-USD on Alpaca paper). ETH/SOL return when the 1-axis result is established and the cluster-cap rule needs exercising.
- **Async vector substrate as a separate crate** — for one active vector, the FAISS index loads inline in `xianvec-inference`. The async worker pool / priority queue / cancellation surface returns when the v2 self-improvement loop needs concurrent vector mutations.
- **Full contract-layer crate** — manifest types live in `xianvec-core` for v1 and validate via `serde + garde` at load time. The dedicated `xianvec-contracts` crate with generic `Vector<Layer, Model>` types lands when there's more than one vector slot to police.
- **Geometry crate (`xianvec-geometry`)** — corridors as first-class artifacts are v2. v1 uses a single-anchor entropy gate inline in `xianvec-gating`.
- **Telemetry crate + OTel/Langfuse** — v1 writes a `traces` table in SQLite (§9.4 flight recorder is sufficient for replay). `tracing` + console output for live dev. OTel export, GenAI semantic conventions, and self-hosted Langfuse return post-v1.
- **Telegram bot (`xianvec-bot`)** — v1 demo is CLI + report markdown + plots. Telegram is post-v1 polish.
- **xStocks integration** — Mantle tokenized equities are out of v1 entirely. PERP_BTC_USDC on Mantle via Orderly is the on-chain trade artifact; ERC-8004 NFT mint on Mantle is the on-chain identity artifact (same chain).
- **`mantle-risk-evaluator` LLM pre-flight gate** — v1 trusts the deterministic risk layer for the small forward run. Re-add when Orderly trade volume justifies a second LLM-mediated gate.

What stays in v1: introspection (required by the Phase 0.3 spike), the four experimental control arms (off / on / random / orthogonal — cheap and protects credibility), Alpaca paper for plumbing validation, ERC-8004 identity registration on Mantle, Orderly executor for live Mantle trades (single-chain audit trail), Byreal Agent Skills vendored for the Stage 1 Intern's context, the structural-review Tier 1 fixes, the boundary-probe runner (minimal corpus).

---

## Structural review (2026-05-02) — fixes baked into the tasks below

A pre-build review surfaced ten structural issues that would have suppressed the magnitude or invalidated the credibility of the headline Δ-Sharpe. Every fix is folded into the relevant task; this list is the manifest so the rationale is traceable.

**Tier 1 — material to Δ-Sharpe / CI / divergence credibility**

1. **Intern non-determinism breaks pairing.** Per-arm Claude calls produced different briefings for the same setup. Fix: cache briefings keyed by `setup_id` and run both trader arms against the same cached briefing; set Intern `temperature=0`. *(Phase 2.2, 8.3, 9.2)*
2. **Trader temperature jitter inflates noise.** `temperature=0.4` makes vectors-OFF non-deterministic, polluting both PnL variance and decision-divergence rate. Fix: greedy decoding (`temperature=0`) for both arms in the controlled backtest; sampled decoding only for forward paper. *(Phase 3.1, 4.4, 9.2)*
3. **Backtest portfolio is frozen — risk layer is a no-op.** A fresh `{nav: 10000, open_positions: [], daily_pnl_pct: 0}` per setup means the risk rules are inert. Fix: stateful portfolio tracker in `iter_setups`/`run_backtest` updating NAV, open positions, daily PnL window, loss streak, and ATR across the test window. *(Phase 8.3)*
4. **Setup overlap inflates effective n.** `step=8` with `horizon=16` shares half the forward window across consecutive setups. Fix: `step >= horizon` (default 24); add a block-bootstrap option for time-series-correct CIs. *(Phase 8.2, 8.3)*
5. **Confidence gate read at the `{` token.** The first generated token is the JSON brace; gating its entropy reflects format confidence, not trading conviction. Fix: gate logits at the position immediately after `"action": "` (the `buy`/`sell`/`flat` choice point). *(Phase 4.4)*

**Tier 2 — credibility and statistical power**

6. **Missing experimental controls (random + orthogonal vectors).** `architecture.md` §9.3 commits to three controls; only OFF was implemented. Without random/orthogonal arms, a positive Δ-Sharpe is consistent with "any perturbation activates exploration." Fix: extract a Gaussian-noise vector (matched Frobenius norm) and a basis-orthogonal vector; run as additional arms. *(Phase 4.1, 9.2)*
7. **Per-setup model reload during gating creates hours of pure I/O.** Reloading the quantized model and vectors per setup is unworkable. Fix: log the would-be gate magnitude in backtest but skip the dampened re-run. Confidence gating is a forward-paper-only feature in v1. *(Phase 4.4, 9.2)*
8. **`returns_from_pnl` is path-dependent.** Dividing by trailing equity makes the return series order-dependent; bootstrap permutations corrupt Sharpe. Fix: `pnl_i / nav_initial` (constant denominator); order-invariant. *(Phase 8.1)*
9. **Vector format conversion validation is one-shot.** Q4 quantization can attenuate vector effects 30–60%; verifying with one print statement is insufficient. Fix: re-run the spike's directional-match criterion against the loaded vector through the runtime path as a hard Phase-4 gate. *(Phase 4.3)*
10. **Single-asset eval halves statistical power.** Hardcoded `BTC-USD` while architecture and risk layer assume a basket. Fix: iterate over the whitelist (BTC + ETH + SOL); concatenate paired returns across assets for the bootstrap. Also exercises the cluster-cap path. *(Phase 9.2)*

**Tier 3 — cleanup**

- Risk layer runs twice (pipeline + harness) — pipeline owns risk, harness trusts the decision. *(Phase 8.3, 9.2)*
- Decision divergence defined on `action` only — extend to `(action, direction, size_bucket)`. *(Phase 9.2)*
- Briefing log uses literal `setup_id="ab"` — fix to use real setup_id. *(Phase 9.2)*
- Walk-forward `train` slice generated but unused — v1 takes the delete path; document it. *(Phase 8.4)*
- 50 contrastive pairs/axis is at the low end for a 14B model; bump to 200 (per Mitra). *(Phase 4.1)*
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

### M1. ERC-8004 identity registration (per arm)

Each experimental arm gets its own identity NFT on Mantle. Vectors-OFF registers as one agent, vectors-ON registers as a second, and both post performance updates to the same reputation registry — the comparison is a publicly auditable single-chain experiment.

- Two `agentURI` manifests live in `identity/` (JSON metadata: model, vector config, code commit, contact). Pin to IPFS or HTTPS.
- Mint via the Identity Registry contract (Mantle mainnet) using `alloy`.
- After every closed Orderly position on Mantle, post a reputation update keyed by setup_id and outcome.
- Both NFTs and reputation history become demo evidence.

Implemented in **Phase 6.5**. Must be in place before any forward Orderly run.

### M2. Orderly Network as the on-chain execution path

`orderly-connector-rs = "0.4"` (ranger-finance, MIT, last published 2025-06; M0 confirms it works against the current API). Stage 3 gets a *second* executor alongside Alpaca paper — same `RiskDecision → Stage 3` contract, different downstream tool. A `--executor {alpaca,orderly}` CLI flag selects between them.

Implementation in `crates/xianvec-execution/orderly.rs` constructs an `OrderlyService` against `https://api-evm.orderly.org`, holds `Credentials { orderly_key, orderly_secret, orderly_account_id }` for signed calls, and surfaces SDK methods (`create_order`, `cancel_order`, `get_holding`, `get_positions`, `get_account_info`) through the `Executor` trait. No Node.js runtime dependency, no subprocess shellout.

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

## Phase 0 — Foundation & vector validation spike

The spike is the load-bearing decision: if vectors don't measurably steer Qwen3-14B at Q4 via candle, the architecture has to change before further work. Don't skip.

### Task 0.1: Cargo workspace init (single tree)

Create the workspace from the file structure above as a single `crates/xianvec-*` tree. Each crate starts as a stub with `lib.rs` and one passing test. No lodestar split, no `deny.toml` boundary check — both deferred to v2.

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
faiss            = "0.13"          # faiss-rs binding
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

### Task 0.2: Pull Qwen3-14B locally + candle smoke test

Download Qwen3-14B Q4 weights (HuggingFace mirror). Verify candle loads them and runs a forward pass on M-series Metal.

**`crates/xianvec-inference/src/smoke.rs`:**
```rust
pub async fn smoke_test(model_path: &Path) -> Result<()> {
    let device = Device::new_metal(0).or_else(|_| Device::new_cuda(0)).unwrap_or(Device::Cpu);
    let model = load_qwen3(model_path, &device, Quant::Q4)?;
    let prompt = "The trading signal is";
    let output = generate(&model, prompt, 32, 0.0)?;
    assert!(!output.is_empty());
    Ok(())
}
```

**Acceptance:**
- Forward pass completes in <10s on M-series at Q4
- Output is coherent (not gibberish — visual check)
- Memory footprint <12GB
- If candle's Qwen-3 quantization is rough: fall back to `llama-cpp-rs`, document the decision in `decisions/0001-inference-backend.md`

### Task 0.3: Vector validation spike (CRITICAL GATE) — with introspection mandatory

Verify that disposition-style vectors actually steer Qwen3-14B Q4 *via candle's hook mechanism* before committing the architecture. This is the single most important phase-0 task and the first consumer of `xianvec-introspect`.

**Approach:** Pick one toy axis (e.g. "decisive vs hedging"), generate ~30 contrast pairs in Python (`tools/extract_vectors/spike.py`), extract the vector via repeng, save to FAISS-compatible format, load into candle, run on 20 holdout prompts comparing baseline vs. magnitudes [-2.0, -1.0, 0.0, +1.0, +2.0]. Wrap the steering hook in `IntrospectionHook` with all capture flags enabled.

**Pass criteria (hard, all must hold):**
- Directional match rate ≥80% on holdout (positive magnitude → decisive output, negative → hedged)
- No coherence collapse (output remains parseable as JSON given a JSON-shaped prompt)
- Effect persists at Q4 quantization (this is the core risk)
- MMLU-equivalent capability check (10-question reasoning probe) shows ≤2 point degradation
- **Logit lens at the decision-emit layer shows a clear shift** in decision-token probabilities (`buy`/`sell`/`flat`) consistent with the magnitude direction — not just the output text changing
- **Magnitude sweep is non-monotonic past threshold** per Mitra (effect peaks then degrades or reverses around α ≈ 2)
- **Residual stream norm changes detectably** between α=0 and α=±1 — confirms the vector is actually being applied, not silently dropped
- **Vector–residual cosine** is bounded away from ±1 — confirms we're steering, not just amplifying the existing trajectory

**Fail behavior:** If any criterion fails, do not proceed. Document the outcome in `decisions/0002-spike-validation.md` with the introspection JSON attached, consider larger model (Qwen3-32B at Q3) or full-precision via `transformers` for runtime, or pivot to non-vector approach. The whole project rests on this validation. (The same ADR records the *passing* outcome — pass and fail both write here.)

**`scripts/spike_vector_validation.rs`** runs the test end-to-end with a `--report` flag emitting:
- A summary JSON (pass/fail per criterion)
- A full introspection JSON (per-layer per-magnitude diagnostics)
- A pre-rendered set of plots via the notebook for visual review

**Why introspection is mandatory here:** Without it, a borderline-failing vector can look like a passing one in output text alone. The cvidialog problem (vector seems to do something but you can't tell what or why) gets resolved by direct layer-level measurement, not by stronger prompts.

### Task 0.4: Vendor mantle-skills

Add `github.com/mantle-xyz/mantle-skills` as a git submodule under `.claude/skills/mantle/`. Verify the skill catalog is loadable and contains the expected skills (network primer, address registry navigator, risk evaluator, portfolio analyst, defi operator, tx simulator, openclaw competition).

**Acceptance:**
- Submodule present at correct path
- `git submodule status` clean
- README documents which skills are loaded into Claude project context for which tasks

---

## Phase 1 — Schemas, config, persistence

### Task 1.1: Schema crate (`xianvec-core`)

For v1 single-tree, both substrate types and trading types live in `xianvec-core` (modules: `core::substrate` for `LayerIndex`, `Manifest`, `GenParams`, `Generation`, `InferenceError`, `VectorRef`; `core::trading` for `Action`, `Direction`, `AssetSymbol`, `Regime`, `EvidenceTag`, `DispositionAxis`, `InternBriefing`, `TraderDecision`, `RiskDecision`, `VetoReason`). The module split previews the v2 lodestar-core / xianvec-core boundary so the lift refactor reduces to two `git mv`s and an import-path search-replace.

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
    pub active_vectors: BTreeMap<DispositionAxis, f32>,
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

### Task 1.2: Config loader (`crates/xianvec-core/src/config.rs`)

TOML-backed config (we use TOML over YAML — it integrates with Cargo idioms and `serde` parsing is first-class).

**`config/default.toml`:**
```toml
[runtime]
mode = "backtest"           # backtest | paper | live
executor = "alpaca"         # alpaca | orderly
random_seed = 42

[intern]
backend = "anthropic"       # anthropic | local
model = "claude-haiku-4-5"
temperature = 0.0           # MUST be 0 for backtest pairing (Tier 1 fix #1)

[trader]
model_path = "models/Qwen3-14B-Q4.bin"
temperature = 0.0           # MUST be 0 for backtest (Tier 1 fix #2)
forward_paper_temperature = 0.4

[trader.vectors]
enabled = true
config = "regime_conditioned"   # off | random | orthogonal | regime_conditioned

[backtest]
step = 24                   # >= horizon (Tier 1 fix #4)
horizon = 16
```

**`config/whitelist.toml`** — assets and per-venue symbol mappings (BTC only in v1; xStocks deferred).
**`config/risk.toml`** — risk layer thresholds.

(`config/regime_vectors.toml` was the regime-conditioned magnitude map — deferred to v2 along with multi-axis composition.)

Acceptance: round-trip + validation tests; bad configs produce structured errors not panics.

### Task 1.3: SQLite persistence (`crates/xianvec-core/src/store.rs`)

`sqlx` with compile-time-checked queries. Tables: `setups`, `briefings`, `decisions`, `risk_outcomes`, `executions`, `traces`.

**Key invariants:**
- `briefings` keyed on `setup_id` only (Tier 1 fix #1: same briefing serves both arms)
- `decisions` keyed on `(setup_id, vector_config_hash)` — both arms persist independently
- `traces` mirrors the OTel span structure for offline replay (§9.4 flight recorder)

Acceptance: migrations run cleanly, `sqlx::query!` macros compile-check against the schema, round-trip inserts/queries for every type.

### Task 1.4: Technical indicators (`crates/xianvec-data/src/indicators.rs`)

RSI(14), SMA(20/50/200), EMA(12/26), Bollinger Bands(20, 2σ), ATR(14), MACD(12/26/9), Donchian(20), Fibonacci retracements with rolling-window peak detection.

Use `polars` lazy frames where possible; hand-code the few indicators not in the `ta` crate.

Acceptance: per-indicator unit tests against canonical fixture data (e.g. RSI on a published worked example agrees to 1e-6).

---

## Phase 2 — Stage 1 Intern (`crates/xianvec-intern/`)

### Task 2.1: Intern prompt builder

The Intern emits balanced bull/bear/flat cases — never recommends. The prompt explicitly forbids `candidate_direction` to keep vectors' steering surface clean (§2 architecture).

```rust
pub fn build_intern_prompt(state: &MarketState, mantle_skills: &[Skill]) -> String { ... }
```

Acceptance: snapshot tests against fixed market state inputs; output prompts are deterministic.

### Task 2.2: Intern via Anthropic SDK or local Qwen-7B

Two backends behind a trait:

```rust
#[async_trait]
pub trait InternBackend: Send + Sync {
    async fn brief(&self, prompt: &str) -> Result<InternBriefing>;
}
```

Implementations: `AnthropicIntern` (via `anthropic-sdk` or raw `reqwest`), `LocalIntern` (via `xianvec-inference` with Qwen3-7B). Both set `temperature=0` for the backtest path (Tier 1 fix #1). Output is parsed via `serde_json` + `garde`; on parse failure, retry once with a corrective system message.

**Briefing cache:** keyed on `setup_id`. Both vectors-on and vectors-off arms read the same cached briefing (Tier 1 fix #1 — pairing).

Acceptance:
- Live Anthropic call returns a valid `InternBriefing` for a fixture market state
- Cached briefing reused across paired arms
- Mantle-skill context loaded into Anthropic project context for Mantle-touching setups

---

## Phase 3 — Stage 2 Trader (no vectors yet)

### Task 3.1: Local model loader (`crates/xianvec-inference/`)

`candle`-based Qwen3-14B Q4 loader with hidden-state hooks at configurable layers. The hook signature is the point where steering will eventually live:

```rust
pub trait LayerHook: Send + Sync {
    fn apply(&self, layer_idx: usize, residual: &Tensor) -> Result<Tensor>;
}

pub struct InferenceEngine {
    model: Qwen3Model,
    hooks: BTreeMap<usize, Arc<dyn LayerHook>>,
}

impl InferenceEngine {
    pub fn generate(&self, prompt: &str, params: &GenParams) -> Result<Generation> { ... }
    pub fn install_hook(&mut self, layer: usize, hook: Arc<dyn LayerHook>) { ... }
}
```

For Phase 3, hooks are no-ops (identity function on residual). Phase 4 swaps in real steering hooks.

`temperature=0` (greedy) for backtest paths (Tier 1 fix #2). Sampled decoding only for forward-paper.

### Task 3.2: Trader prompt + JSON-constrained generation

The Trader receives an `InternBriefing` and emits a `TraderDecision`. Use a constrained-generation grammar (or schema validation with single retry) to keep output parseable.

```rust
pub async fn run_trader(
    engine: &InferenceEngine,
    briefing: &InternBriefing,
    portfolio: &PortfolioState,
    params: &TraderParams,
) -> Result<TraderDecision>;
```

Acceptance:
- 95%+ first-pass JSON parse rate on fixture briefings
- 99%+ after one retry
- Output validates against the `garde` schema
- Decision is logged to `decisions` table with `vectors_enabled=false` for this phase

### Task 3.3: Smoke pipeline (Intern → Trader, no vectors)

End-to-end test that runs Stage 1 + Stage 2 on a fixture setup with vectors disabled. Confirms plumbing before vectors enter the picture.

---

## Phase 4 — Vector extraction (Python tool + Rust loader)

### Task 4.1: Contrastive datasets per disposition axis (`tools/extract_vectors/datasets.py`)

**v1 active axis: Conviction only.** Patience, Risk-appetite, and Trend-disposition are extracted (so the contrast pipeline gets exercised end-to-end and survives `cargo test --workspace` against any axis), but they are **not active in the headline experiment**. Multi-axis composition and regime-conditioned configurations are deferred — see "Future additions."

Plus the two experimental control arms applied to the Conviction axis: Random (Gaussian noise, matched Frobenius norm — Tier 2 fix #6) and Orthogonal (basis-orthogonal to the Conviction direction).

200 templated pairs for the active axis (Tier 3 cleanup — bumped from 50 per Mitra). The other three axes can ship at 60–80 pairs each (Mitra's lower bound) since they are pipeline tests, not experiments. Each pair is `(positive_prompt, negative_prompt)` differing on exactly one dimension. Use the template approach from `architecture.md` §7.5 / Mitra:

```python
template = "An analyst evaluates {asset} during {regime}. The analyst's view: {behavior}."

# Conviction axis (the v1 active axis)
positive = [template.format(asset=a, regime=r, behavior="committed and decisive — names the call without hedging") for ... ]
negative = [template.format(asset=a, regime=r, behavior="hedged and conditional — every claim qualified by 'unless' clauses") for ... ]
```

### Task 4.2: Extraction utility (`tools/extract_vectors/extract_vectors.py`)

Python CLI that reads a contrast spec, runs `repeng` against `transformers`-loaded Qwen3-14B (fp16), extracts the steering vector at configurable layers, and writes a FAISS-compatible `.index` file plus a contract manifest sidecar.

```bash
python tools/extract_vectors.py \
  --model Qwen/Qwen3-14B \
  --spec specs/conviction.yaml \
  --layers 20,22,24 \
  --out data/vectors/conviction.index
```

The manifest sidecar (`conviction.manifest.json`) carries `(model_version, embedder_version, layer_id, contrast_pair_set_hash, alpha_curve_hash, derivation_timestamp)` — the contract layer's input.

This utility is invoked offline once per axis. The Karpathy self-improvement loop (Phase 5+) calls the same utility from the Rust orchestrator with model-generated specs.

### Task 4.3: Vector loader (`xianvec-inference::substrate`)

For v1 (single active vector), the loader is a synchronous module inside `xianvec-inference` — not a separate async substrate crate. Reads the FAISS `.index` file + manifest sidecar at startup, validates the manifest against runtime configuration via `xianvec-core::core::substrate::Manifest`, hands the loaded `Tensor` to the gating layer. The async substrate (`VectorStore` with `arc-swap`, worker pool, snapshot reads, priority queuing, cancellation) is deferred to v2 — see "Future additions / Async vector substrate."

```rust
// xianvec-inference/src/substrate.rs (v1 sync loader)
pub struct VectorBundle {
    pub manifest: Manifest,
    pub tensor: Tensor,
}

pub fn load_vector(path: &Path, expected: &Manifest) -> Result<VectorBundle> { ... }
```

Validation gate (Tier 2 fix #9): the loader runs the spike's directional-match criterion against the loaded vector through the runtime path. If the criterion fails on the loaded-into-candle vector, the load is rejected. Quantization-driven attenuation is caught here, not at deploy time.

### Task 4.4: Steering hooks + confidence gating (`xianvec-gating` + `xianvec-inference`)

The candle hook that applies steering. Multiple vectors can apply at distinct layers (per Mitra) with per-vector gating.

```rust
pub struct SteeringHook {
    pub vectors: Vec<(VectorRef, AlphaSchedule, GatingStrategy)>,
}

pub enum GatingStrategy {
    Always,
    EntropyGated { token_set: TokenSet, threshold: f32 },
    CastGated   { condition: ConditionVector, threshold: f32 },
}

impl LayerHook for SteeringHook {
    fn apply(&self, layer_idx: usize, residual: &Tensor) -> Result<Tensor> {
        let mut output = residual.clone();
        for (vec, alpha, gate) in &self.vectors {
            let g = gate.evaluate(layer_idx, &output)?;
            output = output + (g * alpha.current() * vec.tensor())?;
        }
        Ok(output)
    }
}
```

**Tier 1 fix #5:** Entropy gating reads logits at the position immediately *after* `"action": "` — the buy/sell/flat decision token, not the opening `{`. The gate is implemented as a logits inspection during constrained generation, not as a hidden-state hook directly.

**Tier 2 fix #7:** During backtest, gating is *logged but not re-applied* — magnitude is recorded but the model is not re-run with a dampened vector. Re-running per setup is unworkable. Confidence gating is a forward-paper-only feature in v1.

Acceptance:
- Single-vector steering matches the spike result on fixture prompts
- Multi-vector composition at distinct layers shows no coherence regression
- Gating threshold behaviour documented and unit-tested
- Re-injection on long generation (>300 tokens) re-applies the hook
- `IntrospectionHook` wraps cleanly via composition with zero overhead when not installed; verified via `criterion` benchmark comparing wrapped vs. unwrapped hot path

### Task 4.4.1: Introspection hook (`xianvec-introspect`)

Composes around any other `LayerHook` to capture diagnostic data. Opt-in by composition — if you don't install it, you don't pay for it.

```rust
pub struct IntrospectionHook<H: LayerHook> {
    inner: H,
    flags: CaptureFlags,
    capture: Arc<Mutex<CaptureBuffer>>,
}

#[derive(Clone, Copy)]
pub struct CaptureFlags {
    pub residual_norms: bool,
    pub activation_diff: bool,
    pub vector_residual_cosine: bool,
    pub logit_lens: bool,
    pub decision_token_logits: bool,
    pub decision_token_entropy: bool,
}

impl<H: LayerHook> LayerHook for IntrospectionHook<H> {
    fn apply(&self, layer_idx: usize, residual: &Tensor) -> Result<Tensor> {
        let pre_norm = if self.flags.residual_norms { Some(residual.norm()?) } else { None };
        let post = self.inner.apply(layer_idx, residual)?;
        // capture per-layer diagnostics into self.capture according to flags
        Ok(post)
    }
}

impl<H: LayerHook> IntrospectionHook<H> {
    pub fn drain_report(&self) -> InspectionReport { ... }
}
```

Logit lens implementation requires holding a reference to the model's final layer norm and unembedding matrix; `xianvec-introspect` takes those as constructor arguments so it can decode any layer's residual stream into a vocabulary distribution.

CLI tool for ad-hoc diagnostics:

```bash
cargo run -p xianvec-cli -- inspect-vectors \
    --vector data/vectors/conviction.index \
    --magnitudes -2.0,-1.0,0.0,1.0,2.0 \
    --layers 18,20,22,24,26 \
    --prompts data/probes/conviction_test.json \
    --output reports/vector_inspection/conviction_$(date -Iseconds)
```

Output: structured JSON + auto-rendered plots via `notebooks/inspect_vector.py`.

Acceptance:
- Wrapped hot path within 5% of unwrapped on the `criterion` benchmark
- Logit lens output matches manually-computed decoded distributions on fixture residual states
- All capture flags togglable independently
- Report serializes to JSON cleanly for notebook consumption

### Task 4.5: Lookahead bias audit (`decisions/0005-lookahead-audit.md`)

Audit Stage 1 inputs to confirm no future data leaks into briefings. Document the audit and its result.

---

## Phase 5 — Risk Layer (`crates/xianvec-risk/`)

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

Vetoes are logged to `risk_outcomes` with reason. Vetoes are signal — they tell us when vectors push the agent into territory a human risk manager would also reject.

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

### Task 6.2: Alpaca executor (`crates/xianvec-execution/alpaca.rs`)

`apca` (mature Alpaca client; `alpaca-rs` on crates.io is a 0.1.0 stub). Submit market or bracket orders. Read portfolio state after every action and cache for next Stage-1 input.

### Task 6.3: Orderly executor (`crates/xianvec-execution/orderly.rs`)

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

**Credentials.** Orderly uses `(orderly_key, orderly_secret, orderly_account_id)` for signed calls. The `account_id` is derived from a brokered onboarding flow (one-time setup); keys are loaded from `op` (1Password CLI) per workspace convention. `xianvec-cli setup --orderly-onboard` runs the brokered onboarding once and writes the resulting account_id to local config; secrets stay in `op`.

**Acceptance:**
- M0' probe (✅ 2026-05-03) already verified the SDK reaches the live API on Mantle. Phase 6.3 builds the `Executor` impl on top of the proven SDK surface.
- Place + cancel a small `PERP_BTC_USDC` order against the live API with size below the caps in `risk.toml` (or testnet equivalent if Orderly exposes one — check `https://testnet-api-evm.orderly.org` during Phase 6.3).
- `get_account_info` + `get_positions` reads land in the same `PortfolioState` shape Alpaca produces.
- All SDK errors (`OrderlyError`) map cleanly into the executor's error enum without `unwrap()` in the hot path.

### Task 6.4: Backtest simulator

In-process executor that takes `RiskDecision` and walks forward through historical OHLCV applying realistic slippage and fees. Implements the same `Executor` trait so `xianvec-eval` swaps it in transparently.

**Tier 1 fix #3:** Stateful portfolio tracker — NAV, open positions, daily PnL window, loss streak, ATR — updated across the test window. The risk layer must actually fire during backtest.

---

## Phase 6.5 — ERC-8004 identity registration (Mantle hackathon)

Two `agentURI` manifests in `identity/` (vectors_on, vectors_off). Mint via `alloy` against the Identity Registry contract on Mantle mainnet.

```rust
pub struct IdentityClient { provider: Provider, registry: Address }

impl IdentityClient {
    pub async fn register(&self, agent_uri: &Url, signer: &PrivateKeySigner) -> Result<TokenId> { ... }
    pub async fn post_reputation(&self, agent: TokenId, setup_id: Uuid, outcome: TradeOutcome) -> Result<TxHash> { ... }
}
```

Acceptance: both arms have NFTs minted; reputation posts succeed for fixture trades on Mantle testnet before main run.

---

## Phase 7 — Baselines (`crates/xianvec-eval/baselines/`)

Each baseline implements a simple decision rule that consumes the same `MarketState` the Intern sees and emits a `TraderDecision`-shaped output (action + size + direction + stops). They are evaluated by the same backtest harness.

**Null baselines (must beat):** buy-and-hold, random direction with constant 1% sizing, always-long, always-short.

**Classical technicals:** RSI(14) 30/70 mean-reversion, MA(30/90) crossover, MA(30/60/90) triple-confirmation, Bollinger(20, 2σ) mean-reversion, MACD(12/26/9) momentum, Donchian(20) breakout, Fibonacci 38.2/50/61.8 retracements with peak detection.

**Onchain (the real bar):** Nansen smart-money copy-trader, funding-rate fader, stablecoin exchange-inflow risk-off, liquidation cascade fader.

**ML stretch:** XGBoost on technical + onchain features (use `xgboost-rs` or shell out to a Python script — XGBoost training/serving in Rust is workable but unergonomic; if it's a stretch baseline only, the Python escape hatch is fine).

**Experimental controls:** vectors-OFF, vectors-RANDOM (Gaussian noise, matched Frobenius — Tier 2 fix #6), vectors-ORTHOGONAL.

Each baseline outputs to `data/baselines/{name}.parquet` consumed by the eval framework.

---

## Phase 8 — Eval framework (`crates/xianvec-eval/`)

The most important non-obvious piece. Without it, vector improvements cannot be measured.

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
    pub vector_config: VectorConfig,    // off | on | random | orthogonal
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

The gate is computed and **reported with explicit framing**, but in v1 it does **not block** the forward paper run. The original "hard requirement" framing was correct for a deployable trading agent and wrong for a 45-day hackathon: a strict gate combined with weak Q4-attenuated vectors and a 100-trade sample produces a high probability that *no* configuration advances, killing the demo even when honest single-regime evidence exists.

**v1 behaviour:** compute Δ-Sharpe stratified by regime (pre-2023 bear, 2023–2024 bull, plus any other detected regimes in the data). Surface three named verdicts in the report:

- **`PassesBothRegimes`** — positive Δ-Sharpe with CI excluding zero in both. Cleared for forward paper. Headline claim: "vectors generalize."
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

Rationale: the NexusTrade $676 warning still applies — a *self-improvement loop* (Karpathy v2) without the gate will hill-climb into single-regime optima. v1 is not running a self-improvement loop; v1 is one human picking a vector configuration and reporting its limits honestly. The gate's epistemic role is preserved (the report frames the result truthfully); its scheduling role (blocking forward paper) is what gets relaxed.

Re-tightening trigger: any v2 work that adds an automated optimizer over vector configurations or magnitudes — in that mode, the gate must block again to prevent Goodhart.

### Task 8.5: Boundary probes (Glamin pattern formalization)

Curated edge-case corpus in `data/probes/`. Each probe is `{input: SetupSpec, expected: TraderDecision, tolerance: Tolerance}`. Re-run on every model/vector/prompt change; emit decision-flip count, corridor drift, capability-floor delta.

Specifically: ambiguous regime transitions (regime classifier disagrees with itself), low-liquidity setups, hardest historical decisions (manually curated), flash-crash conditions, regulatory edge cases.

```rust
pub struct ProbeRunner {
    corpus: ProbeCorpus,
    harness: Arc<BacktestRunner>,
    introspect: bool,    // when true, every probe runs with IntrospectionHook installed
}

impl ProbeRunner {
    pub async fn run(&self, vector_config: &VectorConfig) -> ProbeReport { ... }
}

pub struct ProbeReport {
    pub pass_rate: f32,
    pub flips_vs_baseline: Vec<DecisionFlip>,
    pub corridor_drift: f32,
    pub capability_floor_delta: f32,
    pub introspection: Option<Vec<InspectionReport>>,    // populated when introspect=true
}
```

**Introspection in probe runs.** Probe runs default to `introspect=true` so every decision flip is captured *with its layer-level mechanism*. This is the Goodhart-resistance step: a vector that game-flips probes by exploiting a quantization artifact will have a different layer signature than one that genuinely shifts the residual stream toward the desired disposition. The harness compares introspection signatures across versions and flags vectors whose signatures look anomalous — sudden cosine spikes, layer-norm explosions, logit lens predictions that contradict the output token, etc. This catches a class of Goodhart failures that pure outcome-based evaluation misses.

---

## Phase 9 — Pipeline orchestration + the A/B experiment

### Task 9.1: Hermes orchestrator (`crates/xianvec-cli/src/hermes.rs`)

Composes Stage 1 (cached briefing per setup) → Stage 2 (paired arms) → Risk → Executor. Logs everything via `tracing` with the GenAI semantic conventions (Phase T.1 telemetry).

### Task 9.2: A/B comparison runner

```bash
cargo run --release -p xianvec-cli -- ab-compare \
  --setups data/setups/2022_2024_paired.parquet \
  --asset BTC-USD \
  --arms off,on,random,orthogonal \
  --output reports/ab_compare/$(date -Iseconds)
```

**v1 single-asset (BTC).** Runs all four arms with `temperature=0` (Tier 1 fix #2) against cached briefings (Tier 1 fix #1), risk layer fires at pipeline scope only (Tier 3 cleanup), decision divergence computed on `(action, direction, size_bucket)` (Tier 3 cleanup), real `setup_id` logged (Tier 3 cleanup). Tier 2 fix #10 (multi-asset basket) is deferred — see "Future additions."

Output: structured JSON consumed by the Python notebook for plots + summary statistics for the demo report.

### Task 9.3: Headline run on rented GPU at higher precision

Q4 quantization attenuates vector effects 30–60% (architecture review). The local M-series dev loop runs Q4 for velocity. **The single headline backtest run for the demo report** runs once on a rented GPU (Vast.ai or RunPod, A40/A100 spot, ~$30) at **Q5_K_M or Q6_K**, same setups, same arms, same seeds. If the Q5 result diverges materially from the local Q4 result, the report leads with the Q5 number and explains the precision dependency; if they agree, the report uses the Q4 number with a footnote that Q5 was checked.

```bash
xianvec-cli ab-compare \
  --setups data/setups/2022_2024_paired.parquet \
  --asset BTC-USD \
  --arms off,on,random,orthogonal \
  --quant Q5_K_M \
  --output reports/ab_compare_headline_Q5/$(date -Iseconds)
```

Provides a defensive moat against the headline number being a Q4 artifact.

---

## Phase 10 — Demo polish

### Task 10.1: Demo CLI commands

`xianvec-cli` gains a small set of demo-supporting subcommands that double as judge-reproducibility entry points:

- `xianvec-cli run-setup --setup-id <uuid>` — runs a single setup end-to-end, prints the briefing, the paired decisions (off/on), the risk verdict, and the would-be execution.
- `xianvec-cli show-decision --setup-id <uuid>` — pretty-prints the cached decision with active vectors and gate metadata.
- `xianvec-cli show-metrics --report <path>` — renders the latest A/B report's headline Δ-Sharpe and dashboard.
- `xianvec-cli explain-vectors` — prints the active disposition axis (Conviction in v1), the manifest hash, the layers it's installed at, and the introspection summary from its last spike validation.

Telegram bot (`xianvec-bot`) is deferred to v2 polish — see "Future additions."

### Task 10.2: Report generator

Renders the headline Δ-Sharpe with 95% CI, the secondary metrics dashboard, regime-stratified results with the named gate verdict (Task 8.4), and the divergence-rate table. Output is a single Markdown file plus the Python-notebook-rendered plots.

The report explicitly states which metrics are inferential (Δ-Sharpe) versus descriptive (MDD, PF, WR) and notes that secondary metrics are not multiple-comparisons-corrected. Where the gate verdict is `SingleRegimeEvidence` or `Fails`, the report leads with that framing rather than burying it.

---

## Phase 11 — Forward paper trading + onchain data

### Task 11.1: Alpaca paper forward run

Run the full pipeline live against Alpaca paper for at least 4–7 days (whatever fits in the schedule after the backtest is in the can — see premortem; this is one of the easiest tasks to lose to clock drift) before any Mantle capital is touched. Both arms (vectors-on, vectors-off) run alternating setups so live data is paired.

### Task 11.5: Orderly forward run on Mantle (M3)

After Alpaca paper validation, switch the executor to `orderly`. Small N (5–20 paired live trades on `PERP_BTC_USDC`) suffices for the on-chain proof — the headline statistical claim still rides on Phase 9's backtest. Each closed Orderly trade emits an ERC-8004 reputation- and validation-registry post on the same chain (Mantle), tagged with the agent NFT, completing the single-chain audit trail.

`mantle-risk-evaluator` LLM pre-flight gate from earlier drafts is **deferred to v2** — v1 trusts the deterministic risk layer for the small forward run. Re-add when forward volume justifies a second LLM-mediated gate.

---

## Phase 12 — Self-review checklist

Acceptance criteria for hackathon submission:

- [x] M0 venue verification passed: Orderly primary (`probes/m0-orderly/`) + Byreal fork option (`probes/m0-byreal/`) — done 2026-05-03
- [ ] All Tier 1 structural fixes verified in code and tests
- [ ] Spike (Phase 0.3) passed with documented evidence in `decisions/0002-spike-validation.md`
- [ ] Conviction vector extracted with manifest sidecar (active axis)
- [ ] Patience / Risk-appetite / Trend-disposition vectors extracted as pipeline tests (not active)
- [ ] Random + orthogonal control vectors extracted on the Conviction direction (Tier 2 fix #6)
- [ ] Backtest harness produces stable results across 3 reruns on identical seeds
- [ ] Q5_K_M headline backtest run completed on rented GPU (Task 9.3)
- [ ] Anti-overfit gate computed and verdict reported (gate is reportable, not blocking — Task 8.4)
- [ ] Δ-Sharpe with 95% CI reported for ≥100 paired trades on BTC-USD
- [ ] Both ERC-8004 identity NFTs minted on Mantle mainnet
- [ ] Byreal Agent Skills + mantle-skills loaded into Claude project context
- [ ] ≥1 Alpaca paper trade closed
- [ ] ≥1 Orderly trade closed on Mantle (`PERP_BTC_USDC`)
- [ ] ≥1 ERC-8004 reputation-registry post per arm on Mantle, tied to a closed Orderly trade
- [ ] Demo report rendered with plots and reproducibility steps (single `cargo run --release` invocation reproduces the headline)

---

## Telemetry (v1: SQLite flight recorder only)

v1 ships the §9.4 SQLite flight recorder plus `tracing` with `tracing-subscriber` printing to stderr in dev. **OTel export, GenAI semantic conventions, self-hosted Langfuse, and Python-extractor span propagation are deferred to v2** — they were appropriate for a deployable serving system and are over-budget for a 45-day hackathon. The decision conflict between "self-improvement loop without traces is just drift" (true, but the v1 scope explicitly does not run a self-improvement loop) and "ship the headline number" (the v1 priority) resolves toward the latter.

### Task T.1: `tracing` console subscriber

Initialize `tracing_subscriber::fmt` with `EnvFilter::from_default_env()` early in `xianvec-cli`'s main. Every Intern and Trader call emits a structured span. SQLite `traces` table mirrors the same structure for replay (§9.4 covers the schema).

### Task T.2: SQLite trace verification

Spot-check after a backtest run: every row in `decisions` has a matching row in `traces` keyed on `(run_id, setup_id, stage)`. Mismatches are flight-recorder bugs; fail the demo build if found.

---

## Glamin pattern formalization (cross-phase index)

The patterns from Glamin we are bringing (per `architecture.md` §7.5 and `steering-vector-architecture.md`) thread through several phases. Index for clarity:

**Corridors as decision boundaries** — v1 vector application uses a single-anchor entropy gate inline in `xianvec-gating`. Full corridor abstraction (separate `xianvec-geometry` crate, multiple anchors, width as a first-class artifact) is deferred to v2.

**Contract layer** — v1 manifest types live in `xianvec-core` and validate via `serde + garde` at vector-load time. The dedicated `xianvec-contracts` crate with `Vector<Layer, Model>` generic types is deferred to v2 (when more than one vector slot needs policing).

**Boundary probes** — Phase 8.5 (above). Minimal curated corpus in `data/probes/`. Versioned, replayable, diffable.

**Document/Geometry separation** — enforced by Cargo dependency graph at the crate level. `xianvec-data` does not depend on `xianvec-inference` / `xianvec-gating`. Cross-transforms are explicit functions in higher crates.

**Async-first vector storage** — for v1's single active vector, the FAISS index loads inline in `xianvec-inference` at startup. The async worker pool / priority queue / cancellation / snapshot-read substrate from earlier drafts is deferred to v2 (when concurrent vector mutation enters the picture, e.g. the Karpathy loop).

**FAISS file format compatibility** — established in Phase 4 from the extraction utility outward. `faiss-rs` reads the canonical `.index` format with sidecar JSON manifests.

---

## Future additions (post-hypothesis-validation)

These are deferred until the headline Δ-Sharpe claim has been validated. Each is a real follow-on, not v1.

### Scope items cut from v1 (re-add triggers explicit)

- **lodestar / xianvec subtree split + `cargo deny` boundary.** Restores the domain-agnostic substrate as a liftable artifact for a future second consumer (EditEngage etc.). Re-add trigger: a second consumer materializes, OR v1 has shipped and there is a 2-week window to refactor. The mechanical lift is `git mv crates/{xianvec-inference,xianvec-substrate,xianvec-contracts,xianvec-geometry,xianvec-gating,xianvec-introspect,xianvec-telemetry} crates/lodestar/lodestar-{...}` plus a path-to-git swap. Earlier draft of this plan retains the lodestar surface sketch in §"Lodestar surface (deferred)" below.
- **3 of 4 disposition axes active.** Patience, Risk-appetite, Trend-disposition. Re-add trigger: Conviction-only v1 produces a positive headline and there is time to compose. Composition risk (Mitra: distinct layers, ≤3 active) applies.
- **Regime-conditioned vector configs (`config/regime_vectors.toml`).** Re-add trigger: at least 2 axes are active and the regime classifier is calibrated against backtest data.
- **Multi-asset basket (Tier 2 fix #10).** ETH, SOL, xStocks. Re-add trigger: BTC v1 result is positive and the cluster-cap risk rule needs cross-asset exercise. Concatenate paired returns across assets for the bootstrap.
- **xStocks integration (Mantle tokenized equities).** Re-add trigger: Mantle's xStocks have a programmatic surface that doesn't require a separate executor (current state is unverified for v1; check ecosystem registry).
- **Async vector substrate (worker pool, snapshot reads, priority queue, cancellation).** Re-add trigger: Karpathy loop or any concurrent-vector-mutation feature.
- **Full contract layer (`xianvec-contracts` crate with `Vector<L, M>` generics).** Re-add trigger: more than one vector slot needs compile-time policing.
- **Geometry crate (`xianvec-geometry` with first-class corridors).** Re-add trigger: a use case that needs more than a single-anchor entropy gate (multi-anchor decision boundaries, named corridors evaluated by the harness, drift quantification).
- **Telemetry crate + OTel + Langfuse.** Re-add trigger: serving load justifies live observability, OR the Karpathy loop ships and needs honest cross-run traces.
- **Telegram bot (`xianvec-bot`).** Re-add trigger: post-hackathon polish, demo audience extends beyond the judges' README walkthrough.
- **`mantle-risk-evaluator` LLM pre-flight.** Re-add trigger: Mantle forward-trade volume justifies a second LLM-mediated gate on top of the deterministic risk layer.

### Karpathy self-improvement loop (Phase 5+ deferred)

The Rust orchestrator generates contrast specs from observed agent behaviour, invokes the Python extractor as a subprocess, validates the resulting vector against the boundary probe corpus, and admits survivors to the active geometry. Implementation in `crates/xianvec-harness/src/karpathy_loop.rs`.

Trigger: anti-overfit gate verdict is `PassesBothRegimes` for v1 Conviction OR at least one manual multi-axis configuration produces a clean result. Goodhart-resistance comes from the harness, not the loop. **The gate must re-tighten to blocking when this loop ships.**

### MemPalace cross-run memory

Semantic retrieval of past run summaries (vector configs, rubric scores, regime conditions, lessons) injected into subsequent runs. Deferred until v1 vector hypothesis validated — adding memory before the hypothesis is confirmed conflates two variables.

### Vector magnitude hill-climbing loop

Automated search over disposition magnitudes using a run → grade → seed-next-run feedback loop. Currently magnitudes are hand-set. NexusTrade $676 warning applies directly: a rubric without the regime gate finds single-regime optima that fail in deployment. Get the gate working and a baseline result first.

### CAST projection-based gating (post-v1)

Replaces the v1 entropy gate with full CAST projection of hidden state onto a condition vector. Implementation in `xianvec-gating` (single-tree v1) or `lodestar-gating` (post-lift) as an additional `GatingStrategy` variant. Trigger: v1 entropy gate is operating cleanly and a vector composition needs richer gating than per-emission entropy provides.

### PID alpha control (post-v1)

Replaces static alpha schedules with a PID controller bounded by hand-set floors and ceilings. Maintains steering across long generations (>500 tokens) where autoregressive conditioning would otherwise wash out the effect.

### Conceptor-based composition (research)

Replaces additive vectors with conceptor matrices supporting Boolean operations (`Buffett AND skeptic AND NOT Livermore`). Path that scales beyond 3 simultaneously active vectors without interference blowup. Read-only research target until v1 establishes whether 2–3 vectors at distinct layers is sufficient.

### YaPO / SAE-guided steering (research)

Learns sparse steering vectors via preference optimization with reportedly zero MMLU degradation and no contrast pairs needed. If real, solves the "we can't write 80 clean pairs for a niche stance" problem. Track the literature; revisit when the v1 contrast-pair pipeline is mature.

---

## Lodestar surface (deferred to v2)

The lodestar substrate sketch from earlier drafts (`lodestar-core` shared types, `lodestar-inference` `InferenceBackend` + `LayerHook` traits, `lodestar-substrate` async `VectorStore`, `lodestar-geometry` generic `Mint`/`Corridor`/`Probe<I, O>`, `lodestar-gating`, `lodestar-introspect`, `lodestar-telemetry`, `lodestar-cli inspect-vectors`) and the EditEngage second-consumer thought experiment that documented the contract from lodestar's side both **lived in this file historically and now live in v2**.

The v1 single-tree implementation gives the same primitives under `xianvec-*` names. When the lift trigger fires (second consumer materializes, OR v1 ships and there's a 2-week refactor window), the post-lift names become `lodestar-*` per the original design. The full surface sketch + EditEngage example are preserved in git history at commit `878adc5` (last revision before this scope cut).

---

## Conversation provenance

The structural review, Mantle integration plan, telemetry hardening, Glamin formalization, and the Rust-from-day-one decision derive from the May 2026 design conversation. Source material:

- Mitra. *Activation Steering in 2026: A Practitioner's Field Guide* — sharpened layer/alpha/composition discipline; established what vectors cannot do.
- Glamin (LynnColeArt) — pattern source, not code dependency. Corridors, contracts, boundary probes, document/geometry separation, async storage, FAISS compatibility — all rebuilt in Rust here.
- OpenTelemetry GenAI semantic conventions — span attribute schema.
- Self-hosted Langfuse, Phoenix (Arize) — observability backends.
- Rust ecosystem: `candle`, `tokio`, `arc-swap`, `faiss-rs`, `serde` + `garde`, `sqlx`, `polars`, `alloy`, `teloxide`, `tracing`.

The Python implementation plan that preceded this version is preserved in `implementation-plan-python-archive.md` for traceability of decisions made before the Rust pivot.

---

*Document version: 2026-05-02. Lives at `/Users/edkennedy/Code/xianvec/implementation-plan.md`.*
