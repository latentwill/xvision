# Plans & Specs Index

> **Auto-generated:** 2026-06-01  
> **Scope:** `docs/superpowers/plans/` (94 files) + `docs/superpowers/specs/` (56 files)  
> **Status legend:** ACTIVE = in progress or recently completed; COMPLETE = shipped to main; ARCHIVED = superseded; REFERENCE = design/roadmap, not an execution plan  
> **Note:** The Cline-specific sub-index is at `plans/2026-05-24-cline-runtime-unification-INDEX.md` — that file is a cross-referenced inheritance ledger for the 5 Cline stages and is not replaced here.

---

## Autoresearcher / Optimizer

The subsystem was renamed from "autoresearcher" → ops surface "Optimizer" / codename `autooptimizer` in May 2026. Pre-rename docs retain the `autoresearcher` name.

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md](2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md) | Autoresearcher AR-1 — Mutator + Lineage Store + Numeric Gate + CycleSeal | 2026-05-09 | COMPLETE | First wave: mutation engine, lineage tracking, gate/seal primitives |
| [2026-05-09-autoresearcher-2-cycle-judge-evals.md](2026-05-09-autoresearcher-2-cycle-judge-evals.md) | Autoresearcher AR-2 — Cycle Orchestrator + Judge + Canary + Inversion + Diversity | 2026-05-09 | COMPLETE | Cycle orchestrator (T9), LLM judge (T4), honesty-check canary (T6), inversion-pair eval (T5), diversity-decay (T7), mutator ladder (T8) |
| [2026-05-09-autoresearcher-3-dashboard.md](2026-05-09-autoresearcher-3-dashboard.md) | Autoresearcher AR-3 — Dashboard (5 views) + SSE + Mutator-Skill Ladder UI | 2026-05-09 | COMPLETE | 5 dashboard views (live cycle, genealogy, diff inspector, experiment-writer ladder, provenance), SSE bridge |
| [2026-05-27-autoresearcher-master-implementation-spine.md](2026-05-27-autoresearcher-master-implementation-spine.md) | Autoresearcher master implementation plan — the spine | 2026-05-27 | ACTIVE | Master spine tying AR-1/AR-2/AR-3 together; tracks open follow-ups and overnight autonomous sessions |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-09-karpathy-autoresearcher-design.md](../specs/2026-05-09-karpathy-autoresearcher-design.md) | Karpathy Autoresearcher — Design | 2026-05-09 | REFERENCE | Original design: self-improving strategy loop inspired by Karpathy's autoresearcher concept |
| [2026-05-27-autoresearcher-terminology-lock.md](../specs/2026-05-27-autoresearcher-terminology-lock.md) | Autoresearcher terminology lock — 2026-05-27 | 2026-05-27 | COMPLETE | Locked operator-facing vs developer-facing name pairs for all autoresearcher surfaces |

---

