# XVision / Agentic Trading

This page collects the trading-specific docs that map directly to xvision: strategy design, evaluation, agent tooling, and trading infrastructure.

## Why these belong here

These reports all inform the same product surface: how xvision should reason about strategies, what should stay deterministic, how to evaluate agentic behavior, and which infra choices make the system safer and faster.

## Included reports

### Agentic Trading Strategy Research: Filters, Prompts, Indicators, and Market Conditions for LLM-Assisted Trading

- Emphasizes a hard split between deterministic guardrails and LLM policy decisions.
- Says the LLM is best used for contextual reasoning over compressed market state, not raw alpha generation.
- Recommends multi-agent decomposition with structured JSON handoffs for auditability.
- Calls out memory hygiene, backtest/live separation, and outcome-bias prevention as non-negotiable.

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

## Bottom line

If a doc changes how xvision should *decide*, *evaluate*, or *execute* trading behavior, it belongs here.
