# Turing Hackathon — Evidence-Based Audit & Subagent Task Graph

**Date:** 2026-06-10 · **Demo deadline:** 2026-06-15 · **Scope:** demo spine only
**Method:** 5 parallel read-only exploration agents over the full repo + direct
verification of contested findings by the lead auditor. No code was modified.
**Demo spine:** strategy/agent identity → strategy creation/eval/live-paper run →
prompt-to-decision pipeline → risk gate → execution/receipt → dashboard/marketplace
surface → ERC-8004 / on-chain reputation artifact.

---

## 1. Executive Summary

**Health grade for the hackathon goal: B−.** The off-chain demo spine — strategy
authoring, agent pipeline, decision loop, paper-live execution on Alpaca, SSE live
cockpit, optimizer — is real, wired end-to-end, and demoable today. The on-chain
layer is the inverse: all 8 Solidity contracts (including ERC-8004 Identity/
Reputation/Validation registries) are written and tested, and the Rust client code
(`xvision-identity`, `xvision-marketplace::Erc8004MantleDriver`) is complete — but
**nothing is deployed** (config addresses are all `0x0`, broadcast history is
anvil-only) and **no runtime surface reaches the on-chain code**. The marketplace
dashboard is a polished fixture-driven shell that returns fake tx hashes. The
single biggest risk is not missing code but missing *wiring*: deploy → address
plumbing → one engine call site → one UI link closes the entire on-chain story.
Secondary risks: CI runs only 3 of ~550 test files, so a full local test pass is
the mandatory safety net; and the live paper path lacks the strategy-level risk
vetoes that the backtest path enforces. With contracts deploying today, the
72-hour critical path is: deploy + plumb addresses (day 1), wire the attestation
trigger and identity surface (day 2), rehearse and polish (day 3), leaving June 14
as buffer.

---

## 2. Repo Map

**Purpose:** xvision is an AI-agent trading platform: LLM agent pipelines
("Strategies" composed of `Vec<AgentRef>` over a workspace Agent library) make
per-cycle trading decisions, evaluated against historical data (backtest) or live
paper trading (Alpaca), with an autooptimizer running mutation/tournament loops
over strategies, a strategy marketplace (UI + Solidity contracts), and ERC-8004
on-chain agent identity/reputation on Mantle.

**Maturity:** v0.21.0 pre-1.0, heavily active (multiple agents/day, ~900 PRs).
Off-chain core: production-shaped. On-chain: code-complete, deploy-pending.

**Stack:** Rust workspace (18 crates, tokio/axum/sqlx-SQLite), Vite/React/TS SPA
(embedded into the dashboard binary via rust-embed), Foundry/Solidity contracts
(UUPS, OZ upgradeable), Node sidecar `xvision-agentd` (@cline/sdk 0.0.41, JSON-RPC
over UDS), Docker single-image deploy (~150 MB, includes CLI + SPA + agentd).

### Demo-critical architecture (verified flow)

```
xvn CLI / dashboard POST /api/eval/runs
  └─ xvision-engine api/eval.rs ─ run lifecycle (eval_runs table)
       └─ eval/executor/backtest.rs :: Executor          ← ONE executor, two compositions
            ├─ Backtest: InjectedBars + InstantClock + SimulatedFills
            └─ Live:     MultiLiveStream + WallClock + RealBrokerFills (Alpaca paper)
       per cycle (cycle_id):
            briefing seed → agent/pipeline.rs run_agent_pipeline
              → dispatch_capability.rs (Trader+Router real; Filter/Critic/Intern stubs)
                → execute.rs (LlmDispatch raw HTTP)  OR  execute_cline.rs (agentd sidecar)
              → TraderDecision (xvision-core trading.rs, serde-validated)
            → inline risk handling (sizing + vetoes; see Finding F5)
            → SafetyGate (engine/safety/gate.rs) → BrokerSurface submit
            → decisions/fills persisted; SSE → dashboard /live cockpit
```

### Key directories

