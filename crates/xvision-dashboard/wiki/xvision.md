# XVision / Agentic Trading

This page collects the trading-specific docs that map directly to xvision: strategy design, evaluation, agent tooling, and trading infrastructure.

## Roadmap

- [XVN Roadmap](/docs?slug=xvn-roadmap) — the single visible roadmap for V2 items, follow-ups, the today-made plan set, and the hackathon countdown.

## Why these belong here

These reports all inform the same product surface: how xvision should reason about strategies, what should stay deterministic, how to evaluate agentic behavior, and which infra choices make the system safer and faster.

## Included reports

### Agentic Trading Strategy Research: Filters, Prompts, Indicators, and Market Conditions for LLM-Assisted Trading

- Emphasizes a hard split between deterministic guardrails and LLM policy decisions.
- Says the LLM is best used for contextual reasoning over compressed market state, not raw alpha generation.
- Recommends multi-agent decomposition with structured JSON handoffs for auditability.
- Calls out memory hygiene, backtest/live separation, and outcome-bias prevention as non-negotiable.

### XVision vs. Agentic Trading Research: Current State, Planned State, and Gap Analysis

- Compares the research thesis to xvision's current implementation and the roadmap already in this repo.
- Lists which structures are present, which are still only planned, and which are missing entirely.
- Links the thesis to the implementation plan instead of leaving it as a standalone research memo.

### AI-Agentic Algorithmic Trading Platforms: Landscape, Comparisons, and Product Gaps

- Finds no production-ready fully autonomous trading platform yet.
- Argues that deterministic execution engines plus AI/ML signal layers are the practical path today.
- Highlights the lack of trustworthy backtesting for agentic behavior as a major product gap.
- Frames that gap as an opportunity for xvision.

### Using AutoResearcher to Its Fullest for Agentic Trading and PnL-Focused Research

- Treats AutoResearcher as a research engine for hypothesis formalization and PnL attribution.
- Stresses source tiering, critique loops, and reproducible research workflows.
- Pushes for a clean path from hypothesis → backtest → attribution → roadmap.

### Optimizing Cline SDK Usage for Reliable Agentic Engineering

- Focuses on production hardening for Cline-based workflows.
- Calls out context management, cost optimization, security, observability, and lineage tracking.
- Applies directly to agentic engineering work inside a trading product.

### Optimizing Prompts and Agents with DSPy

- Frames prompt tuning as a programmatic optimization problem instead of artisanal prompt crafting.
- Gives a systematic route to optimize modules, signatures, and metrics.
- Useful for trading-agent slot tuning and evaluation loops.

### Using Rust for Trading Platforms, DEX/CEX Infrastructure, and Financial Dashboards

- Argues for Rust where latency, safety, and reliability matter.
- Covers connectors, dashboards, backtesting, and production safety patterns.
- Supports the infra side of xvision rather than the strategy side.

## Onchain / agentic strategy tooling

- [Agentic Strategy Tooling for XVision](/docs?slug=agentic-strategy-tooling) — 30-day synthesis of how traders use Dune, Nansen, Glassnode, The Graph, QuickNode, Alchemy, Moralis, Chainbase, Etherscan, and Chainalysis.

## Implementation Plans