## Marketplace & Blockchain

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-10-blockchain-1-non-custodial-wallets-plan.md](2026-05-10-blockchain-1-non-custodial-wallets-plan.md) | Blockchain Plan #1 — Non-Custodial Agent Wallets | 2026-05-10 | COMPLETE | Non-custodial wallet plan: Ed25519 keys, Mantle deployment, xvision-identity crate |
| [2026-05-10-blockchain-1-non-custodial-wallets-amendments.md](2026-05-10-blockchain-1-non-custodial-wallets-amendments.md) | Wallet Plan #1 — Amendments | 2026-05-10 | COMPLETE | Amendments to the wallet plan before execution |
| [2026-05-25-testnet-venue-plan.md](2026-05-25-testnet-venue-plan.md) | Testnet Venue Plan | 2026-05-25 | ACTIVE | Testnet venue for on-chain strategy execution before mainnet |
| [2026-05-26-blockchain-plan-navigation.md](2026-05-26-blockchain-plan-navigation.md) | Blockchain Plan — Navigation Doc | 2026-05-26 | REFERENCE | Navigation guide tying together the blockchain/marketplace plan family |
| [2026-05-26-marketplace-design-direction.md](2026-05-26-marketplace-design-direction.md) | Marketplace + Identity — Design Direction | 2026-05-26 | REFERENCE | High-level design direction reconciling 6 earlier docs into a Phase 0–8 arc |
| [2026-05-26-marketplace-program-strategy.md](2026-05-26-marketplace-program-strategy.md) | Marketplace Program — Strategy & Front Door | 2026-05-26 | ACTIVE | Phase 0–8 master strategy; Phase F (front-end first) COMPLETE on main (PRs #616–#619); deferred items register included |
| [2026-05-26-marketplace-phase-f0-foundation.md](2026-05-26-marketplace-phase-f0-foundation.md) | Marketplace Phase F0 — Foundation | 2026-05-26 | COMPLETE | Foundation routes and fixture-backed MarketplaceData seam |
| [2026-05-26-marketplace-f1-browse.md](2026-05-26-marketplace-f1-browse.md) | Marketplace F1 — Browse Route | 2026-05-26 | COMPLETE | `/marketplace/browse` — strategy listing and filter UI |
| [2026-05-26-marketplace-f2-lineage.md](2026-05-26-marketplace-f2-lineage.md) | Marketplace F2 — Lineage Identity Page | 2026-05-26 | COMPLETE | `/marketplace/lineage/:id` — strategy provenance + on-chain receipts |
| [2026-05-26-marketplace-f3-creator.md](2026-05-26-marketplace-f3-creator.md) | Marketplace F3 — Creator Profile + Lineage Forest | 2026-05-26 | COMPLETE | `/marketplace/creator/:id` — creator profile and strategy tree |
| [2026-05-26-marketplace-f5-sell.md](2026-05-26-marketplace-f5-sell.md) | Marketplace F5 — Seller Onboarding | 2026-05-26 | COMPLETE | `/marketplace/sell` — seller onboarding and strategy listing flow |
| [2026-05-26-marketplace-f6-receipt.md](2026-05-26-marketplace-f6-receipt.md) | Marketplace F6 — Purchase Receipt | 2026-05-26 | COMPLETE | `/marketplace/receipt/:txId` — purchase confirmation and receipt |
| [2026-05-26-marketplace-f-routes-integration-addendum.md](2026-05-26-marketplace-f-routes-integration-addendum.md) | Phase F Route Plans — Integration Addendum | 2026-05-26 | COMPLETE | Binding integration addendum for all F-route implementations |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-08-smart-contract-surface-design.md](../specs/2026-05-08-smart-contract-surface-design.md) | Smart Contract Surface — Design | 2026-05-08 | REFERENCE | Early smart contract surface design |
| [2026-05-09-non-custodial-agent-wallets-design.md](../specs/2026-05-09-non-custodial-agent-wallets-design.md) | Non-Custodial Agent Wallets — Design | 2026-05-09 | REFERENCE | Wallet design: key custody model, signing flow, Mantle integration |
| [2026-05-09-marketplace-plugin-design.md](../specs/2026-05-09-marketplace-plugin-design.md) | Marketplace Plugin — Design | 2026-05-09 | REFERENCE | Early marketplace plugin concept |
| [2026-05-25-global-signal-producer.md](../specs/2026-05-25-global-signal-producer.md) | Global / Pair Signal Producer + Cross-Asset Selector | 2026-05-25 | ACTIVE | Design for cross-asset signal routing (Phase 6 deferred item) |
| [2026-05-26-marketplace-phase-f-frontend-design.md](../specs/2026-05-26-marketplace-phase-f-frontend-design.md) | Marketplace Phase F — Frontend on Fixtures | 2026-05-26 | COMPLETE | Frontend-first Phase F design spec (fixture-backed MarketplaceData seam) |
| [2026-05-26-marketplace-phase1-metadata-data-contract.md](../specs/2026-05-26-marketplace-phase1-metadata-data-contract.md) | Phase 1 — Marketplace Metadata & Data-Contract Spec | 2026-05-26 | REFERENCE | On-chain metadata contract spec for Phase 1 smart contract deployment |

---