| Path | One-liner | Demo relevance |
|---|---|---|
| `crates/xvision-engine` | The core: strategies, agents, pipeline dispatch, eval executor, autooptimizer, safety gate, HTTP API | **Critical** |
| `crates/xvision-core` | Shared types: `TraderDecision`, `RiskDecision`, provider catalog | Critical |
| `crates/xvision-execution` | BrokerSurfaces (Alpaca paper complete; Alpaca/Orderly live stubbed), error classifier | Critical |
| `crates/xvision-dashboard` | Axum server + embedded SPA; routes for strategies/agents/eval/live/optimizer | Critical |
| `frontend/web` | React SPA: live cockpit, optimizer, marketplace (fixture), charts | Critical |
| `contracts/` | Foundry: 8 contracts incl. ERC-8004 registries; testnet deploy script; green tests | **Critical, undeployed** |
| `crates/xvision-identity` | alloy client: NFT register, giveFeedback, attestation bridge. Excluded from default-members (heavy alloy stack); built with `--with-identity` | Critical, **unreached** |
| `crates/xvision-marketplace` | `AnchorDriver` trait: MockDriver (used) + Erc8004MantleDriver (complete, deploy-gated) + Pinata IPFS | High, **unreached** |
| `crates/xvision-cli` | `xvn` verbs: eval run/pause/resume/flatten, strategy, marketplace (mock), optimizer | Critical |
| `xvision-agentd/` | Node Cline-SDK sidecar; baked into deploy image at `/opt/xvision-agentd` | Medium |
| `crates/xvision-risk` | 9-rule RiskLayer — used by xvision-eval baseline harness, **NOT by engine executor** | Medium (see F11) |
| `crates/xvision-{intern,trader}` | Legacy standalone stage-1/2 HTTP wrappers; engine does not import them | Ignore |
| `crates/xvision-{eval,harness}` | Baseline Algorithm A/B harness (separate from engine Executor) | Low |
| `crates/xvision-{memory,filters,mcp,observability,data}` | Cortex memory (opt-in, default Off), filter DSL, MCP server, tracing, market data | Supporting |

### Entry points

- `xvn dashboard serve --bind 0.0.0.0:8788` (Docker CMD) — serves SPA + API.
- `xvn eval run <strategy> --mode live --live-asset … --live-capital …` — live paper run (`crates/xvision-cli/src/commands/eval/mod.rs:688–760`).
- `xvn eval pause|resume|flatten|cancel <run_id>` — run controls (`crates/xvision-engine/src/api/eval.rs:490,498,510`).
- `forge script contracts/script/DeployTestnet.s.sol` — testnet deploy (never yet run).
- `crates/xvision-identity/examples/mint_identity.rs` — op-only identity mint (not surfaced in CLI, per `docs/cli-non-surfaced.md`).

### Conventions worth preserving (from CLAUDE.md, enforced)

Terminology lock (cycle_id/Strategy/Agent; `autooptimizer` never bare `optimizer`);
no popups/modals; no right-side boxes where the chat rail lives; worktree isolation
for all branch work; build via `scripts/cargo` wrapper; deploy via
`scripts/deploy-image.sh` (never cargo/docker-build on remote hosts).

### Surprises

1. The on-chain Rust driver layer is **finished** (all 4 verbs of
   `Erc8004MantleDriver` implemented: `crates/xvision-marketplace/src/adapter.rs:286–434`)
   yet deliberately unreachable: the CLI hard-rejects `MARKETPLACE_DRIVER=onchain`
   (`crates/xvision-cli/src/commands/marketplace.rs:87–97`).
2. The attestation bridge (`crates/xvision-identity/src/attestation.rs`) is real,
   pure-tested code with **no engine call site** — the prompt-to-chain loop is one
   integration away from existing.
3. The engine eval executor does **not** use the `xvision-risk` RiskLayer crate at
   all; risk is inlined from `strategy.risk` config (verified: zero `RiskLayer`
   references in `crates/xvision-engine/src/eval/executor/backtest.rs`).
4. CI runs exactly 3 hand-picked Rust tests + script unittests
   (`.github/workflows/cargo-test.yml`); ~316 Rust + 232 frontend + agentd test
   files never run pre-merge.

---

## 3. Hackathon-Critical Audit Report

Severity is **demo impact**. F = verified fact (file:line read), J = judgment/inference.

### Critical

**F1 — Contracts never deployed; all runtime addresses are zero placeholders.** (F)
- Where: `config/mantle-sepolia.toml` (`identity_registry = "0x0000…"` etc.);
  `contracts/broadcast/` contains only chain-31337 (anvil) runs;
  `contracts/script/DeployTestnet.s.sol:51–126` is written and complete.
- Why it matters: the entire ERC-8004 / marketplace / attestation story is
  undemoable on-chain until this runs. Owner says deploy is happening today —
  this is the #1 critical-path item and everything in Milestone 1 depends on it.
- Env needed: `OPERATOR_EOA`, `USDC_ADDRESS` (USDC.e Sepolia), `LICENSE_URI`,
  optional `XVN_DEPLOYER`, `PROTOCOL_FEE_BPS`. Funded nonce-0 deployer EOA.

**F2 — No automated address plumbing from forge output to runtime config.** (F)
- Where: deploy script prints addresses to console
  (`DeployTestnet.s.sol:166–177`); Rust resolves via env vars
  `MANTLE_TESTNET_IDENTITY_REGISTRY` / `MANTLE_TESTNET_REPUTATION_REGISTRY`
  (`crates/xvision-identity/src/client.rs:229–235`) or returns `None`.
