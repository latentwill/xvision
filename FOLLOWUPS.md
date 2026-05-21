# Follow-ups — operational queue

## Active roadmap

The active V2-V4 execution plan lives in
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.

| Phase | Theme | Key followup anchors |
|---|---|---|
| V2A | Ease of use sweep: in-app docs, tutorials, onboarding | onboarding/settings, command palette, agent/CLI discoverability |
| V2B | Security hardening for dashboard, remote CLI, broker, wallet | F37, remote CLI specs |
| V2C | Blockchain testnet: mint, buy, sell, delegate/license, marketplace, reputation, validation receipts | F5, SLF2–SLF5, SLF8, F34 |
| V3 | Autoresearcher and final UI/UX | SLF9, SLF13, F29, F31, F33, autoresearcher plans |
| V4 | Smart contract go-live off testnet | ADR 0008, smart contract, wallet, and marketplace specs |

---

Tactical work deferred during Phase 4–8 implementation. Not strategic
re-examinations (those live in `decisions/strategy-choices.md`); these are
scheduled tasks with a clear trigger or phase that should pick them up.

Format: title → trigger → scope → blocking?

## Track classification (post-2026-05-05 hackathon pivot — see ADR 0010)

After ADR 0010 (Strategy Loom + ERC-8004 marketplace pivot), this queue runs
on two tracks. F4, F14, F15, F17, F23 are superseded by SLF items; control
vector items (F1–F3, F9–F13, F16) are closed — CV substrate moved to
xvision-play per ADR 0011.

| Track | Items | Lives on |
|---|---|---|
| **SLF — Strategy Loom** | SLF1–16 (below) | `main` (post-merge of `pivot/cv-extract`) |
| **Shared** | F5, F6, F7, F8, F18, F19, F20, F22, F24, F26, F27, F28, F29, F30, F31, F33, F34, F37, F38, F39, F40, F41, F42 | `main` |
| **Done** | F25, F35, F36 | — |

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

## Shared queue

Infrastructure used by both tracks. Lives on `main`.

### F5 [Shared]. Orderly testnet credentials + smoke trade

- **Trigger:** Phase 6.3 lands.
- **Scope:** complete brokered onboarding once (`xvn setup --orderly-onboard` per plan §6.3); store `(orderly_key, orderly_secret, orderly_account_id)` in `op` (1Password); place + cancel a small `PERP_BTC_USDC` order against testnet to validate the full path. SDK errors mapped to `ExecutorError`.
- **Blocking:** YES for Phase 11.5 (personal track) and forward delegate-flow demo (hackathon).

### F6 [Shared]. `cycle_id` reuse guard in the harness

- **Trigger:** Phase 9.1 ops crate work.
- **Scope:** harness rejects setups whose `cycle_id` was already cached this run; cache key is `(cycle_id, intern_provider, intern_model)` per Tier 1 fix #1. From `decisions/0005-lookahead-audit.md` follow-up #1.
- **Blocking:** non-blocking; defensive.

### F7 [Shared]. Lookahead-bias boundary-condition test

- **Trigger:** Phase 9.1 ops.
- **Scope:** unit test that constructs a `MarketSnapshot` whose `recent_bars.last().timestamp` is *after* `snapshot.timestamp` (an impossible state); harness should reject the snapshot rather than process it. From `decisions/0005-lookahead-audit.md` follow-up #2.
- **Blocking:** non-blocking; defensive.

### F8 [Shared]. Document `MarketSnapshot` invariants

- **Trigger:** Phase 9.1 ops.
- **Scope:** doc comment on `xvision-core::market::MarketSnapshot` listing the temporal invariants (recent_bars.last().timestamp ≤ snapshot.timestamp; recent_bars chronologically ordered; horizon_hours non-negative). From `decisions/0005-lookahead-audit.md` follow-up #3.
- **Blocking:** non-blocking; documentation hygiene.

### F18 [Shared]. Add `asset: AssetSymbol` to `TraderDecision`