## Eval Engine

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-08-eval-engine-plan.md](2026-05-08-eval-engine-plan.md) | Eval Engine Implementation Plan | 2026-05-08 | COMPLETE | Foundation: RunStore, Executor trait, BacktestExecutor, PaperExecutor |
| [2026-05-11-v1-gaps-multi-agent.md](2026-05-11-v1-gaps-multi-agent.md) | v1 Gaps — Multi-Agent Implementation Spec | 2026-05-11 | COMPLETE | 8 gap tracks (A–H) identified post-merge; all closed same day (see closure status note) |
| [2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md](2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md) | Custom-Scenario Eval — M1: Bars cache + Alpaca fetcher + asset unlock | 2026-05-11 | COMPLETE | Bars cache, Alpaca fetcher, per-asset unlock |
| [2026-05-11-custom-scenario-2-scenario-table-cli.md](2026-05-11-custom-scenario-2-scenario-table-cli.md) | Custom-Scenario Eval — M2: Scenario table + CLI + capital/risk move | 2026-05-11 | COMPLETE | Scenario table, CLI verbs, capital/risk configuration move |
| [2026-05-11-custom-scenario-3-dashboard-wizard.md](2026-05-11-custom-scenario-3-dashboard-wizard.md) | Custom-Scenario Eval — M3: Dashboard wizard + inline form + run launcher | 2026-05-11 | COMPLETE | Dashboard scenario wizard and run launcher |
| [2026-05-11-perps-eval-simulator.md](2026-05-11-perps-eval-simulator.md) | Perpetuals eval simulator — leverage, funding, liquidation in backtest | 2026-05-11 | ACTIVE | Perps simulation: leverage, funding rates, liquidation mechanics |
| [2026-05-12-eval-per-agent-metrics.md](2026-05-12-eval-per-agent-metrics.md) | Eval per-agent metrics | 2026-05-12 | ACTIVE | Per-agent cost and performance metrics in eval results |
| [2026-05-22-orderly-multi-asset-expansion.md](2026-05-22-orderly-multi-asset-expansion.md) | Plan — Orderly multi-asset expansion (post-F18) | 2026-05-22 | ACTIVE | Multi-asset expansion via Orderly venue after F18 baseline |
| [2026-05-24-multi-asset-strategies.md](2026-05-24-multi-asset-strategies.md) | Multi-Asset Strategies Implementation Plan | 2026-05-24 | COMPLETE | Multi-asset backtest (pooled fan-out, asset-free scenarios) — completed in PR #593 |
| [2026-05-25-multi-asset-followups.md](2026-05-25-multi-asset-followups.md) | Multi-Asset Follow-Up Plan | 2026-05-25 | ACTIVE | Follow-ups from multi-asset wave (migration-ladder dedup deferred, global-signal runtime) |
| [2026-05-28-portfolio-execution-mode.md](2026-05-28-portfolio-execution-mode.md) | Portfolio Execution Mode — Implementation Plan | 2026-05-28 | ACTIVE | Portfolio-level execution mode for capital allocation across multiple strategies |
| [2026-05-29-agentless-mechanistic-strategies.md](2026-05-29-agentless-mechanistic-strategies.md) | Agentless Mechanistic Strategies Implementation Plan | 2026-05-29 | ACTIVE | Pure rule-based strategies without LLM agent, for baseline comparison |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-08-eval-engine-decisions-so-far.md](../specs/2026-05-08-eval-engine-decisions-so-far.md) | Eval Engine — Brainstorm Decisions So Far | 2026-05-08 | REFERENCE | Early decision log for eval engine architecture |
| [2026-05-08-eval-engine-design.md](../specs/2026-05-08-eval-engine-design.md) | Eval Engine — Design | 2026-05-08 | REFERENCE | Eval engine design: run lifecycle, executor trait, store contract |
| [2026-05-11-custom-scenario-eval-design.md](../specs/2026-05-11-custom-scenario-eval-design.md) | Custom-Scenario Eval — Design | 2026-05-11 | REFERENCE | Custom scenario design: bars cache, scenario table, wizard flow |
| [2026-05-11-freqtrade-metrics-charts-design.md](../specs/2026-05-11-freqtrade-metrics-charts-design.md) | FreqTrade-Inspired Backtest Metrics & Charts — Design | 2026-05-11 | REFERENCE | Metrics and chart design inspired by FreqTrade's eval surface |
| [2026-05-14-alpaca-paper-eval-surface-design.md](../specs/2026-05-14-alpaca-paper-eval-surface-design.md) | Alpaca Paper Eval Surface — Design | 2026-05-14 | REFERENCE | Paper eval surface for Alpaca broker; live eval follow-on |
| [2026-05-15-eval-review-agent.md](../specs/2026-05-15-eval-review-agent.md) | Eval Review Agent | 2026-05-15 | COMPLETE | Rule-based eval review agent spec |
| [2026-05-16-q15-eval-resilience-and-contracts.md](../specs/2026-05-16-q15-eval-resilience-and-contracts.md) | Q15 — Eval resilience + per-object data contracts | 2026-05-16 | COMPLETE | Eval retry, idempotency, and per-object data contract spec (Q15) |
| [2026-05-23-compare-ab-respec.md](../specs/2026-05-23-compare-ab-respec.md) | Compare AB respec — post Charts v2 | 2026-05-23 | ACTIVE | Re-spec for compare view after TradingView → KlineChart migration |
| [2026-05-24-multi-asset-strategies-design.md](../specs/2026-05-24-multi-asset-strategies-design.md) | Multi-Asset Strategies — Design | 2026-05-24 | COMPLETE | Design for multi-asset pooled fan-out and per-asset isolation |
| [2026-05-25-execution-capital-modes.md](../specs/2026-05-25-execution-capital-modes.md) | Execution & Capital Modes — Behavior Spec | 2026-05-25 | COMPLETE | Backtest/live/paper mode behavior spec; paper mode deprecated as product concept |

---

