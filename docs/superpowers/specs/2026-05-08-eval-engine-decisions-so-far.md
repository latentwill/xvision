# Eval Engine — Brainstorm Decisions So Far

> **Status:** PAUSED. Brainstorm intentionally stopped before approach is finalized.
> **Reason:** The eval engine consumes strategy artifacts produced by the strategy creation engine. We're brainstorming and spec'ing the strategy creation engine first so the artifact contract is locked before the eval engine spec is written. Resume this brainstorm after the strategy creation spec lands.

## Pivot context

Xvision is pivoting fully to: (1) the multi-strategy evaluation engine, then (2) the marketplace + 8004 trading agent strategy. This document captures decisions for (1)'s evaluation engine, the visualization layer half of the pivot.

## Locked decisions (captured during 2026-05-08 brainstorm)

| # | Decision |
|---|---|
| 1 | **Execution mode:** backtest + paper, toggleable per run, via a clean execution-surface abstraction so strategy code is mode-agnostic. |
| 2 | **Scenario controls:** time window + asset universe + regime tags; capital + position + risk constraints; slippage / fee / latency models. Synthetic shocks/noise/dropout deferred. |
| 3 | **Comparison axis:** runs are first-class artifacts identified by (strategy, params, scenario) triples. Comparison view renders any arbitrary user-picked subset. |
| 4 | **Visualization host:** Rust axum server + SPA dashboard at localhost. Live run progress streams via SSE/WebSocket. |
| 5 | **Chart library:** TradingView Lightweight Charts (Apache 2.0) as primary. TradingView Advanced Charts noted as a post-hackathon upgrade path if studies / drawings / multi-chart layouts are wanted. |
| 6 | **NL evaluator v1:** structured-finding extractor only — LLM reads run set, emits JSON findings with kind / severity / affected_runs / evidence. No chat surface. v2 will add Q&A on top of the same record stream. Records are the substrate the future Karpathy autoresearcher loop consumes. |
| 7 | **Concurrency model:** tier-gated. Free tier = 1 run at a time. Paid tier = bring-your-own API keys, unlimited concurrent agents. Backtests parallelizable (CPU-bound, pure); paper sequential against shared Alpaca rate budget. |
| 8 | **Persistence:** SQLite + JSONL trade tapes on disk for hackathon / free tier. Run id = ULID. Postgres + object-storage migration path noted but deferred until the marketplace ships. |

## Architectural shape (locked)

**Approach B — greenfield `xvision-engine` crate, deprecate `xvision-eval`.**

- New top-level engine crate owns: runs, scenarios, executor, store, dashboard server, findings extractor.
- Existing `xvision-eval` deprecated. ab-compare, baselines, bootstrap, gate logic ported in.
- Rationale (user's words): start fresh with the new design. The new design's scope (multi-strategy comparison, run-as-artifact, NL findings, tier-gated concurrency) materially exceeds what `xvision-eval` was built for; layering on top would accumulate archaeological debt.

## What still needs to be decided when we resume

- Strategy artifact contract (defined by the strategy creation engine spec — that's why we're pausing).
- Module layout inside `xvision-engine` (runs / scenarios / executor / store / dashboard / findings).
- SSE/WebSocket message schema for live progress.
- Finding record JSON schema (will be informed by what data the strategy creation engine surfaces).
- Migration / port plan for ab-compare, baselines, bootstrap, gate from `xvision-eval`.
- CLI surface (`xvn eval ...` subcommands).
- Rate-limit + concurrency policy specifics for the tier-gated model.
- LLM model + prompt for the findings extractor.

## Out of scope for this spec (when resumed)

- Strategy creation engine itself — separate spec.
- Marketplace UI, ERC-8004 reputation surfaces — later phase.
- Karpathy autoresearcher improvement loop — consumes findings but doesn't constrain this spec.
- Synthetic stress scenarios — deferred per Q2.
- Native desktop wrapper — deferred per Q4.
- Q&A surface over findings — v2 per Q5.
