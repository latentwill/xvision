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