- Why: a manual copy-paste step on the critical path 5 days before demo. One
  typo silently degrades every on-chain call to `NotConfigured`.

**F3 — CI is a 3-test smoke; full suites never run.** (F)
- Where: `.github/workflows/cargo-test.yml` runs `broker_surface_mock_orders`,
  `multi_asset_backtest`, `multi_asset_filter_scope` + python script tests only.
  Frontend (232 vitest files) and `xvision-agentd` tests: zero CI coverage.
- Why: any wiring work this week ships with no automated regression net.
  Milestone 0 (full local test run + smoke script) must precede all wiring.

### High

**F4 — Attestation engine wiring missing (AM4): the ERC-8004 demo artifact has no producer.** (F)
- Where: `crates/xvision-identity/src/attestation.rs` —
  `decide_submission()` / `submit_attestation()` / `build_attestation_outcome()`
  are implemented and unit-tested; **no call site** exists in
  `crates/xvision-engine/src/api/eval.rs` or anywhere in the engine (grep:
  engine never imports xvision-identity).
- Why: without one trigger (e.g. on eval-run finalize), no reputation entry ever
  lands on-chain, and the demo's closing beat (validation artifact) is missing.

**F5 — Live paper path lacks the strategy-level risk vetoes the backtest path enforces.** (F — verified directly by lead auditor)
- Where: veto block (daily-loss kill + max-concurrent-positions, rewrites order
  to hold + records supervisor note + emits `risk_veto` event):
  `crates/xvision-engine/src/eval/executor/backtest.rs:1834–1892` — backtest
  decision path only. Live path `decide_one_live` (`backtest.rs:3341+`) applies
  position sizing (`risk_pct`, line 3547) and the pyramid-flip guardrail
  (~3483–3492) but has **no** daily-loss-kill / max-positions check.
- Mitigations already present on live: per-run pause is fail-closed
  (`backtest.rs:3518`), global SafetyGate with venue-label mismatch + notional
  limits (`crates/xvision-engine/src/safety/gate.rs:85–182`), real-money runs
  blocked entirely (`live_config.rs:231` rejects `VenueLabel::Live`; CLI
  hardcodes Paper at `eval/mod.rs:750`).
- Why: if the demo narrates "risk gate" over a live run, the named vetoes only
  exist on backtest. Either port the block (small, self-contained) or
  demo the risk-veto beat on a backtest/eval run and narrate SafetyGate on live.

**F6 — Marketplace surface is 100% fixture: fake tx hashes, no backend routes.** (F)
- Where: `frontend/web/src/features/marketplace/routes/MarketplaceLayout.tsx:9`
  instantiates `FixtureMarketplaceData`; "DEPLOY WALL" comments at
  `BrowseRoute.tsx:94` and `SellRoute.tsx:46`; no `/api/marketplace/*` routes in
  `crates/xvision-dashboard/src/server.rs`; CLI rejects onchain driver
  (`commands/marketplace.rs:87–97`); wallet hook is a mock (no signer).
- Why: demoing buy/sell without disclosure invites an embarrassing judge
  question ("is that real?"). The UI itself is excellent (200+ fixture listings,
  sell stepper, receipts) — frame as testnet preview, and make exactly one
  artifact real (see strategy Theme 2).

**F7 — Stub `sol!` ABIs not yet pinned to deployed bytecode (AM7).** (F)
- Where: `crates/xvision-identity/src/contracts.rs` (noted stub bindings);
  agent recommendation to pin verified ABIs post-deploy.
- Why: an ABI mismatch surfaces as opaque revert/decode errors during the demo
  window. One anvil-vs-sepolia round-trip (register + giveFeedback) after
  deploy de-risks this in under an hour.

**F8 — Genart divergence (AM2): on-chain tokenURI image ≠ dashboard preview.** (F)
- Where: Rust SVG generator `crates/xvision-identity/src/genart.rs` vs frontend
  canvas `frontend/web/src/features/marketplace/components/GenArtPlaceholder.tsx`;
  same seed renders different art.
- Why: if the demo shows an agent NFT in a wallet/explorer next to the
  dashboard card, they won't match. Pick a canonical renderer or avoid showing
  both side-by-side.

### Medium

**F9 — No identity/NFT surface in the UI.** (F) `GET /api/settings/identity`
exists (`crates/xvision-dashboard/src/routes/settings/identity.rs`) but no
frontend page renders it; agent NFT data exists in marketplace types only.
A small inline identity strip (tokenId + Mantlescan link + latest attestation tx)
is the cheapest way to make the on-chain layer *visible*.