## Charts

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-11-tradingview-charts-1-replace-svg.md](2026-05-11-tradingview-charts-1-replace-svg.md) | TradingView Charts — M1: Replace SVG sparklines | 2026-05-11 | COMPLETE | Replaced SVG sparklines with TradingView lightweight-charts (later migrated again to KlineChart) |
| [2026-05-11-tradingview-charts-2-scenario-strategy.md](2026-05-11-tradingview-charts-2-scenario-strategy.md) | TradingView Charts — M2: Scenario + Strategy charts | 2026-05-11 | COMPLETE | Scenario and strategy chart pages with TradingView (superseded by v2) |
| [2026-05-11-tradingview-charts-3-live-wizard-preview.md](2026-05-11-tradingview-charts-3-live-wizard-preview.md) | TradingView Charts — M3: Live cockpit + wizard preview | 2026-05-11 | COMPLETE | Live cockpit and wizard chart preview (superseded by v2) |
| [2026-05-23-charts-section-b0-foundation.md](2026-05-23-charts-section-b0-foundation.md) | Charts Section B0 — Foundation | 2026-05-23 | COMPLETE | Charts dashboard section + `Strategy.color` field — merged in PR #556 |
| [2026-05-23-charts-section-b1-overview-dashboard.md](2026-05-23-charts-section-b1-overview-dashboard.md) | Charts Section B1 — Dark Minimal Strategy Dashboard | 2026-05-23 | COMPLETE | `/charts/overview` dark minimal strategy overview dashboard |
| [2026-05-23-charts-section-b2-comparison-ab.md](2026-05-23-charts-section-b2-comparison-ab.md) | Charts Section B2 — Comparison AB Scalable | 2026-05-23 | COMPLETE | `/charts/compare` scalable A/B comparison view |
| [2026-05-23-charts-section-b3-ai-annotation.md](2026-05-23-charts-section-b3-ai-annotation.md) | Charts Section B3 — AI Annotation Chart | 2026-05-23 | COMPLETE | `/charts/annotated` AI-annotated chart view |
| [2026-05-23-charts-section-b4-gradient-hero.md](2026-05-23-charts-section-b4-gradient-hero.md) | Charts Section B4 — Gradient Warm Hero Dashboard | 2026-05-23 | COMPLETE | `/charts/hero` gradient hero dashboard |
| [2026-05-23-charts-section-b5-hero-default-review.md](2026-05-23-charts-section-b5-hero-default-review.md) | Charts Section B5 — Hero-default review checkpoint | 2026-05-23 | COMPLETE | Review checkpoint after hero chart section; accepted gaps documented |
| [2026-05-26-tradingview-to-klinechart-uplot-migration.md](2026-05-26-tradingview-to-klinechart-uplot-migration.md) | Retire TradingView → KlineChart + uPlot v2 | 2026-05-26 | COMPLETE | Full chart v2 migration — all charts migrated in PR #614; v1 lightweight-charts deleted |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-11-tradingview-charts-design.md](../specs/2026-05-11-tradingview-charts-design.md) | TradingView Charts — Design | 2026-05-11 | ARCHIVED | Original TradingView charts design (superseded by KlineChart+uPlot v2) |
| [2026-05-14-tradingview-lightweight-eval-surface-design.md](../specs/2026-05-14-tradingview-lightweight-eval-surface-design.md) | TradingView Lightweight Eval Surface — Design | 2026-05-14 | ARCHIVED | Eval surface design with TradingView lightweight-charts (superseded) |
| [2026-05-21-chart-rework-klinecharts-uplot.md](../specs/2026-05-21-chart-rework-klinecharts-uplot.md) | Chart rework — KlineCharts + uPlot (Tracks A + B) | 2026-05-21 | COMPLETE | Design spec for KlineCharts + uPlot migration |
| [2026-05-26-tradingview-to-klinechart-uplot-migration-design.md](../specs/2026-05-26-tradingview-to-klinechart-uplot-migration-design.md) | Retire TradingView → KlineChart + uPlot v2 surfaces | 2026-05-26 | COMPLETE | Detailed migration design for each chart surface |
| [2026-05-23-live-annotation-producer-and-review-autofire.md](../specs/2026-05-23-live-annotation-producer-and-review-autofire.md) | Live annotation producer + review auto-fire setting | 2026-05-23 | COMPLETE | Live annotation production and automatic review triggering spec |

---

## Memory / Cortex

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-11-cortex-memory-integration-plan.md](2026-05-11-cortex-memory-integration-plan.md) | Cortex memory integration for xvision — implementation plan | 2026-05-11 | ARCHIVED | Early cortex memory plan; superseded by the 2026-05-21 version |
| [2026-05-21-cortex-memory-integration-plan.md](2026-05-21-cortex-memory-integration-plan.md) | Cortex Memory Integration Plan (V2D) | 2026-05-21 | COMPLETE | V2D cortex memory integration: flywheel, DSPy hook-ins, memory-aware eval findings |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-24-cortex-memory-cline-dspy-flywheels.md](../specs/2026-05-24-cortex-memory-cline-dspy-flywheels.md) | Cortex memory + ClineSDK + DSPy — self-improvement flywheel | 2026-05-24 | COMPLETE | Full spec: memory store, ClineSDK wiring, DSPy optimization loop, flywheel integration |

---