<section id="implementation-plans">
  <h2>Implementation Plans</h2>
  <p>These are the current implementation plans tied to xvision's roadmap, written as HTML so the wiki can render them as a clean index.</p>

  <h3>Autoresearcher and agent-planning track</h3>
  <ul>
    <li><a href="/docs/superpowers/plans/2026-05-29-autoresearcher-research-synthesis-plan.md">Autoresearcher Research Synthesis Plan</a> — keep strategy handoff, meta-strategies, lineage / provenance, cycle seals, judge / canary / inversion / diversity checks, and local dashboard or CLI review; drop Slack/email approvals.</li>
    <li><a href="/docs/superpowers/plans/2026-05-29-plan-1-data-integrations-capabilities.md">Plan 1: Data Integrations as Capabilities</a> — expose external data sources as first-class capabilities.</li>
    <li><a href="/docs/superpowers/plans/2026-05-29-plan-2-multi-agent-routing.md">Plan 2: Multi-Agent Routing</a> — named agents, stable handoffs, and graph-based orchestration.</li>
    <li><a href="/docs/superpowers/plans/2026-05-29-plan-3-loose-framework-bots.md">Plan 3: Loose-Framework Bots</a> — research, risk, and portfolio bots without hidden orchestration.</li>
    <li><a href="/docs/superpowers/plans/2026-05-29-xvision-local-llama-cpp-ollama-providers.md">Local llama.cpp / Ollama Providers</a> — local-provider support with explicit provenance.</li>
    <li><a href="/docs/superpowers/plans/2026-05-29-chat-rail-context-switching.md">Chat Rail Context Switching</a> — workspace / category / page / object context transitions.</li>
    <li><a href="/docs/superpowers/plans/2026-05-29-dspy-autoresearcher-integration.md">DSPy × Autoresearcher Integration</a> — capability-first optimizer coverage, holdout discipline, and autoresearcher consumption of optimizer artifacts.</li>
    <li><a href="/docs/superpowers/plans/2026-05-29-agentless-mechanistic-strategies.md">Agentless Mechanistic Strategies</a> — deterministic strategy paths with no hidden agent loop.</li>
  </ul>

  <h3>xvn trace / observability track</h3>
  <ul>
    <li><a href="/docs/superpowers/plans/xvn/README.md">xvn Plans Index</a> — directory index for the xvn plan set.</li>
    <li><a href="/docs/superpowers/plans/xvn/2026-05-29-xvn-clinesdk-gap-analysis.md">xvn ClineSDK Gap Analysis</a> — compare the research report against what xvision already has and isolate the real gaps.</li>
    <li><a href="/docs/superpowers/plans/xvn/2026-05-29-xvn-plan-gap-answers.md">xvn Plan Gap Answers</a> — answer the open questions for Plans 1–4 and the ClineSDK trace surface.</li>
    <li><a href="/docs/superpowers/plans/xvn/2026-05-29-xvn-trace-observability-plan.md">xvn Trace Observability Plan</a> — make the redesigned trace the main operator surface for observability.</li>
  </ul>

  <p>The main theme is not "add a second reporting system." The main theme is to make the newly redesigned trace surface the operator view for observability, while keeping the SQLite ledger and trajectory store as the sources of truth.</p>
</section>

## QA passes

- [XVN QA Pass 1 — Dashboard + Settings](/docs?slug=2026-06-01-xvn-qa-pass-1-dashboard-settings) — route / home-state pass, including the stale danger-zone alias check.
- [XVN QA Pass 2 — Live Alpaca + Agents / Authoring](/docs?slug=2026-06-01-xvn-qa-pass-2-live-alpaca-agents) — live-mode gating, agent authoring, skills, and memory surfaces.

## Active Board items

These are the remaining live items from `team/board.md` after cleaning out the merged/stale entries.

### agent-graph-2026-05-22 — capability-first refactor cascade

- [agent-graph-unified-recorder](/docs?slug=xvision#implementation-plans) — P1 recorder; deferred behind Phase B.
- [agent-graph-template-capabilities](/docs?slug=xvision#implementation-plans) — P2 leaf; deferred behind Phase A.

### memory-safety-and-observability-2026-05-22 — V2D follow-up

- memory-aware-eval-findings — P2 leaf; deferred until `memory-provenance-in-decisions-trace` is in place.

### cli-operator-safety-wave-b-2026-05-22 — model-bakeoff cluster

- [cli-strategy-clone-model-override](/docs?slug=xvision#implementation-plans) — P1 leaf; ready.

### charts-section-2026-05-23 — Track B of chart-rework spec

- [charts-section-b0](/docs?slug=xvision#implementation-plans) — P0 foundation; ready.
- B1 (`charts-section-b1`) — Dark Minimal Strategy Dashboard.
- B2 (`charts-section-b2`) — Comparison AB Scalable.
- B3 (`charts-section-b3`) — AI Annotation Chart.
- B4 (`charts-section-b4`) — Gradient Warm Hero.
- B5 — review checkpoint.
- B-rollout — drop the `xvn.chartv2=1` cookie gate.
- F-CHART-LIQHEAT — followup, Chart 04 Liquidation Heatmap.

### chart-context-2026-05-29 — scenario-aware chart prompt surface

- F45 [XVision] — chart context for scenario-aware prompt queries, so the chart can answer "what is the setup here?" with the right scenario name, timeframe, strategy, and live state.
- This is the wiki-visible home for the `FOLLOWUPS.md` item and the natural bridge from the TradingView-style idea to the chart / scenario surfaces.

### cli-accessibility-2026-05-30 — xvn base URL env var + remote command parity

- F46 [Shared] — make the xvn CLI easier to reach by adding a first-class base-URL environment variable and relaxing remote-command restrictions for safe operator verbs.
- This is the wiki-visible home for the operator-accessibility follow-up; it keeps the live CLI path discoverable without forcing helper-script detours.

## Bottom line

If a doc changes how xvision should *decide*, *evaluate*, or *execute* trading behavior, it belongs here.