**F10 — Stop is not atomic: `cancel` does not flatten positions.** (F)
`flatten` (`api/eval.rs:510`, executor `backtest.rs:3743+`) and `cancel`
(`api/eval.rs:438–482`) are separate calls; the live-trading spec §2.7 calls for
close-then-terminate. Demo-safe if the operator knows the order; a composite
"stop" action is a quick win.

**F11 — Two disconnected risk systems.** (F) `xvision-risk::RiskLayer` (9-rule
pipeline, used by the `xvision-eval` baseline harness at
`crates/xvision-eval/src/harness.rs:269` and `xvision-harness`) vs the engine
executor's inline `strategy.risk` checks. Confusing for anyone wiring "the risk
gate"; do NOT attempt to unify before the demo — just know which one the demo
path uses (the inline one).

**F12 — USDC.e EIP-3009 support on Mantle Sepolia unverified.** (J)
`Marketplace.buyWithAuthorization` (x402 path) assumes `transferWithAuthorization`
exists on USDC.e; never probed. Fallback approve+`buy()` (2-tx) is implemented.
Don't stake the demo on x402.

**F13 — Dashboard auth hardening incomplete.** (F) Two
`qa-dashboard-auth-hardening` TODOs in
`crates/xvision-dashboard/src/routes/agent_runs.rs`; mutating routes are
auth-gated, GETs are not. Fine on a private/Tailscale demo host; do not expose
the dashboard publicly during the hackathon.

### Low

**F14 — Uncommitted working tree is safe but should be snapshotted.** (F)
15 modified files = formatting + `mutation_idx` threading in autooptimizer
(tests updated to match); `cargo check --bin xvn` passes; openzeppelin submodule
pointer moved (identity-only path). Commit or stash before wiring begins so
subagents start from a clean, tagged baseline.

**F15 — Unwired frontend features `onboarding/` and `cli-jobs/`; unwired
`bybit.rs` executor.** (F) Not referenced from `routes.tsx` / no entry point.
Zero demo impact; leave untouched.

### Strengths to preserve

- **Safety architecture**: SafetyGate (pause, venue-label mismatch, limits,
  audit trail), fail-closed live pause, real-money hard block, broker error
  classifier with recoverable/fatal distinction, no hardcoded secrets anywhere,
  log redaction module (`xvision-dashboard/src/redact.rs`).
- **Live cockpit**: single lifted SSE stream per run, positions table, transport
  controls with optimistic updates (`frontend/web/src/features/live/LiveCockpit.tsx`).
  This is the demo's centerpiece and it works.
- **Contract quality**: full unit+integration suites incl. end-to-end
  `SaleFlow.t.sol` (mint → list → attest → buy → license), UUPS upgrade tests,
  atomic reputation-gate wiring at listing create
  (`ListingRegistry.sol:162–176` → `ReputationRegistry.sol:95–121`),
  free+transferable mint-loop forbid (`ListingRegistry.sol:138`).
- **Self-contained deploy image**: one Docker image = CLI + SPA + agentd +
  config seeds; `scripts/deploy-image.sh` with preflight checks.
- **Optimizer with real SSE** — a genuinely live "AI improving AI" loop to show.

---

## 4. Orphaned / Disconnected Code Map

### Useful and orphaned — wire these (the cheap wins)

| Code | State | Wire-in cost |
|---|---|---|
| `Erc8004MantleDriver` (`crates/xvision-marketplace/src/adapter.rs:186–434`) | All 4 verbs (publish/buy/attest/revoke) implemented; blocked only by addresses + the CLI rejection at `commands/marketplace.rs:87–97` | S–M after deploy |
| Attestation bridge (`crates/xvision-identity/src/attestation.rs`) | Pure, tested; no engine call site | M (one trigger in `api/eval.rs` finalize) |
| `IdentityClient::register/post_reputation*` (`crates/xvision-identity/src/client.rs:380–520`) | Real alloy code; reachable only via `examples/mint_identity.rs` | S (op runbook) — already fine for demo |
| `GET /api/settings/identity` | Backend exists, no UI page | M (inline strip, no popup) |
| `PinataDriver` (`crates/xvision-marketplace/src/ipfs.rs:40–151`) | Production-ready, needs JWT env | S if listing content pinning desired |

### Ignore or hide for the hackathon

