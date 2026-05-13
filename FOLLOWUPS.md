# Follow-ups — operational queue

## Active roadmap

The active V2-V4 execution plan now lives in
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.

Use that roadmap for board ordering and phase gates:

| Phase | Theme | Key followup anchors |
|---|---|---|
| V2A | Ease of use sweep: Driver.js tours, in-app docs page, tutorials, examples | F36, F25, onboarding/settings, command palette, agent/CLI discoverability |
| V2B | Security hardening for dashboard, remote CLI, broker, wallet, and testnet actions | F35, F37, F21, remote CLI specs |
| V2C | Blockchain testnet: mint, buy, sell, delegate/license, marketplace, reputation, validation receipts | F5, SLF2-SLF5, SLF8, F34 |
| V3 | Autoresearcher and final UI/UX | SLF9, SLF13, F29, F31-F33, autoresearcher plans |
| V4 | Smart contract go-live off testnet | ADR 0008, smart contract, wallet, and marketplace specs |

The older SLF/F items below are preserved as historical anchors and source
notes. They are not the current execution order.

---

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

### F26 [Shared]. Bump GitHub Actions off Node 20 before the runner deprecation

- **Trigger:** GHA warning surfaced on the 2026-05-11 `docker.yml` workflow_dispatch run (25654716433). Hard deadlines: Node 20 forced to Node 24 by **2026-06-02**; Node 20 binary removed from the runner image **2026-09-16**.
- **Scope:** `.github/workflows/docker.yml` pins five Node-20-based actions: `actions/checkout@v4`, `docker/setup-buildx-action@v3`, `docker/login-action@v3`, `docker/metadata-action@v5`, `docker/build-push-action@v6`. Audit each upstream for a Node-24 / next-major release and bump in lockstep. As a short-term escape hatch we can set `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24=true` at the workflow level to opt in early, or `ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION=true` to defer after the forced flip — both are stopgaps, not the fix.
- **Validation:** trigger `gh workflow run docker.yml --ref main -f dockerfile=Dockerfile.deploy` post-bump, confirm the deprecation annotation is gone and `smoke` still passes.
- **Blocking:** non-blocking until 2026-06-02; becomes deploy-blocking on 2026-09-16 (no Node 20 on the runner → workflow won't start at all).

### F27 [Shared]. Install customizer — interactive module selection on install / upgrade

- **Trigger:** F28 (plugin architecture) lands. Without a plugin contract there is nothing for the customizer to drive.
- **Scope:** `xvn install` / `xvn install --customize` TUI wizard + non-interactive flags + `~/.xvn/install.toml` manifest, per [install-customizer design spec](docs/superpowers/specs/2026-05-11-install-customizer-design.md). Re-entrant add/remove/reconfigure. Generates `.cargo/xvn-features` and `docker-compose.override.yml` from the manifest. Initial registered modules: marketplace, memory (cortex sidecar), autoresearcher. Future plugins auto-appear via the F28 registry — no customizer code change required.
- **Why deferred:** the v1-test surface ships full-default-build; module-selection friction isn't on the critical path until operators start running multiple deployments with different module sets, or the marketplace acquires third-party plugins (see F28).
- **Open extension (spitballed 2026-05-11):** third-party plugins on the marketplace could ship with monetisation wrappers — monthly subscription or streamed-payment (Superfluid / Sablier style) gates that the customizer enforces at install / runtime. Out of scope for v1 of the customizer; capture in F28's plugin manifest schema so payment-gated plugins are representable from day one without a schema break.
- **Blocking:** non-blocking; quality-of-life for fleet operators and a prerequisite for any future plugin marketplace.

### F28 [Shared]. Plugin architecture for xvn — make optional modules pluggable

- **Trigger:** post-v1-test, before the marketplace plugin acquires any third-party participants and before F27 (install customizer) starts.
- **Scope:** define a first-class plugin contract for xvn so optional capabilities (marketplace, memory, autoresearcher, and future modules) plug into the engine through a uniform surface instead of ad-hoc cargo features + bespoke wiring. Likely shape:
  - `PluginManifest` — declarative metadata (id, name, description, category, cargo features, sidecars, config template, env prompts, deps, conflicts, resource cost, monetisation hints).
  - `Plugin` trait (or trait family) — lifecycle hooks: `register(&mut Engine)`, `install(&InstallContext) -> Result<()>`, `uninstall(...)`, `health(&Engine) -> HealthReport`, and the existing per-domain extension surfaces (CLI subcommands, scheduler hooks, dashboard panes, engine API extensions).
  - `PluginRegistry` — discovery (compile-time builtins for v1; filesystem / remote registry in v2), conflict resolution, dependency ordering.
  - Migration of existing optional code into plugin form: marketplace (already has a cargo feature — re-shape into a manifest + plugin impl), memory (per cortex-integration plan — the new `xvision-memory` crate becomes a plugin from the start), autoresearcher (AR-1/2/3 program becomes a plugin so it can be enabled/disabled per deployment).
  - Plugin distribution shape — in v1 plugins live in-tree under `crates/xvision-plugin-*/`. v2 may add out-of-tree plugins discovered at runtime or installed from a remote registry (see monetisation note below).
- **Why now-ish:** the engine has accreted three "optional but real" modules (marketplace, memory, autoresearcher) without a shared contract. Each new one re-litigates the same wiring questions. Lock the shape before a fourth lands.
- **Open extension (spitballed 2026-05-11):** plugins distributed via the marketplace plugin itself, with monetisation envelopes — monthly subscription, streamed payment (e.g. Superfluid / Sablier on Mantle), one-time mint-to-unlock, or per-cycle metered usage. The plugin manifest should carry an optional `monetisation` field from v1 (even if unused by builtins) so payment-gated plugins are representable without a schema break. The marketplace plugin's existing ERC-8004 surface is the natural enforcement point — receipts that gate plugin activation.
- **Blocking:** YES for F27 (install customizer). Non-blocking otherwise; current ad-hoc wiring works for the three in-tree modules.

### F29 [Shared]. Agent social feed — identity-based comms for strategies

- **Trigger:** after F28 (plugin architecture) lands and the marketplace plugin is live with multiple registered strategy identities (ERC-8004 NFTs). Could ship as its own plugin.
- **Scope:** a social-feed surface where each strategy / agent identity posts in-character commentary tied to its actual activity — trade reasoning excerpts, mutation diffs from autoresearch cycles, Reputation Registry receipts, "the funding-rate fader is mad about the funding flip at 04:00 UTC" style takes. Personality / voice driven by a per-identity prompt + the strategy's recent ledger. Feed items are signed by the agent's wallet so the social layer inherits the ERC-8004 trust model — provenance for free, no anonymous trolls. Likely lives as a new plugin (`xvision-plugin-social` or similar) so deployments can run it off; pairs naturally with the marketplace + memory plugins.
- **Why it's interesting beyond the lulz:** identity-bound, on-chain-anchored posts make the strategy population legible as *characters* rather than rows in a table. That's a real product wedge for Persona B (marketplace participants picking strategies to delegate to) — "this lineage has 30 days of consistent vibes + an audit trail" is a stronger sell than a Sharpe number. Also a natural surface for cross-pollination signals (SLF13) — agents publicly reacting to each other's posts.
- **Open questions:** post cadence (per-cycle? per-trade? per-mutation? hand-rolled rate limits to avoid spam) · moderation surface (slashable on toxic / off-spec output? operator override?) · whether posts go on-chain (Reputation Registry with `tag1="social"`) or stay off-chain with hash-anchoring · UI shape (timeline tab in dashboard? standalone web view? RSS / ActivityPub bridge?).
- **Blocking:** non-blocking; pure narrative / community surface. Park here as a "would be funny + actually useful" lane.

### F30 [Shared]. Custom-scenario eval — operator-authored scenarios + Alpaca crypto unlock

- **Trigger:** v1-test surface stabilises; multi-asset eval is the gating gap for credible Persona-B framing.
- **Scope:** see [custom-scenario eval design spec](docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md). Three milestones: M1 bar cache + Alpaca fetcher + asset unlock (drops the BTC-only wall in `xvision-execution/alpaca.rs`); M2 immutable scenarios table + CLI + capital/risk move-off-scenario + canonical seed; M3 dashboard wizard at `/scenarios/new` + inline-form on `/eval-runs` + run launcher. F18 partial pull-in (`TraderDecision.asset`) lands in M1.
- **Blocking:** YES for F31 (replay modes — those extend the scenario `ReplayMode` enum). YES for F32 M1 (chart needs bars in cache). Non-blocking otherwise.

### F31 [Shared]. Scenario replay modes — Stepped, Accelerated, Realtime

- **Trigger:** F30 M2 (scenario table) landed. Stepped + Accelerated can ship anytime after; Realtime is gated on live-paper mirror.
- **Scope:** extend `ReplayMode` enum (already shaped in F30 spec §5) past `Continuous`. Priority order:
  1. **Stepped** — halt after each bar, operator advances. Highest debug value ("why did the strategy do the dumb thing on this bar?"). Adds a per-bar pause channel to the harness.
  2. **Accelerated { speed: f64 }** — N× wall-clock pacing. Useful for demos and watching behaviour without waiting wall-clock time.
  3. **Realtime** — 1× wall-clock pacing. Mostly matters for paper-mirror parity testing. **Gated on live-paper mirror landing** (otherwise unclear what we'd compare against).
- **Why deferred:** the harness already processes bar-by-bar in `Continuous`; the pause/channel + pacing machinery is real work and v1 doesn't need it for scenario-replay correctness.
- **Blocking:** non-blocking individually; collectively they round out the "test like a trader" surface.

### F32 [Shared]. TradingView Lightweight Charts — real charts across six surfaces

- **Trigger:** F30 M1 landed (bar cache populated; chart endpoint can read from it).
- **Scope:** see [TradingView charts design spec](docs/superpowers/specs/2026-05-11-tradingview-charts-design.md). Drops `lightweight-charts@4.x` via npm, ships six chart surfaces (run detail / compare / scenario detail / strategy detail / live cockpit / wizard preview), kitchen-sink server-computed indicator set (SMA/EMA/Bollinger/Donchian/RSI/MACD/ATR — same math the `xvn-mcp` server exposes to agents), multi-pane stack (price + indicators + equity + drawdown + volume), localStorage layer prefs, SSE-streamed live cockpit. Deletes the existing 30-line SVG sparklines (`eval-runs-detail.tsx:221`, `eval-compare.tsx:194`).
- **Why pulled forward:** the operator's "see what my strategy did" surface is currently a glorified sparkline; trader-tool framing requires real charts. Pairs naturally with F30 — F30 makes scenarios runnable on any asset / window; F32 makes the results legible.
- **Blocking:** non-blocking for F30 (F30 ships with the existing SVG charts intact until F32 M1 replaces them).

### F33 [Shared]. TradingView Advanced Charts upgrade — Pine studies + drawing tools

- **Trigger:** post-F32. Requires application/license from TradingView (Lightweight Charts is Apache 2.0; Advanced is free-but-licensed).
- **Scope:** swap `lightweight-charts` for the Advanced Charts library. Unlocks Pine-script studies, manual drawing tools (trend lines, fib drags, channels), multi-chart layouts, custom indicators. The data API + payload shape from F32 mostly carries over (Advanced Charts has a different ingestion shape — UDF or datafeed adapter — but our payload data is the same).
- **Why deferred:** the licensing flow + bundle weight + integration work is meaningfully larger; F32's Lightweight surface already covers 90% of the trader-tool need.
- **Blocking:** non-blocking; pure capability lift.

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

### F34 [SLF]. ERC-8004 reputation leaderboards — gamification surface

- **Trigger:** SLF4 done (per-cycle Reputation Registry writes available) AND SLF10 dashboard rendering at least one ladder view. Without those there's nothing to rank.
- **Scope:** explore the leaderboard product layer on top of ERC-8004 reputation. Brainstorm seed (2026-05-11):
  - **"Most strategies sold"** ranking — counts NFT marketplace sales per agent / per author. Pulls from the per-strategy NFT mint flow (SLF3) + secondary-market events on Mantle.
  - **Other rank axes worth prototyping** — cumulative on-chain PnL (weighted by capital actually deployed, not paper notional), live Sharpe / Calmar, drawdown survival streaks, validation-receipt count (SLF5), genealogy fan-out (SLF8 — "strategies forked from me"), cross-pollination citations (SLF13 — "agents that read my Reputation before mutating"), feedback-velocity (how fast a mutation converges on a positive Reputation score).
  - **Gamification knobs** — seasonal resets vs all-time, capital-tier brackets (sub-1k / 1k–10k / 10k+), risk-bucket brackets (low-vol / mid / high-vol), badge issuance via Reputation Registry `tag1` field (e.g. `tag1="badge:first-100-trades"`), decay curves so dormant agents drop off, sybil-resistance (one author → many agents inflates "most strategies sold" — needs author-id binding via IdentityRegistry).
  - **Optimisation moves** — incentive design (does "most sold" reward novelty or quality? a cheap copy can outsell a winner), anti-gaming guards (wash-trade detection on secondary sales), reputation portability (can an agent move its rep to a new wallet without losing the leaderboard slot?).
- **Why noted:** the ERC-8004 substrate (SLF2/4/5/8/13) gives us the *data*, but the leaderboard / gamification layer is what makes the substrate a *product* people care about. Risk of building the rails and having nothing on them.
- **Blocking:** non-blocking exploration. Pure ideation lane until SLF4 + SLF10 are real. Capture more axes here as they come up.

### F35 [Shared]. Auth on the dashboard API — required before non-Tailscale wide-bind

- **Trigger:** any deployment that needs to bind the dashboard wider than loopback on a network that isn't Tailscale-gated (LAN-only, public cloud, shared Wi-Fi). Also a prerequisite for the mobile PWA + Web Push work in `frontend/MOBILE.md` §9 Phase 5 if that ever ships outside a private tailnet.
- **Scope:** the dashboard currently has no auth on `/api/*` (DESIGN.md §8.4 — same-origin localhost assumption). Vite dev now binds `0.0.0.0` and accepts `*.ts.net` (2026-05-11, so a phone over Tailscale loads the SPA); the production `xvn dashboard serve` still defaults to `127.0.0.1:8788` but the `--bind` flag is documented for wider exposure. As long as the only wider-bind path is Tailscale, the tailnet ACL is the auth layer. If we ever expose past Tailscale, add a real auth layer:
  - Bearer-token middleware on the axum router (`xvision-dashboard/src/server.rs`), tokens issued via `xvn` CLI and stored in `~/.xvn/`.
  - Session cookie + CSRF for browser flows; `same-origin` fetches keep their current shape.
  - Settings page surface to rotate / revoke tokens.
  - Per-route auth scopes once the surface stops being one-user-one-machine.
- **Why noted:** the mobile work and the Tailscale convenience widen the failure-mode surface — easy to forget the trust model when the phone "just works" from anywhere on the tailnet. Document the assumption *and* gate the wide-bind path on auth before the assumption ever stops holding.
- **Blocking:** non-blocking today (Tailscale ACL is the gate). BLOCKING for any non-Tailscale wide-bind deployment.

### F36 [Shared]. driver.js guided tours — in-app onboarding for the dense surfaces

- **Trigger:** after the v1 vertical slice (Phase 1+2 in DESIGN.md §10) is reachable end-to-end and the UI stops moving in big ways. Earliest sensible moment: once Inspector and Eval-runs stop changing weekly.
- **Scope:** add [driver.js](https://driverjs.com/) (~5KB, vanilla, has a thin React wrapper) and ship one tour per dense surface. Candidate tours, ranked by payoff:
  - **Inspector (`/authoring/:id`)** — the densest screen (4-column on desktop, 3-tab on mobile per `frontend/MOBILE.md` §3.3). Walk through: bundle outline → slot editor → live preview → validation rail. Highest payoff; the cost-of-confusion peak.
  - **Run detail (`/eval/runs/:id`)** — what equity / findings / trade ledger actually mean. Especially the "Draft variant from this finding →" affordance, which is non-obvious.
  - **Compare (`/eval/compare`)** — overlay chart + paired KPIs; the comparison semantics aren't visible from the layout.
  - **Setup wizard** — probably *doesn't* need a tour (the wizard already self-explains via chat), unless we add a "what does this Strategy in progress panel mean" first-pass.
  - **Home Control Tower** — short tour pointing at the KPI tiles and the chat composer; low-cost.
- **Implementation knobs:**
  - **Triggering** — first-visit-per-route via a localStorage flag; or always-available via a small `[?]` help button in the topbar that re-opens the current page's tour.
  - **Persistence axis** — localStorage is fine for single-machine; if we later want "I already saw the Inspector tour on my laptop, don't show it on my phone", move the flag into `engine::api::settings` and key it per-user / per-bundle.
  - **Content authoring** — keep tour JSON/TS colocated with the route (e.g. `routes/authoring.$id.tour.ts`) so it doesn't drift from the screen it describes. Lint rule: every tour step references a `data-tour="…"` attribute that must exist in the matching route component.
  - **Mobile** — driver.js works on touch but the spotlight + tooltip pattern needs testing at 390×844. May need to use a "bottom sheet hint" pattern on mobile and the classic popover only on desktop.
  - **Tours vs the chat rail** — the chat rail (DESIGN.md §7) is the long-tail Q&A surface; tours are the up-front "here's what's on this screen" surface. They complement, don't overlap.
- **Why noted:** DESIGN.md Appendix C explicitly ruled out "In-app onboarding tour beyond the first-run wizard" for v1. That's correct for the v1 cut, but the Inspector + Eval surfaces are dense enough that anyone past the user himself will need a guided pass on first encounter, and "go read DESIGN.md" doesn't scale. Cheap to add (driver.js is one of the most lightweight tour libs), incremental (one tour at a time), and removable (a flag-toggle disables all tours if they get in the way).
- **Blocking:** non-blocking. Quality-of-life / onboarding polish.

### F37 [Shared]. Orphan recovery for remote CLI jobs after dashboard restart

- **Trigger:** any operational use of `/api/cli/jobs*` on the tailscale-served dashboard nodes beyond one-shot smoke tests.
- **Scope:** the remote CLI backend now exists in `xvision-dashboard` (`routes/cli.rs`, `cli_jobs/{model,store,runner}.rs`, migration `013_cli_jobs.sql`, and `tests/cli_jobs_routes.rs`), but in-flight jobs are still tracked by an in-memory runner. If the dashboard process restarts, a persisted `running` job can become orphaned because there is no startup sweep equivalent to the eval-run cleanup path in `xvision-dashboard/src/server.rs`.
  - add a startup sweep that marks stale `queued` / `running` CLI jobs as failed-or-cancelled with an explicit restart/orphan reason
  - document the post-restart contract for `GET /api/cli/jobs/:id` and `/output`
  - add at least one restart/orphan integration test once cargo verification is available
- **Why noted:** the agent-access plan originally assumed the remote CLI backend was missing, but execution confirmed the main backend surface already exists. The real remaining backend gap is restart/orphan handling, not route creation or SQLite persistence.
- **Blocking:** non-blocking for tailscale-only private use; blocking before treating remote CLI jobs as a durable operator surface.

### F38 [Shared]. QA6 dashboard remediation — chat, strategies, charts, eval reliability

- **Trigger:** QA6 operator pass on 2026-05-13 found product-state and chart parity regressions across Strategies, chat rail, Settings, eval launch, and Scenario TradingView charts.
- **Scope:** execute [QA6 dashboard remediation](docs/superpowers/plans/2026-05-13-qa6-dashboard-remediation.md):
  - Chat rail: `New chat` preserves previous conversations, restores history, clears composer immediately, and does not reorient unexpectedly on Strategies.
  - Strategies: name-first open form, template optional rather than default, strategy ID secondary, no surfaced `canonical_defaults`, readable strategy names in eval pickers, no `NaN` cadence.
  - Agents/settings: Skills lives under Agents, provider env details are hidden from UI, agent/provider/model pick lists show configured choices, chat rail exposes provider/model discovery tools.
  - Eval: missing scenario parquet/cache failures become actionable preflight errors instead of raw filesystem errors.
  - TradingView charts: Scenario uses the same reusable chart/layer behavior as run/strategy surfaces; timeframe controls affect chart range; indicator choices include the expanded SMA/EMA periods.
  - Performance: audit first-load and chart-heavy routes after the behavioral fixes.
- **Blocking:** YES for treating the dashboard as QA-ready for non-author operators.
