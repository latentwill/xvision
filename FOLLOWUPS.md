# Follow-ups — operational queue

Tactical work deferred during Phase 4–8 implementation. Not strategic
re-examinations (those live in `decisions/strategy-choices.md`); these are
scheduled tasks with a clear trigger or phase that should pick them up.

Format: title → trigger → scope → blocking?

## Track classification (post-2026-05-05 hackathon pivot — see ADR 0010)

After ADR 0010 (Strategy Loom + ERC-8004 marketplace pivot), this queue runs
on three tracks. Existing F-numbers are preserved as historical anchors. New
hackathon work uses the **SLF** series.

| Track | Items | Lives on |
|---|---|---|
| **SLF — Strategy Loom** | new SLF1–16 (below); supersedes F4, F14, F15, F17, F23 | `main` (post-merge of `pivot/cv-extract`) |
| **Shared** | F5, F6, F7, F8, F18, F19, F20, F21 (landed partial), F22, F24, F25 | `main` |

Quick navigation: [SLF queue](#strategy-loom-queue-slf) ·
[Shared queue](#shared-queue)

---

## Strategy Loom queue (SLF)

The hackathon sprint queue. Branch: `hackathon/turing`. Submission deadline:
**2026-06-15**. See ADR 0010 + `LatentWill/Xvision/pivot1-strategyloom.md`.

### SLF1. Cut `hackathon/turing` branch + initial scaffolding

- **Trigger:** ADR 0010 ratified (done, 2026-05-05).
- **Scope:** branch off `main`. Commit ADR 0010 + `pivot1-strategyloom.md` (done) + this FOLLOWUPS restructure (done). Smoke `cargo build --workspace` on the new branch to confirm parity with `main`.
- **Blocking:** YES for everything else on the SLF track.

### SLF2. Execute ADR 0008 ops runbook on Mantle Sepolia

- **Trigger:** SLF1 done.
- **Scope:** see `decisions/0008-erc8004-deployment.md`. Deploy `IdentityRegistry` + `ReputationRegistry` to Mantle Sepolia (chain 5003) via Foundry. Update `RegistryAddresses::mantle_testnet()` in `crates/xvision-identity/src/client.rs`. Drop the `#[ignore]` on the integration tests; smoke a register + giveFeedback round-trip.
- **Why pulled forward:** ADR 0008 originally gated this on Phase 11.5 forward Orderly run. The pivot makes ERC-8004 a week-1 dependency. Mainnet still gated on Phase 9 eval clearing per ADR 0008.
- **Blocking:** YES for SLF3, SLF4, SLF5.

### SLF3. Mint per-strategy NFT on `ab_compare` startup

- **Trigger:** SLF2 done.
- **Scope:** extend `xvision-eval::ab_compare` to call `IdentityClient::register` for each Strategy in the active set on run start, persisting `(strategy_name, agent_id, agent_uri)` mapping. `agent_uri` points to a stable manifest (code commit + Strategy adapter type + risk preset). Idempotent — re-runs reuse the existing `agent_id` if the manifest hash matches.
- **Decision:** TraderArm gets one NFT (vectors-on/off/random/orth no longer apply post-ADR-0011). The leaderboard view treats it as a single unit alongside other Strategy implementations.
- **Blocking:** YES for SLF4.

### SLF4. Per-cycle Reputation Registry write path

- **Trigger:** SLF3 done.
- **Scope:** at end of each `ab_compare` cycle, sign + post a performance receipt to the Reputation Registry per strategy: `(value=cycle_pnl_bps, valueDecimals=4, tag1="cycle", tag2=cycle_id, endpoint=https://...full_metrics, feedbackHash=keccak(metrics_blob))`. `xvision-identity::ReputationClient::give_feedback` already wired in ADR 0008 stub.
- **Riskiest seam.** Engine writes → Mantle Sepolia → dashboard reads back. Get end-to-end smoke green before scaling beyond one strategy / one cycle.
- **Blocking:** YES for SLF10 dashboard.

### SLF5. Validation Registry — signed-oracle backtest receipts

- **Trigger:** SLF4 done; held-out backtest cycle implemented (SLF9).
- **Scope:** add `ValidationRegistry` contract to ADR 0008's Foundry deployment. After each evening Karpathy cycle, post a signed-oracle receipt for each kept mutation: `(strategy_id, parent_strategy_id, holdout_window_id, sharpe_delta_bps, mutation_diff_hash)`. v1 uses operator-signed oracle, NOT TEE/zkML. v2 escalates to TEE attestation.
- **Why operator-signed and not TEE/zkML:** scope. TEE setup on Mantle is multi-week. Signed oracle is a credible verification-layer story for hackathon judges and matches the ERC-8004 EIP's "different trust models" framing — reputation systems vs validation, not strictly cryptographic.
- **Blocking:** non-blocking for headline demo, but materially stronger story with it.

### SLF6. Onchain strategy baselines — Nansen / funding / stablecoin / liquidation

(supersedes F14 — pulled forward from "deferred to post-headline" to "week 1 critical")

- **Trigger:** SLF1 done.
- **Scope:** four strategies as `Strategy` impls in `crates/xvision-eval/src/baselines/onchain/`, each consuming `OnchainPanel` fields. Data sourcing: Nansen API key, funding-rate feed (Bybit), stablecoin exchange-flow feed, liquidation feed. See F14 for original strategy descriptions (smart-money copy, funding-rate fader, stablecoin exchange-inflow risk-off, liquidation cascade fader).
- **Why pulled forward:** the marketplace narrative is credibly Mantle-native only if the seed strategy population includes Mantle-native onchain signals. Without these, the loom is "AI mutates classical TA on DEX flow" — generic.
- **Blocking:** YES for the demo's "credibly Mantle-native" framing.

### SLF7. TA baselines — Bollinger / Donchian / Fibonacci / MA-triple

(supersedes F15 — pulled forward to "week 2 critical")

- **Trigger:** SLF6 in progress.
- **Scope:** four more `Strategy` impls in `crates/xvision-eval/src/baselines/ta/`. Pure code work, no new data deps. See F15 for per-strategy parameters.
- **Why pulled forward:** part of the seed population. Loom needs ≥10 strategies to demonstrate selection-and-mutation visually by demo day.
- **Blocking:** non-blocking individually; collectively gating the genealogy hero-shot.

### SLF8. Strategy genealogy — `program.md` versioning + parent hash on chain

- **Trigger:** SLF3 done.
- **Scope:** each strategy variant has a `program.md` (Karpathy autoresearch unit-of-work). Mutations append to a content-addressed log with `(version, parent_hash, mutation_diff)`. The Identity Registry's `agentURI` for a forked strategy points to a manifest containing its `program.md` hash + parent `agent_id`, so the genealogy tree is reconstructable from on-chain state alone.
- **Blocking:** YES for SLF10 genealogy view.

### SLF9. Evening Karpathy loop — wrapper around `xvision-intern`

- **Trigger:** SLF3, SLF4 done.
- **Scope:** new module `xvision-eval::loom::evening_cycle`. Per strategy: read day's trade ledger + `program.md`, ask intern for one mutation, paper-test new variant against held-out window via `ab_compare`, accept (mint new NFT via SLF3, post Validation receipt via SLF5, fork lineage via SLF8) or reject (log to mutation registry as proposed-but-pruned). Bound mutations per night per strategy by `[loom] mutations_per_night = N` in config.
- **Decision:** start with constrained mutation surface — knob-level (size, stops, indicator parameters) + indicator selection — for safety + repeatability. Open up to free-form `program.md` editing in v2 once basic loop is stable.
- **Blocking:** YES for "self-improvement" claim in demo.

### SLF10. Next.js dashboard — live ladder + genealogy + risk-preset delegate

- **Trigger:** SLF4 done (reputation reads available); SLF8 done (genealogy reconstructable).
- **Scope:** new repo `xvision-dashboard` (separate from Rust workspace). Next.js 15 + viem + wagmi. Three views:
  - **Ladder.** Live-updating list of strategies sorted by trailing performance (configurable window). Reads Reputation Registry directly.
  - **Genealogy.** Per-lineage tree of strategy variants, each node clickable for `program.md` diff vs parent + Validation receipt.
  - **Delegate.** Risk-preset selector → filtered strategy list → one-click delegation flow.
- **Auth/wallet:** account abstraction via Privy or Dynamic; one-click social login. Match EasyVault's UX bar.
- **Blocking:** YES for hackathon demo content.

### SLF11. Risk-preset configuration wired to `xvision-risk`

- **Trigger:** SLF10 in progress.
- **Scope:** define `RiskPreset::{Conservative, Balanced, Aggressive}` in `xvision-risk`. Each strategy NFT manifest declares which preset(s) it's compatible with. Dashboard's Delegate view filters on this. Tentative defaults (finalise week 2): Conservative = max 2% per position, no leverage, 30% max drawdown halt; Balanced = 5% / 1× / 50%; Aggressive = 10% / 2× / 70%.
- **Blocking:** non-blocking for demo (defaults work); blocking for the "trust" framing.

### SLF12. `control-vectors` cargo feature — **OBSOLETE per ADR 0011**

- **Status:** closed 2026-05-07. The CV substrate moved to xvision-play
  rather than living behind a cargo feature gate. This item required no
  follow-on work in xvision.

### SLF13. Cross-pollination — agents read other agents' Reputation

- **Trigger:** SLF9 stable; week 4+ if scope holds.
- **Scope:** before proposing a mutation, the intern reads the top-K performing agents' Reputation entries (incl. their `program.md` diffs) and incorporates them as priors. Converts ERC-8004 from "storage" into "learning substrate." Knob: `[loom] cross_pollination_weight = 0.0..1.0` (start own-history-dominant).
- **Blocking:** non-blocking. Cut to v2 narrative slide if week 4 is tight.

### SLF14. SMA(30) and SMA(90) on `IndicatorPanel`

(supersedes F17)

- **Trigger:** SLF6 / SLF7 in progress.
- **Scope:** push 30/90 SMA computation upstream from inline-in-MA-crossover-baseline into `xvision-data::indicators` so multi-strategy lookups don't recompute. Add `sma_30: Option<f64>` and `sma_90: Option<f64>` to `IndicatorPanel`.
- **Blocking:** non-blocking; cosmetic.

### SLF15. Pluggable Trader stage — `TraderBackend` trait

(supersedes F23 — the Strategy Loom IS this)

- **Trigger:** post-hackathon, but the architecture should be designed during the sprint.
- **Scope:** see F23 for original framing. `McpAgentTrader` becomes one `Strategy` impl among many on the marketplace ladder, alongside `TraderArm` and the classical TA / onchain baselines. F23's pairing concern dissolves: every strategy is on its own ledger, no implicit pairing. (Per ADR 0011, the previously-planned `Qwen3VectorTrader` lives in xvision-play and would re-enter as a `Strategy` if a CV-driven trader is later built.)
- **Blocking:** non-blocking for v1 hackathon; the loom can run with TraderArm-Off (DeepSeek-via-OpenAICompat per F24 short-term) as the only intern-driven strategy and onchain/TA baselines as the population.

### SLF16. Demo polish — pitch video, README, submission package

- **Trigger:** week 5.5.
- **Scope:** 90-second pitch video (loom in action + dashboard click-through + headline numbers); README in hackathon submission format (problem → solution → architecture → demo → judging-criteria mapping); submission package on DoraHacks at `dorahacks.io/hackathon/mantleturingtesthackathon2026`.
- **Blocking:** YES for actually submitting on Jun 15.

---

## Control Vector queue — closed (2026-05-07)

Per ADR 0011, the CV substrate moved to xvision-play. The following CVF
items are closed in xvision; their live state continues in xvision-play if
applicable: F1, F2, F3 (partial — TraderArm survives without VectorConfig),
F9, F10, F11, F12, F13, F16, F26, F27, F28, F29, F30, F31, F32.

See `decisions/0011-cv-extraction.md`.

---

## Shared queue

Infrastructure used by both tracks. Lives on `main`.

### F4 [SLF — superseded by SLF3, SLF4]. ERC-8004 manifests for both arms + harness wiring (runtime-optional)

- **Status:** Original framing was "two A/B-arm manifests for the personal-track run." Pivot reframes as "per-strategy NFTs across the marketplace population." See SLF3 (mint per-strategy NFT on `ab_compare` startup) + SLF4 (per-cycle Reputation write path).
- **Original placeholder manifests (vectors_off.agent.json, vectors_on.agent.json) deleted in pivot/cv-extract per ADR 0011.**
- **What still applies post-pivot:** the runtime-optional gating (`identity.enabled = true/false` in `config/default.toml`) carries forward — the harness must run without Mantle credentials when identity is disabled, and `xvision-identity` stays an opt-in workspace member.
- **Blocking:** the *concept* is now in SLF3/SLF4.

### F5 [Shared]. Orderly testnet credentials + smoke trade

- **Trigger:** Phase 6.3 lands.
- **Scope:** complete brokered onboarding once (`xvn setup --orderly-onboard` per plan §6.3); store `(orderly_key, orderly_secret, orderly_account_id)` in `op` (1Password); place + cancel a small `PERP_BTC_USDC` order against testnet to validate the full path. SDK errors mapped to `ExecutorError`.
- **Blocking:** YES for Phase 11.5 (personal track) and forward delegate-flow demo (hackathon).

### F6 [Shared]. `setup_id` reuse guard in the harness

- **Trigger:** Phase 9.1 ops crate work.
- **Scope:** harness rejects setups whose `setup_id` was already cached this run; cache key is `(setup_id, intern_provider, intern_model)` per Tier 1 fix #1. From `decisions/0005-lookahead-audit.md` follow-up #1.
- **Blocking:** non-blocking; defensive.

### F7 [Shared]. Lookahead-bias boundary-condition test

- **Trigger:** Phase 9.1 ops.
- **Scope:** unit test that constructs a `MarketSnapshot` whose `recent_bars.last().timestamp` is *after* `snapshot.timestamp` (an impossible state); harness should reject the snapshot rather than process it. From `decisions/0005-lookahead-audit.md` follow-up #2.
- **Blocking:** non-blocking; defensive.

### F8 [Shared]. Document `MarketSnapshot` invariants

- **Trigger:** Phase 9.1 ops.
- **Scope:** doc comment on `xvision-core::market::MarketSnapshot` listing the temporal invariants (recent_bars.last().timestamp ≤ snapshot.timestamp; recent_bars chronologically ordered; horizon_hours non-negative). From `decisions/0005-lookahead-audit.md` follow-up #3.
- **Blocking:** non-blocking; documentation hygiene.

### F18 [Shared]. Add `asset: AssetSymbol` to `TraderDecision` (resolves choices #1, #4 in `strategy-choices.md`)

- **Trigger:** multi-asset enabled in `whitelist.toml` (post-headline / post-hackathon).
- **Scope:** schema field add + cascade through xvision-trader (prompt schema), xvision-intern (briefing format), xvision-risk (drop the separate `asset` parameter), xvision-execution (Alpaca + Orderly stop pinning to BTC), xvision-eval (drop `BacktestConfig.instrument`). Mechanical but wide.
- **Blocking:** YES for multi-asset.

### F19 [Shared]. Re-adopt `orderly-connector-rs` SDK when its `zeroize` pin loosens

- **Trigger:** `orderly-connector-rs` releases a version that no longer transitively pins `zeroize = "=1.3.0"` (currently 0.4.15 does, via `solana-sdk` → `ed25519-dalek 1.x`). The pin conflicts with `rustls 0.23` (workspace `reqwest 0.13`'s TLS) which needs `zeroize ≥ 1.7`.
- **Current state:** Phase 6.3 reimplements the five required Orderly REST endpoints directly via signed `reqwest` + `ed25519-dalek 2.x` calls. Signing scheme is byte-identical to the SDK's `auth::generate_signature` (Ed25519 over `${ts}${METHOD}${path}${body}`, base64-encoded, secret base58). Tests cover the path; ergonomics of the SDK are gone.
- **Scope:** swap the in-house REST shims for SDK calls (`OrderlyService::create_order`, `create_algo_order`, `cancel_order`, `get_account_info`, `get_positions`, `get_futures_info`). Keep the `OrderlyApi` trait so tests stay independent. Strip the local signing code.
- **Blocking:** non-blocking; current implementation is functional.

### F20 [Shared]. Upstream PR: gate Solana stack in `orderly-connector-rs` behind a feature

- **Trigger:** any time before F19's re-adoption (or never, if Orderly upstream fixes it without our PR).
- **Current state:** F19 documents the workspace-side workaround. The conflict is *not* workspace-specific — `orderly-connector-rs 0.4.15` has no `[features]` section, hard-pulls `solana-sdk = "=1.16.13"` + `solana-client = "=1.16.13"` + `ed25519-dalek 1.0` + `zeroize = "=1.3.0"` even for EVM-only users (the only consumer surface that actually exists for Mantle v1). Anyone in the modern async/rustls Rust ecosystem hits it.
- **Scope:** PR against `ranger-finance/orderly-connector-rs` adding:
  - `[features] default = ["solana", "evm"]` to preserve current behavior.
  - `solana-sdk`/`solana-client`/`solana_vault_cpi` and `ed25519-dalek 1.x` made `optional = true`, gated behind `feature = "solana"`.
  - For the `evm` feature, depend on `ed25519-dalek 2.x` (no zeroize pin); the EVM gateway's Ed25519 signing scheme works under either major.
  - Drop the `zeroize = "=1.3.0"` exact pin; let cargo resolve it.
- **Impact if landed upstream:** F19 collapses to "switch from in-house REST shims to `OrderlyService` calls behind `default-features = false, features = ["evm"]`." ~30–50 LoC PR upstream; tests should cover both `--features solana` and `--features evm` invocations.
- **Blocking:** non-blocking. Worth filing whether or not we want to take F19 ourselves; the wider Rust EVM ecosystem benefits.

### F21 [Shared]. Replace HTTP-backend Intern with an OpenClaw / ACPX agent-harness backend  *(partial — ACPX subprocess backend landed)*

**Landed 2026-05-04:** `AcpxIntern` in `crates/xvision-intern/src/backend.rs` spawns `acpx <agent> exec --file -` (or `acpx --agent "<cmd>" exec --file -` in escape-hatch mode) with a wall-clock timeout, captures stdout, strips ACP markers (`[thinking]/[tool]/[done]`), and runs the result through the shared `parse_llm_response`. Wired into `xvn run-setup` and `xvn ab-compare` via provider strings `acpx` or `acpx:<agent>`. Setup script (`scripts/setup_runpod.sh`) installs Node + acpx and exposes the full ACPX built-in registry (claude / codex / gemini / opencode / cursor / copilot / qwen / kimi / iflow / trae / qoder / kilocode / kiro / droid / openclaw / pi) plus an escape-hatch slot for Hermes Agent — itself an ACP server, reached via `acpx --agent "hermes acp" exec ...`. The underlying agent CLI is NOT auto-installed; auth flows vary.

Hermes Agent (NousResearch) is the OpenClaw successor — its own README documents `hermes claw migrate` from OpenClaw — and it has direct first-class routes to Xiaomi MiMo / Kimi / GLM / MiniMax / Nous Portal that none of the other ACPX agents offer in one place. Because it ships an ACP adapter (`acp_adapter/` in the repo), no separate Rust backend is needed: `XVN_INTERN_ACPX_CUSTOM_CMD="hermes acp"` routes through the same `AcpxIntern` code path.

**Tools (landed 2026-05-04):** new crate `crates/xvision-mcp/` ships a stdio MCP server (`xvn-mcp`) wrapping `xvision-data` indicators as agent-callable tools — `xvn_rsi`, `xvn_sma`, `xvn_ema`, `xvn_bollinger`, `xvn_atr`, `xvn_macd`, `xvn_donchian`, `xvn_fib_retracements`, plus `xvn_health`. Built on rmcp 1.6 (the official Rust MCP SDK) so the wire contract is maintained upstream. The setup script writes `<acpx-workspace>/acpx.config.json` registering xvn-mcp as a stdio MCP server, and ACPX threads `mcpServers: [...]` into every agent session — so Hermes, Claude Code, Codex, OpenCode, and any future ACPX agent inherit the tools without further wiring. Pure compute, stateless, no data root or API keys; preserves backtest pairing because the agent supplies the input series from prompt context. Live API tools (funding rates, onchain panel reads) are deferred until the live data path is solid.

**Still open:** budget/cost telemetry, deterministic-fallback wiring (caller currently falls back manually by switching provider), live-data MCP tools (funding/onchain) once the data layer stabilises, backtest determinism story for agent-harness paths.

- **Trigger:** Phase 9 result is positive and we want to push the Intern's analytical depth before forward paper, OR Phase 11 forward run shows the Intern is the bottleneck on hard setups. SLF9 (evening Karpathy loop) is a major new caller of this path on the hackathon track.
- **Current state:** Phase 2.2 ships `OpenAICompatIntern` and `AnthropicIntern` — both single-shot LLM calls that take a prompt and emit `InternBriefing`. The backend trait surface is interchangeable by design (Tier 1 fix #1 + plan §2.2), so a new backend impl plugs in cleanly without touching the prompt builder, cache, or trader.
- **Open questions to resolve:** harness choice (pinned upstream vs thin home-rolled loop); whether the harness calls out to `xvision-data` for indicator recomputation; cost / latency profile vs single-shot (5–10× wall time and token spend possible — need a budget cap and a fallback to single-shot when budget is hit); determinism for backtest (Tier 1 fix #2) — agent loops with tool use are inherently non-deterministic unless temperature=0 *and* all tool calls are deterministic. Backtest may have to use the simpler single-shot backend even after this lands.
- **Blocking:** non-blocking; pure capability lift. The current single-shot Intern is sufficient for the v1 headline result and for SLF9's evening cycle (proposing one mutation per night does not need an agent loop).

### F22 [Shared]. Add `VetoReason::TakeProfitTooTight` (resolves choice #2 in `strategy-choices.md`)

- **Trigger:** any other `VetoReason::Custom(...)` site lands in the codebase.
- **Scope:** one line in `xvision-core::trading.rs` enum + serde rename + cascade through any exhaustive `match VetoReason {...}` — `xvision-risk::rules::take_profit_rr` switches off `Custom("rr_too_low")`.
- **Blocking:** non-blocking; quality-of-enum.

### F24 [Shared]. DeepSeek-TUI as a reasoning intern — short-term via OpenAI-compat, long-term via Hmbown cargo mirror

- **Trigger:** want DeepSeek's reasoner (R1) or chat (V3.x) line in the Stage 1 Intern slot.
- **Short-term (no code):** DeepSeek's hosted API is OpenAI Chat Completions wire-compatible. Use the existing `OpenAICompatIntern` against `https://api.deepseek.com/v1` with `DEEPSEEK_API_KEY`; `deepseek-reasoner` emits `<think>...</think>` blocks which `strip_reasoning` (`crates/xvision-intern/src/reasoning.rs`) already handles. Single-shot, deterministic at `temperature=0` — the *right* shape for Stage 1 (briefing only, no tool use), and unlike `AcpxIntern` it pairs cleanly for backtest (Tier 1 fix #1). No new backend needed. **This is the default intern path for the hackathon submission** — see SLF15 / TraderArm-Off.
- **Long-term (release-time note):** there's a Cargo-native rewrite/mirror of DeepSeek-TUI at https://github.com/Hmbown/DeepSeek-TUI (Hmbown fork). At release time, mention it in our README and consider shipping a zh-CN README localization pointing zh-CN users at that fork (and at Hermes Agent → Xiaomi MiMo / Kimi / GLM / MiniMax routes via ACPX) — the audience for a Rust-first DeepSeek harness skews heavily zh-CN.
- **What we'd actually have to build to drive DeepSeek-TUI as an *agent* (not just the API):** either (a) ~2–3 days for an external `deepseek-tui-acp-shim` binary that translates ACP ↔ DeepSeek-TUI's existing one-shot mode (plugged in via `XVN_INTERN_ACPX_CUSTOM_CMD`), or (b) ~5–10 days upstreaming an `acp` subcommand into DeepSeek-TUI itself. Skip both unless the agent loop (file I/O, multi-step tool use) starts paying for itself in briefing quality — for Stage 1 it doesn't.
- **Blocking:** non-blocking. Short-term path is zero-code.

### F25 [Shared]. Author a `xvision` Claude Code skill

- **Trigger:** after the GPU headline run lands and the operator surface stops moving every other session. Post-hackathon is also a natural trigger — the SLF surface is fresh tribal knowledge worth capturing.
- **Scope:** package the project's tribal knowledge as a skill so a fresh Claude Code session ramps without grepping. Likely contents:
  - **Setup & ops** — `scripts/setup_runpod.sh` stage map, env-var contract (`.env.local` keys, `XVN_INTERN_*`, `XVN_MODEL_*`), how to resume a half-finished install via `ONLY=<stage>`, the torch/CUDA driver-version pitfalls (cu126 vs cu128), Q4/Q5/Q6/Q8/fp16 selection rationale.
  - **Strategies / arms** — `Strategy` trait surface (`async_trait`-lifted), `TraderArm` as a vanilla LLM-driven Strategy (post-ADR-0011), where new baselines plug in (`crates/xvision-eval/src/baselines/`), how the A/B harness pairs cache keys per `setup_id` (Tier 1 fix #1).
  - **Loom (post-hackathon)** — SLF1–16 outcomes: ERC-8004 mint flow, evening cycle, dashboard data shape.
  - **Intern backends** — when to pick `OpenAICompatIntern` (deterministic, backtest-safe) vs `AnthropicIntern` vs `AcpxIntern` (agentic, forward-paper only) vs the F24 deepseek-via-openrouter path; how to add a new backend.
  - **MCP tool surface** — `xvn-mcp` tools (rsi/sma/ema/macd/bollinger/atr/donchian/fib/health), how `acpx.config.json` advertises them, what's intentionally NOT a tool (live-data — preserves backtest pairing).
  - **Monitoring / reports** — `docs/dashboard.md`, `xvn show-metrics` / `xvn show-decision` / `xvn report`, where reports land (`reports/headline_Q8/<date>.{json,md}`).
  - **Phase-map cheat sheet** — current state of phases 9 / 10 / 11 plus FOLLOWUPS Fn-codes / SLFn-codes so the assistant knows what's blocking what.
  - **Don'ts** — never recommend `AcpxIntern` for backtest pairing; never mock the real DB in integration tests; never commit the unbundled torch wheel back into requirements.txt.
- **Format:** YAML-frontmatter skill under `~/.claude/skills/xvision/` (name, description, triggers) + a body with the cheat sheet + `references/` for longer per-area pages (vectors, intern, mcp, ops, loom). Description must be specific enough that the loader picks it up only on xvision sessions, not every Rust project.
- **Validation:** dry-run a fresh session with `/<task>` against the skill — "extract a conviction vector at layer 32 for Qwen3-32B" should produce the right command without me having to re-explain `--out`-is-a-prefix.
- **Open questions:**
  - User-installable vs project-local (`.claude/skills/xvision/` checked in)? Project-local survives across machines + onboards collaborators; user-installable stays light. Probably both — minimal user skill that points at the project copy.
  - Auto-trigger heuristics: filename patterns (`crates/xvision-*/`), workspace-root marker (`Cargo.toml` containing `xvision-core`)? Description-based discovery is usually enough.
- **Blocking:** non-blocking. Quality-of-life for future sessions; deferred until phase 9 headline + GPU experiment land OR until post-hackathon merge so the contents stop churning.