## Agent Graph / Cline Runtime

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-17-cline-sdk-agent-replacement-wave1.md](2026-05-17-cline-sdk-agent-replacement-wave1.md) | Cline SDK Agent Replacement — Wave 1 | 2026-05-17 | COMPLETE | Replace LlmDispatch with Cline SDK for trader/intern/critic slots (wave 1) |
| [2026-05-17-cline-sdk-agent-replacement-wave2.md](2026-05-17-cline-sdk-agent-replacement-wave2.md) | Cline SDK Agent Replacement — Wave 2 | 2026-05-17 | COMPLETE | Wave 2: router, regime, and filter slots |
| [2026-05-24-cline-runtime-unification-INDEX.md](2026-05-24-cline-runtime-unification-INDEX.md) | Cline Runtime Unification — Plan Index & Inheritance Ledger | 2026-05-24 | REFERENCE | **Cross-referenced sub-index** for all 5 Cline stages; see this file for stage inheritance and acceptance gates |
| [2026-05-24-cline-stage0-acpx-purge.md](2026-05-24-cline-stage0-acpx-purge.md) | Cline Stage 0: ACPX Purge + License Guard | 2026-05-24 | COMPLETE | Remove ACPX extension dependency; add license guard |
| [2026-05-24-cline-stage1-live-path.md](2026-05-24-cline-stage1-live-path.md) | Cline Stage 1: Live Path | 2026-05-24 | COMPLETE | Wire Cline SDK on the live execution path |
| [2026-05-24-cline-stage2-trajectory-record.md](2026-05-24-cline-stage2-trajectory-record.md) | Cline Stage 2: Trajectory Record | 2026-05-24 | COMPLETE | Record agent decision trajectories for replay and eval |
| [2026-05-24-cline-stage3-replay-unify-eval.md](2026-05-24-cline-stage3-replay-unify-eval.md) | Cline Stage 3: Replay + Unify Eval | 2026-05-24 | COMPLETE | Unified eval over live-record and replay paths |
| [2026-05-24-cline-stage4-throughput-hardening.md](2026-05-24-cline-stage4-throughput-hardening.md) | Cline Stage 4: Throughput Hardening | 2026-05-24 | COMPLETE | Throughput and concurrency hardening for record/replay pipeline |
| [2026-05-25-cline-live-followups.md](2026-05-25-cline-live-followups.md) | Cline Runtime + Alpaca Paper Live Follow-Up Plan | 2026-05-25 | ACTIVE | Follow-ups: live record→sidecar wiring, Alpaca paper live gaps |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-17-cline-sdk-agent-replacement-design.md](../specs/2026-05-17-cline-sdk-agent-replacement-design.md) | Cline SDK Agent Replacement — Design | 2026-05-17 | COMPLETE | Design for replacing LlmDispatch calls with Cline SDK |
| [2026-05-24-cline-runtime-unification-design.md](../specs/2026-05-24-cline-runtime-unification-design.md) | Umbrella design — Cline SDK runtime unification | 2026-05-24 | COMPLETE | Umbrella design document for all 5 Cline runtime unification stages |
| [2026-05-24-cline-record-throughput-target.md](../specs/2026-05-24-cline-record-throughput-target.md) | Record-Pass Throughput Target — Stage 4 | 2026-05-24 | COMPLETE | Throughput targets and benchmark spec for Stage 4 |
| [2026-05-24-chat-rail-and-strategy-agents-evaluation.md](../specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md) | Chat rail, DSPy, and strategy agents — implementation plan | 2026-05-24 | COMPLETE | Phase 0+1+2 safety core shipped; Research/Act enforcement, tool policy, DSPy hook |
| [2026-05-22-capability-first-agent-model-and-graph-composition.md](../specs/2026-05-22-capability-first-agent-model-and-graph-composition.md) | Capability-first agent model + graph composition | 2026-05-22 | ACTIVE | Agent typed-capabilities (Trader/Filter/Critic/Intern/Router) + graph composition design |

---

## Strategy Authoring & Agent Surfaces

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-08-strategy-creation-engine-mvp.md](2026-05-08-strategy-creation-engine-mvp.md) | Strategy Creation Engine — MVP | 2026-05-08 | COMPLETE | MVP plan for strategy authoring pipeline |
| [2026-05-08-strategy-engine-2a-mcp-tools-templates.md](2026-05-08-strategy-engine-2a-mcp-tools-templates.md) | Strategy Engine — Plan 2a (MCP + Tool-Call + 7 Templates) | 2026-05-08 | COMPLETE | 7 strategy templates, MCP tool dispatch |
| [2026-05-08-strategy-engine-2b-skills.md](2026-05-08-strategy-engine-2b-skills.md) | Strategy Engine — Plan 2b (Skills) | 2026-05-08 | COMPLETE | Skills registry and dispatch |
| [2026-05-08-strategy-engine-2c-scheduler-live-exec.md](2026-05-08-strategy-engine-2c-scheduler-live-exec.md) | Strategy Engine — Plan 2c (Durable Scheduler + Live Execution) | 2026-05-08 | COMPLETE | Durable scheduler and live execution daemon |
| [2026-05-08-strategy-engine-2d-dashboard-wizard.md](2026-05-08-strategy-engine-2d-dashboard-wizard.md) | Strategy Engine — Plan 2d (Web Dashboard + Agent Wizard) | 2026-05-08 | COMPLETE | Web dashboard and strategy creation wizard |
| [2026-05-10-engine-api-foundation.md](2026-05-10-engine-api-foundation.md) | Engine API Foundation | 2026-05-10 | COMPLETE | Engine API foundation: ApiContext, audit, health, dispatch |
| [2026-05-11-agents-page-v1.md](2026-05-11-agents-page-v1.md) | Agents page v1 — minimum useful surface | 2026-05-11 | COMPLETE | Agent library + inline authoring scaffold |
| [2026-05-12-strategies-refactor-agent-composition.md](2026-05-12-strategies-refactor-agent-composition.md) | Strategies refactor — agent composition | 2026-05-12 | COMPLETE | Replaced `StrategyBundle` with `Strategy { agents: Vec<AgentRef> }` |
| [2026-05-12-agent-access-and-cli-discoverability.md](2026-05-12-agent-access-and-cli-discoverability.md) | Agent Access and CLI Discoverability | 2026-05-12 | COMPLETE | CLI discoverability improvements and agent access control |
| [2026-05-21-v2f-strategies-folder-and-template-refactor.md](2026-05-21-v2f-strategies-folder-and-template-refactor.md) | V2F — Strategies folder + template refactor | 2026-05-21 | COMPLETE | Removed template_registry; strategy folder structure refactor |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-08-strategy-creation-engine-design.md](../specs/2026-05-08-strategy-creation-engine-design.md) | Strategy Creation Engine — Design | 2026-05-08 | REFERENCE | Original strategy creation engine design doc |
| [2026-05-08-slot-machine-design.md](../specs/2026-05-08-slot-machine-design.md) | Slot Machine Strategy — Design | 2026-05-08 | REFERENCE | Early slot-machine strategy concept (deferred) |
| [2026-05-12-agent-access-and-cli-discoverability-spec.md](../specs/2026-05-12-agent-access-and-cli-discoverability-spec.md) | Agent Access and CLI Discoverability — Spec | 2026-05-12 | COMPLETE | Spec for agent access gating and CLI discoverability |
| [2026-05-15-xvn-agent-run-system-spec.md](../specs/2026-05-15-xvn-agent-run-system-spec.md) | xvn Agent Run System | 2026-05-15 | COMPLETE | Agent run lifecycle spec: dispatch, record, trace, replay |
| [2026-05-22-agent-firing-filter-operator-surface.md](../specs/2026-05-22-agent-firing-filter-operator-surface.md) | Agent firing-filter operator surface | 2026-05-22 | COMPLETE | Operator-facing filter DSL for agent firing conditions |
| [2026-05-25-agent-cli-press-audit.md](../specs/2026-05-25-agent-cli-press-audit.md) | Agent CLI — cli-printing-press Audit & Punch List | 2026-05-25 | COMPLETE | CLI surface audit and punch list for agent verbs |