| Code | Why |
|---|---|
| `crates/xvision-intern`, `crates/xvision-trader` | Legacy standalone HTTP wrappers; engine has its own dispatch. Do not touch, do not demo `xvn run-setup`. |
| Filter/Critic/Intern capability stubs (`agent/dispatch_capability.rs:13–17`), graph edge predicates (Phase B) | Compile-safe stubs; don't route a demo through them. |
| `frontend …/onboarding/`, `…/cli-jobs/` | Not routed; invisible already. |
| `crates/xvision-execution/src/bybit.rs`, `AlpacaLiveSurface`/`OrderlyLiveSurface` stubs | Unreachable/stubbed; blocked by the VenueLabel gate anyway. |
| `DeployMainnet.s.sol` | Deliberately reverts (V4-gated). |
| Wallet connect mock (`/settings/wallet`) | No real signer; either hide the button or label "testnet preview". |
| Chart-lab routes, mechanistic decision mode, cortex memory default-enable | Off the demo path; leave dormant. |

---

## 5. Improvement Strategy (4 themes)

### Theme A — Close the on-chain loop with ONE real artifact
- **Failure pattern:** 95% coded / 0% deployed / 0% reachable. Three independent
  gaps (deploy, address plumbing, engine trigger) each individually small.
- **Target by demo day:** contracts live on Mantle Sepolia; platform agent NFT
  minted; at least one *real* attestation/reputation entry posted from a real
  eval run, viewable on Mantlescan and linked from the dashboard.
- **Principle:** a single verifiable on-chain artifact beats ten mocked screens.
  Judges can click a Mantlescan link; they can't click a fixture.
- **What NOT to fix:** mainnet path, timelock/multisig, x402 settlement, real
  wallet signer in the SPA, subgraph/indexer (AM6). All post-hackathon.
- **Done signal:** a Mantlescan URL showing a `giveFeedback` tx whose `tag2` is a
  cycle/run id produced during a rehearsal run.

### Theme B — Safety net before wiring (tests + smoke script)
- **Failure pattern:** CI covers ~1% of tests; subagents wiring under deadline
  with no regression net is how demos die the night before.
- **Target:** full `cargo test --workspace`, frontend `pnpm test`, agentd
  `pnpm test` pass locally and are re-run after every merged wiring task; a
  scripted end-to-end smoke (launch dashboard → start live paper run → see
  decisions via SSE → pause → flatten → cancel) that takes <10 min.
- **Principle:** the smoke script *is* the demo rehearsal — every run of it is a
  dress rehearsal.
