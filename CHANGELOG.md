# Changelog

All notable changes to xvision are documented here. The format is based
on [Keep a Changelog](https://keepachangelog.com). Versioning rules
live in [`docs/VERSIONING.md`](docs/VERSIONING.md): the pre-1.0 MINOR
component is the image-release train (`0.21.0` -> `0.22.0`), while PATCH
is reserved for same-train hotfixes.

Unreleased entries accumulate above the most recent released section.
Each release ships as a Docker image; the version that the running
container reports must match the tag pulled.

## [Unreleased]

## [0.38.0] - 2026-06-24

### Added

- **`Disconnected` run status** — runs interrupted by broker connection loss are now marked `disconnected` (resumable) instead of `failed`, with migration 074.

### Changed

- **Separate optimizer gate thresholds** — `holdout_min_improvement` is now independent of `min_improvement`, with the loosening schedule disabled but preserved for later opt-in.
- **`RunStatus::Disconnected`** wired through eval retry, live deployments, and review payload surfaces.
- **`store.fail_active`** now accepts an optional `RunStatus` parameter.
- **`store.get()`** returns `Run` directly instead of `Option<Run>`.

### Fixed

- **Bar storage** — live-run OHLCV bars persisted to `eval_run_bars` with asset column (migration 073), warmup `record_bar` missing `open` field, chart slim-path guards.
- **Delayed decisions** — `delayed` column added to `eval_decisions` (migration 071), surfaced in `DecisionRowDto` type and frontend display.
- **Unrealized PnL** — `unrealized_pnl_usd` field added to `RunSummary`, displayed in dashboard.
- **Frontend CI** — unclosed `<td>` in DecisionsTable, dead deploy-readiness code in home.tsx, missing test fixture fields across 21 files.
- **Live bar counter** — `live_bar_count` now correctly used in progress/stop-policy/logging instead of warmup-inclusive `bar_count`.
- **Windows install** — `install.sh` now detects `mingw`/`msys`/`cygwin` and uses `.zip` extension.
- **Cargo fmt** — workspace-wide formatting applied.

- **Dashboard auth is now opt-in** — the server no longer requires
  `XVN_DASHBOARD_TOKEN` to start on non-loopback binds. By default,
  the dashboard is open. Set a password via Settings → General →
  Dashboard Password to enable authentication. The env var is still
  supported for backward compatibility.
- **Simplified login flow** — the "Start session" two-step gate is
  replaced with a direct password form. No session tokens needed.
- **Settings UI** — new Dashboard Password card on the General settings
  page (set, change, or remove the password).

## [0.37.0] - 2026-06-18

Native PC distribution: `xvn` now ships as a standalone binary for macOS
(arm64 + x86_64), Linux (x86_64-musl), and Windows (x86_64-msvc) — no Docker
required. The SPA dashboard is baked into the binary via rust-embed.

### Added

- **Native binary distribution** — tagged releases build and publish
  per-platform tarballs + SHA256 checksums as GitHub Release assets, and
  auto-update the Homebrew formula (`brew install latentwill/xvision/xvn`).
- **`xvn update`** — self-update command that fetches and installs the latest
  release binary in place.
- `scripts/install.sh` one-line installer and `scripts/smoke-test.sh` for
  post-release verification.
- Marketplace x402 autonomous purchases — self-hosted facilitator + non-custodial
  MCP client on Mantle (#1086, #1089).
- Nanochat Filter Agent — trained GPT2-scale model filter slot + Autoresearcher (#1090).
- Byreal Solana spot trading — gated one-shot swaps and poll-only live marks.

### Changed

- Marketplace network is resolved at runtime from the backend
  (`/api/marketplace/status`); one prebuilt image serves either network.

### Fixed

- Optimizer QA fixes — anti-patterns schema, `response_format` fallback, and
  cross-provider handling (#1096).


## [0.36.0] - 2026-06-15

The first image-release train since the `0.21.0` baseline, covering ~four
weeks and ~259 merged PRs. Highlights below are grouped and condensed;
`git log v0.21.0-baseline..` (or the merged PR titles) has the granular
detail. PR references are representative, not exhaustive.

### Added

**Optimizer (autooptimizer cycle)**
- `xvn optimize run` is the single CLI surface for the overnight cycle; the
  DSPy flywheel runs *inside* the cycle automatically and emits
  `CycleProgressEvent::FlywheelCompiled` (#972).
- Optimizer session-detail page with truthful live-run indication, timeline,
  experiments × regimes heatmap, outcome KPI strip, and blob-diff inspector (#1070).
- Configurable experiments-per-cycle; mutation axes across prompt, filter, and
  DSPy levers; lineage sealing with Merkle root + cycle seal.
- Reliability: 300s timeout for slow OpenAI-compat reasoning models; `run_session`
  survives imperfect model output (#1071, #1075).

**Trace / observability**
- `UnifiedEvent` convergence: agent-run LIVE stream wired onto one event model at
  full inspector fidelity (#1044–#1049).
- Trace dock: nested span tree, model-reasoning taxonomy, deterministic
  filter-firing spans, risk-gate + decision-input + outcome/exit spans, on-chain
  attestation boundary event (#1042).
- Spans + `agent_runs` recorded/finalized for batch, experiment, and sweep eval
  runs; actual prompt + response text recorded for Cline trader calls (#1064, #1066).
- Optimizer cycle visible on the trace surface; candidate eval runs nested under
  their experiment row; full-fidelity trace/flywheel export (#1048, #1050, #1052).

**Mantle marketplace**
- On-chain listing / browse / sell flow on Mantle, with the active network
  resolved from the backend at runtime so one image works on testnet or mainnet (#1065, #1069).
- x402 / EIP-3009 USDC purchase flow; sealed-tier (Lit Protocol) gated strategies
  requiring EIP-191 proof-of-address on import (#979).
- Listing owner management: in-place price edit + My Listings page; mainnet UUPS
  upgrade script for `ListingRegistry.updatePrice` (#1076, #1077).
- On-chain generative art (bitfields engine twins, card renderer); creator
  profile + lineage forest; purchase receipt + share composer; layered data
  selection (subgraph → indexer probe → fixtures).

**Live venues**
- Byreal perps: single-stage agent wiring, native TP/SL brackets, CLMM LP
  open/rebalance/close, funding-aware carry guard, settable credentials (#962, #1000).
- Virtuals Degen Arena (native Hyperliquid perps): settings card, launch
  selector, real-venue standing chips (#1047, #1072).
- Orderly mainnet venue + fuller market coverage; live-deployments contract with
  per-tick capital block streamed over deployment SSE.

**Control Tower / dashboard**
- Control Tower home: readiness strip, since-last-here, nag triage, optimizer
  digest, live reconcile, capital-risk strip (deployed capital · drawdown · daily
  loss buffer), cross-source cost rollup (#983).
- `/live` page with agent-runs list + optimizer digest; experiment detail page (#974).

**Eval engine**
- Live eval mode (`LiveConfig` + `--live-duration`); agentless **Mechanistic
  Strategies** via the `Algorithm` trait; Pine Script ingestion → optimizable
  `Strategy` (#998, #1014).
- Intra-bar O→H→L→C fill ordering with maker/taker aggressor-side fees and
  per-asset fee/slip overrides; lookahead-bias prober + `DataManifest` content
  hashing; net-of-inference-cost return metric.
- Episodic memory (Cortex) across agent surfaces; broker-rule circuit-breaker that
  skips unsupported trades instead of aborting the run.

**CLI / UI**
- `xvn migrate` renamed to `xvn init`; first-run tour with honest forward-test vs
  live labeling (#1035); live token-count + cost on running evals; `xvn eval cancel`.
- Settings consolidation, eval PnL split, Signal model dropdown across all model
  pickers, accent-color picker (#969); standard responsive list component across
  list surfaces; first-class Ollama support.

### Changed

- `autoresearcher` → `autooptimizer`/Optimizer rename across Rust modules, SQLite
  tables, HTTP routes, frontend, and MCP types; operator verb consolidated to
  `xvn optimize` (#706, #972).
- Risk unified onto the engine veto path; the legacy `xvision-risk` crate retired (#1038).
- Legacy Intern/Critic roles and the two-stage Rust trader path retired; Cline
  sidecar is now the trader path (#1004).
- `build_seed_context` made the canonical shared seed-context constructor
  (#963, #973, #984).
- Virtuals removed from the live page/lineage strip; Hyperliquid + Orderly
  credentials now settable in Settings (#1079).

### Fixed

- Marketplace: mainnet parity for sell + browse; QA batch (banner, number format,
  x402, search, gen-art label, ⌘K palette) (#1065, #1073).
- Optimizer: cycle concurrency guard, configurable objective, capture-on-interrupt,
  budget-window cancel, prose visibility in the snapshot view.
- Trace dock: filter bar above the TREE/FLAME toggle; prompt/response clamped to
  4 lines with show-more (#1067, #1068).
- Live eval: guard payload build against empty scenarios; only auto-select
  genuinely-live runs into the `/live` viewport (#1078).
- `enabled_models` allowlist validated in dispatch + dashboard; per-asset
  bar-cache key fix; baseline test-rot repaired across engine + CLI suites.

### Removed

- `xvn ab-compare` (`--setups` → `--cycles` migration); the `xvn optimizer`
  top-level verb and standalone DSPy CLI verbs (folded into `xvn optimize`) (#972).
- `mechanical_params` field + `set_mechanical_param` tool from `Strategy` (#1040).
- `xvision-risk` crate (#1038); llama.cpp provider support (Ollama replaces it);
  Virtuals as a live-page venue (#1079).

### Versioning

- Workspace + frontend bumped `0.21.0` → `0.36.0` (image-release train). A
  `0.36.0` bump was committed on 2026-06-12 (`955c3448`) but lost in a later main
  reset; this release restores it. No image had been shipped or tagged at any
  intermediate train number — `0.21.0` was the only previously released version.
- First tagged release (`v0.36.0`) and first GHCR image published from a version
  tag via `.github/workflows/docker.yml`.


## [0.21.0] - 2026-05-18

Baseline version. Twenty-one image-shipping QA waves preceded this entry;
their granular detail lives in `git log` and the merged PR titles.
Establishing the versioning scheme here so every subsequent image
gets its own changelog section.

Notable state at this baseline (high-level snapshot, not exhaustive):

### Added
- Agent CI/CD Phase-1 spec and contract pack (`docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`, contracts under `team/contracts/agent-cicd-*.md`).
- V2A onboarding wave: in-app `/docs` route, Driver.js first-run tour, restart-tour affordance.
- Agent-run observability stack (retention modes, blob fetch route in flight, span inspector preview path).
- Eval surface: TradingView lightweight charts, mobile inspector polish, decisions table with positions + PnL columns.
- Alpaca paper crypto: non-fatal broker rejection handling for bracket/short semantics.
- QA-driven hardening across the agent runtime, wizard, eval engine, trace dock.

### Changed
- Trader output action match is now case-insensitive (`"Hold"` → `"hold"`) to keep Qwen 3.6 + similar models in vocabulary.
- Wizard `create_strategy_draft.template` relaxed to optional; templates are reference examples, not required.
- Chat-rail mutations now invalidate the matching list queries across strategies / scenarios / agents / eval-runs.
- macOS scrollbar affordance: `.scrollbar-stable` utility + per-surface opt-in so "more below" is visible.
- Trace dock: resizable handle with persisted height; redundant "Full" button dropped.

### Fixed
- 30-bar scenarios now produce N decisions for N bars (off-by-one fix, pinned with parameterized test).
- Cancelled-run capsule no longer bleeds across routes; delete added to inspector.
- Span streaming indicator preserved against legacy `span.streaming` representations.
- "Estimated bars to fetch: 0" now reacts to the context-bars input.

### Versioning
- Workspace + frontend bumped from `0.1.0` → `0.21.0` to establish the scheme.
- `docs/VERSIONING.md` and `scripts/bump-version.sh` added.
- Workspace `[package].version` is the single source of truth; frontend `package.json` mirrors it; both are bumped atomically by the script.