---

## Frontend Surfaces & QA

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-10-frontend-1-foundation-and-strategies.md](2026-05-10-frontend-1-foundation-and-strategies.md) | v1 Frontend — Plan 1: Foundation + Strategies | 2026-05-10 | COMPLETE | SPA shell, Strategies page, routing foundation |
| [2026-05-10-frontend-2-read-only-screens.md](2026-05-10-frontend-2-read-only-screens.md) | v1 Frontend — Plan 2: Read-only screens | 2026-05-10 | COMPLETE | Scenarios, Eval Runs list, run detail pages |
| [2026-05-10-frontend-3-authoring-inspector.md](2026-05-10-frontend-3-authoring-inspector.md) | v1 Frontend — Plan 3: Authoring (Inspector) | 2026-05-10 | COMPLETE | Strategy Inspector with live validation |
| [2026-05-10-frontend-4-agent-surfaces.md](2026-05-10-frontend-4-agent-surfaces.md) | v1 Frontend — Plan 4: Agent surfaces (Wizard, Chat rail, Live preview) | 2026-05-10 | COMPLETE | Agent wizard, chat rail scaffold, live preview |
| [2026-05-10-frontend-5-findings-compare-polish.md](2026-05-10-frontend-5-findings-compare-polish.md) | v1 Frontend — Plan 5: Findings + Compare + Polish | 2026-05-10 | COMPLETE | Findings display, compare view, v1 polish |
| [2026-05-10-chat-rail-persistence-plan.md](2026-05-10-chat-rail-persistence-plan.md) | Chat Rail Persistence Implementation Plan | 2026-05-10 | COMPLETE | Chat session persistence across navigation |
| [2026-05-10-command-palette-plan.md](2026-05-10-command-palette-plan.md) | Command Palette (⌘K) Implementation Plan | 2026-05-10 | COMPLETE | Global ⌘K command palette |
| [2026-05-10-settings-and-onboarding-plan.md](2026-05-10-settings-and-onboarding-plan.md) | Settings & Onboarding Implementation Plan | 2026-05-10 | COMPLETE | Settings tabs, onboarding flow, danger zone |
| [2026-05-11-qa-pass-2-chat-rail-polish.md](2026-05-11-qa-pass-2-chat-rail-polish.md) | QA Pass 2 — Chat-rail Polish | 2026-05-11 | COMPLETE | QA pass focused on chat rail polish issues |
| [2026-05-12-qa-pass-4-remediation.md](2026-05-12-qa-pass-4-remediation.md) | QA Pass 4 Remediation | 2026-05-12 | COMPLETE | QA4 remediation: selective reset, auth gate, danger typed phrases |
| [2026-05-12-qa-pass-4-surface-consistency.md](2026-05-12-qa-pass-4-surface-consistency.md) | QA Pass 4 Surface Consistency | 2026-05-12 | COMPLETE | Surface consistency fixes from QA pass 4 |
| [2026-05-12-pr91-94-unworked-features.md](2026-05-12-pr91-94-unworked-features.md) | PR 91-94 Unworked Features | 2026-05-12 | COMPLETE | Features deferred from PRs 91-94, prioritized for follow-up |
| [2026-05-13-qa6-dashboard-remediation.md](2026-05-13-qa6-dashboard-remediation.md) | QA6 Dashboard Remediation | 2026-05-13 | COMPLETE | QA6 dashboard polish and bug fixes |
| [2026-05-14-chat-rail-inline-charting.md](2026-05-14-chat-rail-inline-charting.md) | Chat Rail Inline Charting | 2026-05-14 | COMPLETE | Inline chart rendering in chat rail responses |
| [2026-05-14-color-themes-light-dark-mode.md](2026-05-14-color-themes-light-dark-mode.md) | Color Themes Light/Dark Mode | 2026-05-14 | COMPLETE | Light/dark mode themes with color token system |
| [2026-05-14-mobile-first-framework.md](2026-05-14-mobile-first-framework.md) | XVN Mobile-First Framework | 2026-05-14 | COMPLETE | Mobile-first responsive framework: `MListCard`, `MobileShell`, breakpoints |
| [2026-05-17-agent-run-observability-plan.md](2026-05-17-agent-run-observability-plan.md) | Agent Run Observability — Implementation Plan | 2026-05-17 | COMPLETE | Trace dock, span timeline, tool-call visualization |
| [2026-05-17-agent-run-observability-ui-implementation-plan.md](2026-05-17-agent-run-observability-ui-implementation-plan.md) | Agent Run Observability UI — Implementation Plan | 2026-05-17 | COMPLETE | UI implementation plan for observability surfaces |
| [2026-05-24-signal-theme-rebrand.md](2026-05-24-signal-theme-rebrand.md) | Signal Theme Rebrand | 2026-05-24 | ACTIVE | Rebrand from xvision to Signal visual identity |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-11-qa-pass-2-spec.md](../specs/2026-05-11-qa-pass-2-spec.md) | QA Pass 2 — Spec | 2026-05-11 | COMPLETE | QA pass 2 spec: chat rail, list polish |
| [2026-05-12-qa-pass-3-spec.md](../specs/2026-05-12-qa-pass-3-spec.md) | QA Pass 3 — Spec | 2026-05-12 | COMPLETE | QA pass 3 spec |
| [2026-05-12-qa-pass-4-surface-consistency-spec.md](../specs/2026-05-12-qa-pass-4-surface-consistency-spec.md) | QA Pass 4 — Surface Consistency Spec | 2026-05-12 | COMPLETE | Surface consistency rules from QA pass 4 |
| [2026-05-14-chat-rail-inline-charting-design.md](../specs/2026-05-14-chat-rail-inline-charting-design.md) | Chat Rail Inline Charting — Design | 2026-05-14 | COMPLETE | Design for inline chart rendering in chat |
| [2026-05-14-color-themes-light-dark-mode-design.md](../specs/2026-05-14-color-themes-light-dark-mode-design.md) | Color Themes and Light/Dark Mode Design | 2026-05-14 | COMPLETE | Color token system, theme spec, dark-mode border rules |
| [2026-05-14-mobile-first-framework-design.md](../specs/2026-05-14-mobile-first-framework-design.md) | XVN Mobile-First Framework — Design | 2026-05-14 | COMPLETE | Mobile-first framework design: breakpoints, list components |
| [2026-05-15-browser-console-logging.md](../specs/2026-05-15-browser-console-logging.md) | Browser Console Logging Pass | 2026-05-15 | COMPLETE | Console logging cleanup and structured log spec |
| [2026-05-15-chat-strategy-agent-authoring-recovery.md](../specs/2026-05-15-chat-strategy-agent-authoring-recovery.md) | Chat Strategy Agent Authoring Recovery | 2026-05-15 | COMPLETE | Recovery flows for interrupted chat-based strategy authoring |
| [2026-05-16-execution-board-process-overhaul.md](../specs/2026-05-16-execution-board-process-overhaul.md) | Execution Board Process Overhaul — Spec | 2026-05-16 | COMPLETE | Team execution board redesign: board.md, MANIFEST, contracts process |
| [2026-05-17-agent-run-observability-ui-design.md](../specs/2026-05-17-agent-run-observability-ui-design.md) | Agent Run Observability — UI Surface Design | 2026-05-17 | COMPLETE | Observability UI design; establishes no-popups rule |
| [2026-05-18-agent-cicd-control-plane.md](../specs/2026-05-18-agent-cicd-control-plane.md) | Agent CI/CD Control Plane — Spec | 2026-05-18 | ACTIVE | CI/CD control plane for automated agent workflows |
| [2026-05-20-standard-list-component.md](../specs/2026-05-20-standard-list-component.md) | Standard List Component — Spec | 2026-05-20 | COMPLETE | `ResponsiveListCard` / `MListCard` spec; establishes `MListSheet` mobile-only exception |
| [2026-06-01-no-filter-creation-warning-design.md](../specs/2026-06-01-no-filter-creation-warning-design.md) | Creation-time "no filter / every-bar" warning — design | 2026-06-01 | ACTIVE | Warning when creating a strategy with no filter (every-bar fire hazard) |
| [2026-06-01-ollama-llamacpp-provider-design.md](../specs/2026-06-01-ollama-llamacpp-provider-design.md) | Ollama and llama.cpp First-Class Provider Support | 2026-06-01 | ACTIVE | Design for first-class Ollama and llama.cpp provider support (PR #702 shipped) |

---

## CLI & Scheduling

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-10-xvn-scheduling-and-agent-cli.md](2026-05-10-xvn-scheduling-and-agent-cli.md) | xvn Scheduling & Agent CLI Surface — Implementation Plan | 2026-05-10 | COMPLETE | Master plan for xvn CLI scheduling and agent verbs |
| [2026-05-10-xvn-scheduling-and-agent-cli-part2.md](2026-05-10-xvn-scheduling-and-agent-cli-part2.md) | xvn Scheduling Plan — Part 2 (Tasks 4–9) | 2026-05-10 | COMPLETE | Engine API tasks 4–9 |
| [2026-05-10-xvn-scheduling-and-agent-cli-part3.md](2026-05-10-xvn-scheduling-and-agent-cli-part3.md) | xvn Scheduling Plan — Part 3 (Tasks 10–14) | 2026-05-10 | COMPLETE | Tool registry + agent runner tasks 10–14 |
| [2026-05-10-xvn-scheduling-and-agent-cli-part4.md](2026-05-10-xvn-scheduling-and-agent-cli-part4.md) | xvn Scheduling Plan — Part 4 (Tasks 14–18) | 2026-05-10 | COMPLETE | Durable scheduler tasks 14–18 |
| [2026-05-10-xvn-scheduling-and-agent-cli-part5.md](2026-05-10-xvn-scheduling-and-agent-cli-part5.md) | xvn Scheduling Plan — Part 5 (Tasks 19–28) | 2026-05-10 | COMPLETE | CLI completeness + polish tasks 19–28 |
| [2026-05-12-ghcr-build-optimization.md](2026-05-12-ghcr-build-optimization.md) | GHCR Build Optimization | 2026-05-12 | COMPLETE | Docker layer caching, build time reduction |
| [2026-05-12-remote-cli-over-tailscale.md](2026-05-12-remote-cli-over-tailscale.md) | Remote CLI Over Tailscale | 2026-05-12 | COMPLETE | `xvn` CLI over Tailscale with auth and rate-limit |
| [2026-05-10-docker-image.md](2026-05-10-docker-image.md) | xvision Docker Image Implementation Plan | 2026-05-10 | COMPLETE | Slim runtime Docker image, GHCR push |
| [2026-05-11-typed-exit-codes.md](2026-05-11-typed-exit-codes.md) | Typed Exit Codes (Plan 2b-followup) | 2026-05-11 | COMPLETE | Typed exit code system for CLI verbs |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-10-xvn-scheduling-and-agent-cli-design.md](../specs/2026-05-10-xvn-scheduling-and-agent-cli-design.md) | xvn Scheduling & Agent CLI Surface — Design | 2026-05-10 | REFERENCE | Design doc for scheduling and CLI surface |
| [2026-05-11-cli-config-and-dispatch-unification-spec.md](../specs/2026-05-11-cli-config-and-dispatch-unification-spec.md) | CLI Config + LLM Dispatch Unification — Spec | 2026-05-11 | COMPLETE | Unified config/dispatch for CLI and engine |
| [2026-05-11-install-customizer-design.md](../specs/2026-05-11-install-customizer-design.md) | Install Customizer — Design | 2026-05-11 | REFERENCE | Install customizer design (post-v1 deferred) |
| [2026-05-12-ghcr-build-optimization-design.md](../specs/2026-05-12-ghcr-build-optimization-design.md) | GHCR Build Optimization — Design | 2026-05-12 | COMPLETE | Build optimization design: layer caching strategy |
| [2026-05-12-remote-cli-over-tailscale-design.md](../specs/2026-05-12-remote-cli-over-tailscale-design.md) | Remote CLI Over Tailscale — Design | 2026-05-12 | COMPLETE | Remote CLI design: Tailscale routing, auth model |
| [2026-05-13-chatrail-file-attach-design.md](../specs/2026-05-13-chatrail-file-attach-design.md) | ChatRail File Attach — Design | 2026-05-13 | ACTIVE | File attachment support in the chat rail |