- **What NOT to fix:** CI itself (don't burn time on Actions config; run locally).
- **Done signal:** smoke script green twice in a row on the demo host image.

### Theme C — Make the spine visible end-to-end in the dashboard
- **Failure pattern:** the pipeline works but its on-chain tail and risk beats
  are invisible: no identity page, risk vetoes only narratable on backtest.
- **Target:** live cockpit demo + an inline identity/attestation strip
  (tokenId, Mantlescan links) + risk veto visibly firing at least once
  (supervisor note / `risk_veto` event in the activity surface).
- **Principle:** demo what exists; surface, don't build. Respect the no-popup /
  no-right-rail rules — inline strips only.
- **What NOT to fix:** marketplace backend routes, charts B1–B4 real builders,
  the two-risk-system unification (F11).
- **Done signal:** one unbroken screen-recording: create/pick strategy → start
  paper run → decision streams in → risk/safety beat → fill → identity strip
  shows the on-chain artifact.

### Theme D — Honest framing of fixtures; risk parity on live (small)
- **Failure pattern:** fake tx hashes presented as real; "risk gate" claimed on
  a path that lacks the vetoes.
- **Target:** marketplace demoed as "testnet preview" (badges already exist);
  either port the backtest veto block (F5) to `decide_one_live` or script the
  demo so risk beats run on backtest. Composite stop (flatten+cancel) wired.
- **Principle:** at a hackathon, one discovered fake costs more credibility than
  ten disclosed WIPs.
- **What NOT to fix:** real marketplace buys, wallet signing.
- **Done signal:** demo script contains zero claims that a judge could falsify
  by clicking.

---

## 6. Subagent Task Plan

Conventions for ALL tasks: work in `.worktrees/<name>` (enforced by pre-commit
hook), `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"`, build via
`scripts/cargo`, never touch `crates/xvision-{intern,trader}`, never rename
autooptimizer/optimizer tokens, no popups/right-rail UI, source `.op_env` before
`gh`/`op`. Conflict zones below are per-task.

### Milestone 0 — Safety net (start immediately, parallel-safe)

**T0.1 — Full local test baseline** · Effort S–M · Risk none · Required
- Objective: establish pass/fail truth before any wiring.
- Commands: `scripts/cargo test --workspace` (includes xvision-identity);
  `cd frontend/web && pnpm test`; `cd xvision-agentd && pnpm i && pnpm test`;
  `cd contracts && forge build && forge test`.
- Acceptance: a written report of failures (if any) filed before M1/M2 tasks merge.
- Conflicts: none (read/execute only).

**T0.2 — Demo smoke script** · Effort S · Risk low · Required
- Objective: one script (`scripts/demo-smoke.sh`, new file) that: starts
  `xvn dashboard serve`, creates/uses a known strategy, launches
  `xvn eval run --mode live --live-asset … --live-time-limit-secs 300`,
  polls `/api/agent-runs/:id`, exercises pause → resume → flatten → cancel,
  asserts decisions > 0 and HTTP 200s.
- Acceptance: exits 0 end-to-end on the deploy image (`docker run … xvision:deploy-<sha>`).
- Verification: run twice; second run must also pass (idempotence).
- Conflicts: new file only; touches nothing existing.

**T0.3 — Snapshot working tree** · Effort S (quick win) · Risk low · Required
- Objective: commit the 15 formatting/mutation_idx files (verified safe,
  `cargo check` green) or stash them; tag `hackathon-baseline-2026-06-10`.
- Note: main-checkout commit needs `XVISION_ALLOW_MAIN_COMMIT=1` or do it from a
  worktree per repo rules. Human approval recommended since auto-commit policies exist.
- Conflicts: coordinate with any agent mid-flight on autooptimizer files.

### Milestone 1 — Critical demo blockers (sequential where noted)

**T1.1 — Deploy contracts to Mantle Sepolia + capture addresses** · Effort M · Risk medium · Required · **Blocks T1.2–T2.3**
- Objective: run `forge script contracts/script/DeployTestnet.s.sol --broadcast --rpc-url https://rpc.sepolia.mantle.xyz` with env `OPERATOR_EOA`, `USDC_ADDRESS`, `LICENSE_URI`, `PROTOCOL_FEE_BPS`; pre-fund deployer EOA.
- Acceptance: 8 non-zero addresses captured into `config/mantle-sepolia.toml`
  (replacing the `0x0` placeholders) and committed; broadcast JSON for chain 5003
  exists under `contracts/broadcast/`.
- Verification: `cast code <each-address> --rpc-url …` non-empty; addresses in
  TOML match broadcast file.
- Owner-in-the-loop: needs the funded operator key — likely human-driven with an
  agent writing the runbook + config commit.
- Conflicts: `config/*.toml` single-writer during this task.

**T1.2 — On-chain round-trip sanity + ABI pin** · Effort M · Risk medium · Required · Depends T1.1
- Objective: using `crates/xvision-identity/examples/mint_identity.rs` (or a
  thin new example), register the platform agent NFT and post one
  `giveFeedback` against deployed Sepolia contracts; verify decode of return
  values/events against the stub `sol!` bindings in
  `crates/xvision-identity/src/contracts.rs`; if mismatch, pin verified ABIs.
- Acceptance: tokenId returned; feedback tx confirmed; Mantlescan links recorded
  in a `docs/` runbook; `MANTLE_TESTNET_IDENTITY_REGISTRY`/`…_REPUTATION_REGISTRY`
  env documented for the demo host.
- Verification: re-run example → second feedback entry visible on-chain.
- Conflicts: `crates/xvision-identity/**` single-writer.

**T1.3 — Wire attestation trigger into engine run finalize** · Effort M–L · Risk medium · Required (for the ERC-8004 beat) · Depends T1.2
- Objective: on eval/live run completion in
  `crates/xvision-engine/src/api/eval.rs` (finalize path), behind an env/config
  gate (e.g. `XVN_CHAIN_ATTEST=1` + addresses present), map run outcome →
  verdict (spec §3.6: sharpe-delta → 100/50/0) and call
  `xvision_identity::attestation::submit_attestation` (signer from env). Fire-and-forget with logged failure — must never fail the run itself.
- Files: `crates/xvision-engine/src/api/eval.rs`, `crates/xvision-engine/Cargo.toml`
  (optional-feature dep on xvision-identity to keep default build light — mirror
  the `WITH_IDENTITY` pattern in `Dockerfile.deploy`).
- Acceptance: completing a run on the demo host produces a reputation entry on
  Sepolia tagged with the run/cycle id; with the gate off, behavior is byte-identical to today (existing tests pass).
- Verification: `scripts/cargo test -p xvision-engine`; one gated live rehearsal.
- Conflicts: `api/eval.rs` is hot — single-writer; coordinate with T2.4.
- Risk note: adding a default-off dependency edge to a non-default-member crate
  must not slow the hot dev loop — feature-gate it.

**T1.4 — Live-path risk veto parity** · Effort S–M · Risk medium · Required-or-narrate
- Objective: port the veto block at
  `crates/xvision-engine/src/eval/executor/backtest.rs:1834–1892`
  (daily_loss_kill_pct + max_concurrent_positions → rewrite to hold + supervisor
  note + `risk_veto` event) into `decide_one_live` (line 3341+), reusing the
  same helpers/book state.
- Acceptance: a live paper run configured with `max_concurrent_positions=1`
  attempting a 2nd open emits the `risk_veto` supervisor note and holds; backtest
  behavior unchanged (existing `multi_asset_backtest` test green).
- Verification: targeted test in `crates/xvision-engine/tests/` mirroring the
  backtest veto test against the live decide path (mock fill sink).
- Fallback: if not done by Jun 13, demo the risk beat on backtest and narrate
  SafetyGate+pause on live. Do not claim live vetoes.
- Conflicts: `backtest.rs` single-writer; do not overlap with anyone touching
  the executor.

### Milestone 2 — High-leverage wiring (parallel after T1.1)

**T2.1 — Identity strip in dashboard** · Effort M · Risk low · Polish-but-high-value · Depends T1.2
- Objective: render `GET /api/settings/identity` (+ new fields: agent NFT
  tokenId, registry address, last attestation tx) as a full-width inline strip
  on the live run detail page and/or a `/settings/identity` page. Links out to
  Mantlescan. NO new right column, NO popup.
- Files: `crates/xvision-dashboard/src/routes/settings/identity.rs` (extend
  report), new `frontend/web/src/routes/settings/identity.tsx` + strip component.
- Acceptance: strip shows real tokenId + clickable explorer links when env
  configured; renders a quiet "not configured" state otherwise; vitest for the
  component; dark-mode borders per workspace rule.
- Conflicts: `frontend/web/src/routes/settings/**` single-writer.

**T2.2 — Composite stop (flatten + cancel)** · Effort S · Risk low · Quick win
- Objective: add `xvn eval stop <run_id>` and/or a dashboard action that calls
  flatten (`api/eval.rs:510`), waits for flat confirmation, then cancel
  (`api/eval.rs:438`). Pure composition of existing endpoints.
- Acceptance: one command/click takes a run with open positions to
  flat+cancelled; partial-fill failure path surfaces the existing supervisor
  warnings rather than silently terminating.
- Conflicts: coordinate with T1.3 on `api/eval.rs` (or implement purely
  client-side in CLI to avoid the conflict zone — preferred).

**T2.3 — Unlock onchain driver for `publish` + `attest` only (CLI)** · Effort M · Risk medium · Optional/defer-if-tight · Depends T1.1, T1.2
- Objective: replace the hard rejection at
  `crates/xvision-cli/src/commands/marketplace.rs:87–97` with construction of
  `Erc8004MantleDriver::with_signer` from env (addresses + RPC + key via `op`);
  scope to `publish`/`attest` verbs; `buy` stays mock.
- Acceptance: `xvn marketplace publish …` creates a real listing on Sepolia and
  prints listingId + explorer link; without env, behavior identical to today.
- Conflicts: `commands/marketplace.rs`, `crates/xvision-marketplace/**`.
- Note: this gives the demo a real ListingRegistry entry to show even though the
  SPA marketplace stays fixture.

**T2.4 — Demo data seeding script** · Effort S–M · Risk low · Required for rehearsal
- Objective: a script seeding the demo host: 2–3 strategies (distinct agents/
  models), 1 scenario, 1 completed backtest with vetoes visible, an optimizer
  session running (for SSE liveliness), 1 live paper run active.
- Acceptance: fresh container + script ⇒ every demo screen has real data; charts
  overview no longer falls back to fixture (it has runs).
- Conflicts: none (uses public API/CLI only).

### Milestone 3 — Polish (only after M1 green)

**T3.1 — Genart reconciliation (AM2)** · Effort M · Risk low · Polish
- Objective: pick Rust SVG (`crates/xvision-identity/src/genart.rs`) as
  canonical; have the frontend render the same SVG (serve via API or port the
  algorithm), or simply stop showing the canvas placeholder where the NFT image
  could be compared.
- Acceptance: same seed renders identically on tokenURI and dashboard, or the
  comparison is impossible to make in the demo flow.
- Decision needed from human first (Open Question Q3).

**T3.2 — Fixture disclosure pass** · Effort S · Risk none · Quick win
- Objective: confirm the existing testnet badges on all marketplace routes;
  label wallet connect "testnet preview"; ensure charts overview fixture state
  says "sample data" when fixture fallback fires
  (`crates/xvision-dashboard/src/routes/charts_dashboards.rs:1–29`).
- Acceptance: no screen presents mocked data without a visible label.

**T3.3 — Demo runbook + recording** · Effort S · Risk none · Required
- Objective: write the click-by-click demo script (happy path + the fallback
  path from §8), record a backup screen-capture of the full spine on Jun 14.
- Acceptance: backup video exists; two people have executed the runbook.

### Quick wins (do anytime)
T0.3 (snapshot tree), T2.2 (composite stop, CLI-side), T3.2 (labels),
running `examples/mint_identity.rs` once right after deploy (instant on-chain artifact).

---

## 7. 24h / 48h / 72h Critical Path

**By +24h (end of June 11):**
- T0.1 test baseline + T0.2 smoke script green locally. (parallel)
- T0.3 baseline tag.
- **T1.1 testnet deploy done + addresses committed** (owner-driven; everything
  on-chain hangs off this).
- T1.2 round-trip sanity: platform agent minted, one feedback posted, explorer
  links saved.

**By +48h (end of June 12):**
- T1.3 attestation trigger merged behind env gate; rehearsal run posts a real
  attestation.
- T1.4 live risk-veto parity merged (or decision made to narrate-on-backtest).
- T2.4 seeding script working against a fresh deploy image.

**By +72h (end of June 13):**
- T2.1 identity strip visible with real data.
- T2.2 composite stop. T2.3 only if everything above is green.
- Full smoke + demo runbook executed once end-to-end on the actual demo host.

**June 14 = buffer + rehearsal day** (T3.x polish, backup recording). Nothing new
merges after June 14 noon. **June 15: demo.**

---

## 8. Fallback Demo Paths

**If testnet deploy/contracts fail (RPC issues, faucet, PUSH0/EVM quirks):**
Run the identical stack against a local anvil chain — every contract test already
passes on 31337, `DeployTestnet.s.sol` runs against anvil (existing broadcast
history proves it), and `RegistryAddresses::custom()`
(`crates/xvision-identity/src/client.rs:238–244`) accepts arbitrary addresses/RPC.
Demo line: "contracts are chain-agnostic; here is the identical flow on our local
node; testnet deployment is in flight." All wiring tasks (T1.3, T2.1) work
unchanged — only env vars differ. Keep an anvil + deploy + mint script as part of
T2.4 seeding so this fallback is one command.

**If live paper trading fails (Alpaca outage, market closed, stream issues):**
Pivot the run beat to a backtest eval run — same executor, same agents, same
decision traces, same SSE eval stream and dashboard surfaces, and (bonus) the
risk-veto beat is already native there. The optimizer SSE loop is a second
independent "live" thing to show. The live cockpit can still be shown replaying a
completed run's decisions/positions.

**If both fail:** recorded backup video (T3.3) + fixture marketplace walkthrough +
contracts code/test walkthrough (`forge test` live on stage is itself a credible
artifact).

---

## 9. Open Questions for the Human

1. **Deploy ownership & key custody:** who holds `OPERATOR_EOA` and the funded
   deployer key, and is the deploy being run by you today as stated? Agents can
   prep the runbook but should not handle the key (use `op`).
2. **Demo host:** which machine serves the dashboard on June 15 (local laptop,
   Tailscale node, fresh VPS)? Determines where T2.4 seeding and env vars land,
   and whether F13 (auth gaps) matters at all.
3. **Genart canonical renderer (AM2):** Rust SVG or frontend canvas? One-line
   decision unblocks T3.1 (or lets us skip it).
4. **Risk-gate narrative:** is porting the live veto block (T1.4) wanted, or do
   you prefer to demo risk vetoes on backtest and narrate SafetyGate on live?
5. **Marketplace ambition:** is one real on-chain listing via CLI (T2.3) worth a
   day, or is the fixture UI + one real attestation + identity strip enough story?
6. **Cline sidecar in the demo:** should the agentd/Cline path be shown (it's
   baked into the image but untested in CI), or is LlmDispatch the demo runtime?
7. **x402:** drop `buyWithAuthorization` from the demo entirely (recommended —
   USDC.e EIP-3009 unverified, F12)?

---

## Appendix: What was NOT verified

- No `cargo test --workspace` / `forge test` / `pnpm test` was executed during
  this audit (T0.1 exists precisely to close that gap). "Tests green" claims for
  contracts are inferred from code state and repo history, not a fresh run.
- Subagent-reported line numbers were spot-checked, not exhaustively re-read;
  the one Critical contradiction (live risk gate) was resolved by direct reads
  (`backtest.rs:1834–1892` vs `decide_one_live` at 3341+, sizing at 3547,
  guardrail ~3483, pause 3518).
- Alpaca live-stream behavior (`MultiLiveStream` actually receiving bars) was
  traced to its construction (`eval.rs:3407–3442` per subagent) but not run.
- ERC-8004 is a draft standard; "compliance" means matching the draft interface,
  not a certified implementation.