- **Trigger:** multi-asset enabled in `whitelist.toml` (post-headline / post-hackathon).
- **Note:** F30 M1 covers the partial pull-in of `TraderDecision.asset` as part of the Alpaca unlock. The full cascade below is the post-hackathon remainder. Validate scope against `docs/superpowers/plans/2026-05-21-multi-asset-alpaca-unlock.md` before opening a contract.
- **Scope (remaining after F30 M1):** cascade `asset` field through xvision-trader (prompt schema), xvision-intern (briefing format), xvision-risk (drop the separate `asset` parameter), xvision-execution (Alpaca + Orderly stop pinning to BTC), xvision-eval (drop `BacktestConfig.instrument`). Mechanical but wide.
- **Blocking:** YES for full multi-asset.

### F19 [Shared]. Re-adopt `orderly-connector-rs` SDK when its `zeroize` pin loosens

- **Trigger:** `orderly-connector-rs` releases a version that no longer transitively pins `zeroize = "=1.3.0"` (currently 0.4.15 does, via `solana-sdk` → `ed25519-dalek 1.x`). The pin conflicts with `rustls 0.23` (workspace `reqwest 0.13`'s TLS) which needs `zeroize ≥ 1.7`.
- **Current state:** Phase 6.3 reimplements the five required Orderly REST endpoints directly via signed `reqwest` + `ed25519-dalek 2.x` calls. Signing scheme is byte-identical to the SDK's `auth::generate_signature`. Tests cover the path.
- **Scope:** swap the in-house REST shims for SDK calls (`OrderlyService::create_order`, `create_algo_order`, `cancel_order`, `get_account_info`, `get_positions`, `get_futures_info`). Keep the `OrderlyApi` trait so tests stay independent. Strip the local signing code.
- **Blocking:** non-blocking; current implementation is functional.

### F20 [Shared]. Upstream PR: gate Solana stack in `orderly-connector-rs` behind a feature

- **Trigger:** any time before F19's re-adoption (or never, if Orderly upstream fixes it without our PR).
- **Scope:** PR against `ranger-finance/orderly-connector-rs` adding `[features] default = ["solana", "evm"]`, gating Solana deps behind `feature = "solana"`, swapping `ed25519-dalek 1.x` with `ed25519-dalek 2.x` for the `evm` feature, and dropping the `zeroize = "=1.3.0"` exact pin. ~30–50 LoC PR upstream.
- **Blocking:** non-blocking. Worth filing regardless; the wider Rust EVM ecosystem benefits.

### F22 [Shared]. Add `VetoReason::TakeProfitTooTight`

- **Trigger:** any other `VetoReason::Custom(...)` site lands in the codebase.
- **Scope:** one line in `xvision-core::trading.rs` enum + serde rename + cascade through any exhaustive `match VetoReason {...}` — `xvision-risk::rules::take_profit_rr` switches off `Custom("rr_too_low")`.
- **Blocking:** non-blocking; quality-of-enum.

### F24 [Shared]. DeepSeek via OpenAI-compat — default hackathon intern path

- **Status: Zero-code, ready now.** DeepSeek's hosted API is OpenAI Chat Completions wire-compatible. Use existing `OpenAICompatIntern` against `https://api.deepseek.com/v1` with `DEEPSEEK_API_KEY`; `deepseek-reasoner` emits `<think>...</think>` blocks which `strip_reasoning` already handles. Single-shot, deterministic at `temperature=0`. **This is the default intern path for the hackathon submission** — see SLF15 / TraderArm-Off.
- **Long-term note:** Hmbown fork of DeepSeek-TUI at https://github.com/Hmbown/DeepSeek-TUI. At release time, mention in README and consider zh-CN README localization.
- **Blocking:** non-blocking.

### F25 [Shared]. ~~Author a `xvision` Claude Code skill~~ — **DONE**

- **Status: Done.** xvision skill authored and installed. See `.claude/skills/xvision/`.

### F26 [Shared]. Bump GitHub Actions off Node 20 before the runner deprecation

- **Trigger:** GHA warning surfaced on the 2026-05-11 `docker.yml` workflow_dispatch run. Hard deadlines: Node 20 forced to Node 24 by **2026-06-02**; Node 20 binary removed **2026-09-16**.
- **Scope:** `.github/workflows/docker.yml` pins five Node-20-based actions: `actions/checkout@v4`, `docker/setup-buildx-action@v3`, `docker/login-action@v3`, `docker/metadata-action@v5`, `docker/build-push-action@v6`. Audit each upstream for Node-24 / next-major release and bump in lockstep. Short-term escape hatch: `FORCE_JAVASCRIPT_ACTIONS_TO_NODE24=true` at workflow level.
- **Validation:** trigger `gh workflow run docker.yml --ref main -f dockerfile=Dockerfile.deploy` post-bump, confirm deprecation annotation is gone and `smoke` still passes.
- **Blocking:** non-blocking until 2026-06-02; becomes deploy-blocking 2026-09-16.

### F27 [Shared]. Install customizer — interactive module selection on install / upgrade

- **Trigger:** F28 (plugin architecture) lands. F28 is post-hackathon, so this is also post-hackathon.
- **Scope:** `xvn install` / `xvn install --customize` TUI wizard + non-interactive flags + `~/.xvn/install.toml` manifest, per [install-customizer design spec](docs/superpowers/specs/2026-05-11-install-customizer-design.md). Re-entrant add/remove/reconfigure. Generates `.cargo/xvn-features` and `docker-compose.override.yml`. Initial registered modules: marketplace, memory (cortex sidecar), autoresearcher. Future plugins auto-appear via the F28 registry — no customizer code change required.
- **Blocking:** non-blocking; prerequisite for future plugin marketplace.

### F28 [Shared]. Plugin architecture for xvn — make optional modules pluggable

- **Post-hackathon.** Do not open contracts for this before the hackathon submits.
- **Trigger:** post-v1-test, before the marketplace plugin acquires any third-party participants and before F27 (install customizer) starts.
- **Scope:** define a first-class plugin contract for xvn so optional capabilities (marketplace, memory, autoresearcher, and future modules) plug into the engine through a uniform surface instead of ad-hoc cargo features + bespoke wiring. Likely shape: `PluginManifest` (declarative metadata), `Plugin` trait family (lifecycle hooks), `PluginRegistry` (discovery, conflict resolution, dependency ordering). Migration of existing optional code: marketplace, memory (`xvision-memory` crate), autoresearcher. Plugin distribution: v1 in-tree under `crates/xvision-plugin-*/`; v2 remote discovery.
- **Open extension:** plugins distributed via the marketplace plugin itself with monetisation envelopes (monthly subscription, streamed payment via Superfluid/Sablier on Mantle, one-time mint-to-unlock, per-cycle metered usage). Plugin manifest should carry optional `monetisation` field from v1.
- **Blocking:** YES for F27. Non-blocking otherwise.

### F29 [Shared]. Agent social feed — identity-based comms for strategies

- **Trigger:** after F28 (plugin architecture) lands and marketplace is live with multiple registered strategy identities (ERC-8004 NFTs).
- **Scope:** social-feed surface where each strategy/agent identity posts in-character commentary tied to actual activity — trade reasoning excerpts, mutation diffs, Reputation Registry receipts. Feed items signed by agent's wallet. Likely ships as `xvision-plugin-social`.
- **Why it's interesting:** identity-bound, on-chain-anchored posts make the strategy population legible as *characters*. Natural surface for cross-pollination signals (SLF13).
- **Open questions:** post cadence, moderation surface, on-chain vs off-chain with hash-anchoring, UI shape (timeline tab? RSS/ActivityPub bridge?).
- **Blocking:** non-blocking.

### F30 [Shared]. Custom-scenario eval — operator-authored scenarios + Alpaca crypto unlock

- **Status:** implementation planned and executed in the 2026-05-14 Alpaca slices. Remaining work follows PR review/merge feedback.
- **Scope:** see [custom-scenario eval design spec](docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md). Three milestones: M1 bar cache + Alpaca fetcher + asset unlock (drops the BTC-only wall in `xvision-execution/alpaca.rs`); M2 immutable scenarios table + CLI + capital/risk move-off-scenario + canonical seed; M3 dashboard wizard at `/scenarios/new` + inline-form on `/eval-runs` + run launcher. F18 partial pull-in (`TraderDecision.asset`) lands in M1.
- **Blocking:** YES for F31 (replay modes). YES for F33 (chart rework needs bars in cache). Non-blocking otherwise.

### F31 [Shared]. Scenario replay modes — Stepped, Accelerated, Realtime

- **Trigger:** F30 M2 (scenario table) landed. Stepped + Accelerated can ship anytime after; Realtime gated on live-paper mirror.
- **Scope:** extend `ReplayMode` enum past `Continuous`. Priority order: (1) **Stepped** — halt after each bar, operator advances. Highest debug value. (2) **Accelerated { speed: f64 }** — N× wall-clock pacing. (3) **Realtime** — gated on live-paper mirror.
- **Blocking:** non-blocking individually; collectively they round out the "test like a trader" surface.

### F33 [Shared]. Chart rework — replace lightweight-charts with new charting library

- **Context:** lightweight-charts (TradingView) is being removed. A chart rework is planned. This item tracks the replacement.
- **Trigger:** chart rework library/approach decided. Gated on AB-compare surface (see `team/intake/2026-05-19-compare-ab-evaluations.md` — reserved until after charting rework).
- **Scope:** replace the existing `lightweight-charts@4.x` usage across six surfaces (run detail, compare, scenario detail, strategy detail, live cockpit, wizard preview) with the chosen charting library. Preserve: multi-pane stack (price + indicators + equity + drawdown + volume), localStorage layer prefs, SSE-streamed live cockpit, kitchen-sink server-computed indicator set. Update the [TradingView charts design spec](docs/superpowers/specs/2026-05-11-tradingview-charts-design.md) to reflect new library once chosen.
- **Blocking:** YES — AB-compare surface is gated on this landing. Bars-in-cache requirement from F30 M1 carries forward.

### F34 [SLF]. ERC-8004 reputation leaderboards — gamification surface

- **Trigger:** SLF4 done AND SLF10 dashboard rendering at least one ladder view.
- **Scope:** leaderboard product layer on top of ERC-8004 reputation. Rank axes: most strategies sold, cumulative on-chain PnL, live Sharpe/Calmar, drawdown survival streaks, validation-receipt count (SLF5), genealogy fan-out (SLF8), cross-pollination citations (SLF13), feedback-velocity. Gamification knobs: seasonal resets vs all-time, capital-tier brackets, risk-bucket brackets, badge issuance via `tag1` field, decay curves, sybil-resistance. Anti-gaming: wash-trade detection, reputation portability.
- **Why noted:** ERC-8004 substrate gives us the *data*, but leaderboard / gamification makes the substrate a *product* people care about.
- **Blocking:** non-blocking exploration. Pure ideation until SLF4 + SLF10 are real.

### F35 [Shared]. ~~Auth on the dashboard API~~ — **DONE (shipped V2B 2026-05-21)**

- **Status: Done.** Two-layer auth implemented in `xvision-dashboard/src/auth/`. Layer 1: non-loopback gate requiring `XVN_DASHBOARD_TOKEN`; loopback passes unconditionally. Layer 2: per-route DB-backed session tokens (24h TTL) on mutating routes. Remote CLI path exempt when behind Tailscale ACL, with full audit trail. For demo: loopback access works with zero config.

### F36 [Shared]. ~~driver.js guided tours~~ — **DONE**

- **Status: Done.** Guided tours implemented.

### F37 [Shared]. Orphan recovery for remote CLI jobs after dashboard restart

- **Trigger:** any operational use of `/api/cli/jobs*` on tailscale-served dashboard nodes beyond one-shot smoke tests.
- **Scope:** add startup sweep marking stale `queued` / `running` CLI jobs as failed-or-cancelled with explicit restart/orphan reason; document post-restart contract for `GET /api/cli/jobs/:id` and `/output`; add at least one restart/orphan integration test. (Remote CLI backend routes + SQLite persistence already exist in `routes/cli.rs`, `cli_jobs/`, migration `013_cli_jobs.sql`; the gap is restart/orphan handling only.)
- **Blocking:** non-blocking for tailscale-only private use; blocking before treating remote CLI jobs as a durable operator surface.

### F38 [Shared]. QA6 dashboard remediation — chat, strategies, charts, eval reliability

- **Trigger:** QA6 operator pass on 2026-05-13.
- **Scope:** execute [QA6 dashboard remediation](docs/superpowers/plans/2026-05-13-qa6-dashboard-remediation.md). Chat rail (`New chat` preserves history, clears composer immediately); Strategies (name-first form, template optional, no NaN cadence); Agents/settings (Skills under Agents, provider details hidden, configured choices in pickers); Eval (missing parquet/cache failures become actionable preflight errors); Charts (timeframe controls, expanded SMA/EMA periods); Performance (first-load + chart-heavy route audit).
- **Blocking:** YES for treating dashboard as QA-ready for non-author operators.

### F39 [Shared]. Serve chart images over the CLI for evals

- **Trigger:** after eval result/chart payloads stabilize and CLI workflow tracks have landed.
- **Scope:** `xvn eval chart <run_id> --output run.png` for local use; remote CLI/API equivalent; use existing run chart payload as source of truth; include useful defaults for price, decisions, equity, drawdown, active indicators.
- **Why noted:** evals increasingly need to be agent-operable. Copyable chart image lets agents include visual evidence in reports and hand off eval results through CLI-only workflows.
- **Blocking:** non-blocking; quality-of-life.

### F40 [Shared]. Scenario display name + eval provider preflight

- **Trigger:** 2026-05-14 operator pass hit two eval-adjacent workflow failures: scenario creation omitted required display name; Web UI eval launch sent stale `openai` provider, triggering tool-use loop cap.
- **Scope:** execute board tracks `qa8-scenario-display-name-contract` and `qa8-eval-provider-preflight`. Scenario create flows require `display_name` before persistence. Eval launch reads configured providers/models before running, blocks zero-provider/stale-provider states with actionable setup path.
- **Blocking:** blocking for unattended agent use of Web UI eval launcher and wizard.

### F41 [Shared]. Eval contract honesty + agent-graph composition

- **Trigger:** QA rerun on `xvnej-app` 2026-05-21 found LLM never called — every decision returned fixture from `gemini-local` Serveo endpoint; eval shipped `status=completed` regardless; 432/432 forced-hold pattern not surfaced.
- **Scope:** execute tracks in `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`. Two cohorts: (a) eval contract honesty — smell-test for uniform decisions, per-call `(provider, model)` attestation in export, provider preflight before launch, log-spam collapse, skip-LLM-when-no-legal-action; (b) agent-graph composition — formalize `kind` on `AgentRef` (`trader/filter/critic/intern`) with per-kind I/O contracts, Filter emits user-named signals into downstream briefings, strategy declares graph edges that can short-circuit downstream calls. Plus token-efficiency knobs, indicator-tool wiring, scaffolding cleanup, `XVN_CONFIG_PATH` docker-compose papercut.
- **Explicitly out of scope:** anything that authors strategies/agents for the user, anything that gates model choice (`required_models` demoted to informational `attested_with`), pre-computing indicators into briefings (agents request via tool).
- **Blocking:** YES for treating eval results as trustworthy. Every metric in dashboard today is derived from fixture response; tier-0 items (stub detection, provider attestation, preflight) must land before publishing eval numbers as evidence.

### F42 [Shared]. Memory safety + observability (post-V2D follow-ups)

- **Trigger:** V2D agent-memory intake landed (`team/intake/2026-05-21-v2d-agent-memory.md`). Three items promoted from V2D's deferred list.
- **Scope:** execute tracks in `team/intake/2026-05-21-memory-safety-and-observability.md`. Three tracks: (a) `memory-forget-undo-snapshot` — soft-delete + grace period so `xvn memory forget` is recoverable; (b) `memory-provenance-in-decisions-trace` — bind `memory_recall` events to `decision_id` so trace can answer "which memories drove this decision"; (c) `memory-aware-eval-findings` — surface findings that name memory items most likely to have influenced outcome.
- **Explicitly out of scope:** kill bucket from `team/decisions.md` D5 (cross-namespace blending, embedder config UI, memory diff CLI, mem0/Honcho/mempalace adapters, cortex-http sidecar, cross-host sharing, embedding swap CLI) and V3-candidate slips (tool-driven memory, TTL/LRU eviction).
- **Blocking:** non-blocking; safety net + observability over V2D. `memory-forget-undo-snapshot` should land close to V2D.
