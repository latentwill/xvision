# XVision vs. Agentic Trading Research: Current State, Planned State, and Gap Analysis

This page compares the agentic trading strategy report to xvision *as it exists today* and to the xvision roadmap already checked into this repo. It is meant to be the bridge between the thesis doc and the implementation plan.

## Source report

- [Agentic Trading Strategy Research: Filters, Prompts, Indicators, and Market Conditions for LLM-Assisted Trading](https://100.65.116.60/docs/reports/agentic-trading-deepseek.html)

## TL;DR

- The research report is broader than xvision today. It assumes a generic LLM-assisted trading stack with regime filters, stock signals, richer onchain inputs, execution-policy selection, and a multi-agent Researcher → Trader → Risk → Execution flow.
- xvision already has the *spine* of that idea: deterministic risk and execution, an Intern/Trader split, regime-aware and news-reader templates, strategy graphs, filters, baselines, tracing, and real execution backends.
- The biggest gaps are richer market-substrate signals, explicit execution-policy selection, broader stock/fundamental/news coverage, and the research report's wider signal taxonomy.
- The roadmap in this repo closes some structural gaps, but it still does not cover the full breadth of the research memo.

## What the research report is asking for

The report argues for a strict separation between a deterministic substrate and a policy layer.

- The **substrate** should classify market structure, enforce hard filters, and constrain execution.
- The **policy layer** should reason over compressed market state, pick a setup, size within bounds, and react to context.
- The design explicitly prefers LLMs as contextual reasoners, not raw alpha generators.
- The multi-agent shape in the report is: Researcher produces evidence, Trader makes the decision, Risk can veto, Execution stays deterministic.

The report's core signal families are broader than the current xvision prompt surface:

- Regime classification: trend, volatility, liquidity, session, news.
- Technical hard filters: MA crossover, RSI, MACD, Bollinger, Donchian, VWAP, order-book conditions.
- Stock signals: fundamentals, earnings surprises, insider trades, short interest, options flow, sector rotation, breadth, ETF flows, sentiment, alternative data.
- Onchain signals: exchange flows, whale accumulation, DEX liquidity, TVL, protocol revenue, active addresses, stablecoin flows, funding, OI, liquidation clusters, perp basis, token unlocks, bridge flows, gas / MEV, smart money, developer metrics.
- Deterministic guardrails: market impact, spread, volatility, slippage, liquidity, circuit breakers, and execution-mode selection such as TWAP / VWAP / adaptive.

## What xvision has today

xvision is not a blank slate. It already implements a substantial portion of the architecture implied by the report.

### Core architecture already present

- A typed Rust workspace with separate crates for core types, engine, eval, execution, dashboard, CLI, MCP, intern, observability, and identity.
- A `Strategy` / `AgentRef` / pipeline model that is more structured than the prose architecture in the report.
- Capability-based agent dispatch with typed outputs for `Trader`, `Filter`, `Critic`, `Intern`, and `Router`.
- Template coverage already includes regime-aware and news-reader variants, so xvision is not starting from a generic blank slate.
- Deterministic risk code and explicit execution backends rather than letting the LLM directly place trades.
- Baselines, A/B compare, trace capture, and replayable evaluation surfaces.
- Real execution surfaces for paper and live-perps style execution.

### Current prompt surfaces

The current prompt surfaces already expose some of the report's key substrate ideas:

- The trader prompt receives a balanced briefing, portfolio state, and structured evidence.
- The trader-facing briefing already includes prices, indicators, regime, and a news digest.
- The intern prompt receives a market snapshot plus an indicator panel and a compact onchain / derivatives panel.
- The repo also ships dedicated regime-aware and news-reader agent templates, which narrow the gap even if they do not cover the report's full breadth.
- The intern is asked to produce bull / bear / flat evidence rather than an implicit trade recommendation.

### Signal coverage already implemented

The repo already contains a fairly broad indicator stack, even if the main trader prompt only surfaces a subset today.

- Surfaced in the main prompt today:
  - RSI
  - SMA / EMA
  - Bollinger bands
  - ATR
  - MACD
  - Donchian channels
  - A compact onchain / derivatives block with funding, open interest, long / short ratio, stablecoin inflows, liquidations, and realized volatility

- Implemented in the filter / indicator catalog but not surfaced as the main trader prompt defaults:
  - ADX and DI+/DI-
  - WMA and ROC
  - Stochastic and StochRSI
  - CCI and MFI
  - OBV and VWAP
  - RVOL / RVOL-TOD and volume z-score
  - Ichimoku components
  - Previous day / week / month levels
  - Premarket levels
  - Opening range features
  - Gap features
  - Keltner channels
  - Williams %R

## What is already planned in xvision

Several repo specs already move xvision closer to the research thesis.

### Capability-first agent model + graph composition

- Makes capability explicit instead of relying on brittle role strings.
- Formalizes `Filter`, `Critic`, `Intern`, `Router`, and `Trader` semantics.
- Moves xvision toward typed graph composition rather than a single monolithic pipeline.

### Agent firing-filter surface

- Makes filter behavior operator-visible instead of hidden implementation detail.
- Pushes filter gating into the product surface, which matches the research report's emphasis on deterministic market-substrate controls.

### Multi-asset strategies design

- Moves the asset universe into strategy config.
- Adds signal scope so signals are not forced into a single synthetic asset namespace.
- Supports per-asset fan-out, which is an important step toward richer strategy decomposition.

### Compare / A-B respec

- Makes strategy comparison more analytical and readable.
- Improves the evaluation surface that xvision uses to judge agentic behavior and strategy variants.

### Chat rail, DSPy, and strategy agents

- Pushes xvision toward better operator workflows for chat-driven authoring, evidence capture, and optimization.
- Keeps DSPy offline, which matches the research report's insistence on keeping live decisions deterministic.

## Specific gaps: architectural structures that are missing or only partial

These are the biggest structure-level differences between the report and xvision today.

- **No separate Researcher agent.**
  - The report wants an explicit evidence-producing role.
  - xvision uses the Intern plus specialized templates (for example, regime and news-reader) to cover parts of that surface, but it does not yet have the report's full Researcher-style evidence pipeline.

- **No explicit Risk Manager agent.**
  - xvision has deterministic risk code, but the report's optional vetoing agent is not a first-class live component.

- **No dedicated Portfolio Manager agent.**
  - Portfolio allocation remains deterministic or executor-side rather than being a policy role.

- **No execution-policy chooser.**
  - There is no LLM-driven branch that decides TWAP / VWAP / adaptive execution based on spread, impact, or volatility.

- **No first-class market-regime subsystem at the report's breadth.**
  - xvision already has a regime-classifier template and regime-aware trader flow, but it still lacks the report's broader regime taxonomy, feature breadth, and classifier stack.

- **No live news ingestion loop.**
  - xvision already has a news-reader template and news-digest field, but the report's broader headline/social/news-velocity pipelines and prompt-injection-aware handling are still not fully represented.

- **No explicit evidence graph matching the report's handoff chain.**
  - xvision has tracing, but it does not yet serialize the full Researcher → Trader → Risk evidence trail the way the report describes.

## Specific gaps: signals and indicators that are missing or not implemented

This is the practical feature gap list.

- **Not surfaced today as first-class prompt inputs:**
  - Choppiness Index
  - Hurst exponent
  - order-book imbalance
  - depth slope
  - explicit liquidity regime features
  - spread regime features
  - market impact estimates
  - execution-style selection signals

- **Not implemented as stock-market feature families:**
  - fundamentals
  - earnings surprises
  - insider transactions
  - short interest
  - options flow
  - sector rotation
  - market breadth
  - ETF flows
  - news sentiment
  - social sentiment
  - alternative data

- **Not implemented as richer onchain feature families:**
  - exchange flows
  - whale accumulation
  - DEX liquidity and DEX volume divergence
  - TVL
  - protocol revenue
  - active addresses
  - perp basis
  - token unlocks
  - bridge flows
  - gas / MEV
  - smart-money detection
  - developer activity
  - liquidation-cluster analysis beyond the current compact fields

## Where xvision differs from the research thesis

The two systems are solving related but different problems.

- **Architecture difference:**
  - The research memo describes a generic Researcher → Trader → Risk → Execution chain.
  - xvision is a typed Rust workspace with Intern → Trader, deterministic risk, explicit execution backends, and product surfaces around that core.

- **Policy difference:**
  - The report imagines the LLM selecting execution strategy and adapting risk parameters within bounds.
  - xvision currently keeps execution policy much more deterministic and narrower.

- **Data-scope difference:**
  - The report covers stocks, crypto, onchain, sentiment, and macro.
  - xvision is crypto / perps-first, with a compact derivatives / onchain snapshot rather than a broad market-data universe.

- **Evaluation difference:**
  - The report emphasizes walk-forward validation and broad signal taxonomies.
  - xvision emphasizes baselines, compare surfaces, traces, and strategy comparison in the product.

- **Platform difference:**
  - xvision is not just a model pipeline.
  - It is also a CLI, dashboard, MCP surface, strategy authoring environment, observability stack, and a future identity / marketplace platform.

## What xvision already does better than the report

A fair comparison also cuts the other way.

- The repo has a typed implementation rather than a prose-only architecture.
- The execution surfaces are real: Alpaca paper and Orderly perps are wired into code.
- The evaluation and observability surfaces are concrete: baselines, compare, traces, replay, and diagnostics.
- The operator surfaces are explicit: dashboard, CLI, and MCP share a common model.
- The roadmap is already encoded in dated specs and plans, which makes the missing pieces easier to land in order.

## Bottom line

The report is a broad design thesis for a generic LLM-assisted trading stack. xvision is a narrower, more concrete implementation direction: Rust-first, strategy-graph-first, crypto/perps-first, and evaluation-heavy.

If the goal is to match the report exactly, xvision still needs richer market data, more signal families, and a more explicit separation around execution policy.

If the goal is to build a safer and more productizable trading system, xvision already has the more disciplined foundation in some important places: typed APIs, deterministic guardrails, traceability, and operator workflows.

## Related docs in this repo

- [XVision / Agentic Trading](xvision.md)
- [Research Index](research-index.md)
- [Capability-first agent model + graph composition](2026-05-22-capability-first-agent-model-and-graph-composition.md)
- [Multi-asset strategies design](2026-05-24-multi-asset-strategies-design.md)
- [Compare A/B respec](2026-05-23-compare-ab-respec.md)
- [Chat rail, DSPy, and strategy agents](2026-05-24-chat-rail-and-strategy-agents-evaluation.md)