---

## LLM Providers

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-10-llm-providers-and-per-arm-models-plan.md](2026-05-10-llm-providers-and-per-arm-models-plan.md) | LLM Providers & Per-Arm Models | 2026-05-10 | COMPLETE | Multi-provider support (Anthropic, OpenAI, OpenRouter); per-arm model override |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-10-llm-providers-and-per-arm-models-design.md](../specs/2026-05-10-llm-providers-and-per-arm-models-design.md) | LLM Providers & Per-Arm Models — Design | 2026-05-10 | REFERENCE | Provider/model design: ProviderEntry, dispatch routing |

---

## Terminology & Misc

### Plans

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
| [2026-05-10-terminology-rename-option-b.md](2026-05-10-terminology-rename-option-b.md) | Terminology Rename (Option B) | 2026-05-10 | COMPLETE | Locked terminology: `cycle_id`, `agent_id`, `Strategy`, `Algorithm`, etc. |
| [2026-05-10-leverage-items.md](2026-05-10-leverage-items.md) | Leverage Items | 2026-05-10 | REFERENCE | High-leverage items list for v1 completion |
| [2026-05-10-lab-notebook-plan.md](2026-05-10-lab-notebook-plan.md) | Lab Notebook (`/journal`) | 2026-05-10 | ARCHIVED | Experiment journal/lab-notebook surface (deferred post-v1) |
| [2026-05-10-deferred-archetypes-roadmap.md](2026-05-10-deferred-archetypes-roadmap.md) | Deferred Archetypes — Post-v1 Roadmap | 2026-05-10 | REFERENCE | Post-v1 deferred features: daemon, journal, marketplace (now in-flight) |
| [2026-05-13-v2-v4-action-plan.md](2026-05-13-v2-v4-action-plan.md) | Xvision V2-V4 Action Plan | 2026-05-13 | REFERENCE | Wave-by-wave roadmap for v2–v4; conductor decomposes one wave at a time |

### Specs

| File | Title | Date | Status | Summary |
|---|---|---|---|---|
