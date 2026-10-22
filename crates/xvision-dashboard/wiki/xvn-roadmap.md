# XVN Roadmap

This is the single visible roadmap for XVision / XVN.
It combines the current V2 board, the follow-up queue, and the plan set we made today.

## Countdown

<div>
  <strong>Submission deadline:</strong> 2026-06-15<br/>
  <strong>Turing Hackathon end date:</strong> 2026-06-15<br/>
  <strong>Days left:</strong> 17 days<br/>
  <strong>As of:</strong> 2026-05-29 UTC<br/>
  <strong>Branch:</strong> <code>hackathon/turing</code>
</div>

## Today’s plan set

These are the new plan docs we made today and want on the roadmap:

- [Autoresearcher Research Synthesis Plan](/docs/superpowers/plans/2026-05-29-autoresearcher-research-synthesis-plan.md)
- [Plan 1: Data Integrations as Capabilities](/docs/superpowers/plans/2026-05-29-plan-1-data-integrations-capabilities.md)
- [Plan 2: Multi-Agent Routing](/docs/superpowers/plans/2026-05-29-plan-2-multi-agent-routing.md)
- [Plan 3: Loose-Framework Bots](/docs/superpowers/plans/2026-05-29-plan-3-loose-framework-bots.md)
- [Plan 4: Local llama.cpp / Ollama Providers](/docs/superpowers/plans/2026-05-29-xvision-local-llama-cpp-ollama-providers.md)
- [Chat Rail Context Switching](/docs/superpowers/plans/2026-05-29-chat-rail-context-switching.md)
- [DSPy × Autoresearcher Integration](/docs/superpowers/plans/2026-05-29-dspy-autoresearcher-integration.md)
- [Agentless Mechanistic Strategies](/docs/superpowers/plans/2026-05-29-agentless-mechanistic-strategies.md)

## New xvision roadmap items

- UI pass and dashboard update
- Live Alpaca testing
- Skills, capabilities, and "agent types" refactor

## xvn plans index

This section combines the old xvn plan index with the roadmap, so there is one visible place to scan the work.

### xvn plan set

- [xvn ClineSDK Gap Analysis](/docs/superpowers/plans/xvn/2026-05-29-xvn-clinesdk-gap-analysis.md) — compare the research report against what xvision already has, and mark the remaining gaps.
- [xvn Plan Gap Answers](/docs/superpowers/plans/xvn/2026-05-29-xvn-plan-gap-answers.md) — answer the plan-specific open questions for data integrations, routing, bot roles, providers, and trace observability.
- [xvn Trace Observability Plan](/docs/superpowers/plans/xvn/2026-05-29-xvn-trace-observability-plan.md) — implement the redesigned trace / observability surface so operators can inspect live, record, and replay runs in one place.

### roadmap / index links

- [xvn Plans Index](/docs/superpowers/plans/xvn/README.md) — this directory's landing page, now folded into the roadmap.

## V2 board snapshot

### Complete / archived

- **V2A** — onboarding & docs: complete.
- **V2B** — security & operability: complete.
- **V2D** — agent memory: complete.
- **V2D follow-up v1.1** (`v2d-memory-cli-and-api`): merged; memory management CLI, HTTP API, memory tab, workspace memory page, and docs shipped.
- **V2E** — eval accuracy & trace surface: complete.
- **V2F** — strategy authoring & user knowledge: complete.
- **Memory safety / observability follow-ups**: archived as merged.
  - `memory-forget-undo-snapshot` — done.
  - `memory-provenance-in-decisions-trace` — done.
  - `memory-aware-eval-findings` — done.

### Next intake

- **V2C — on-chain identity**
  - 7. Mantle Sepolia identity/reputation address deploy
  - 8. Strategy NFT mint + readback flow
  - 9. Testnet marketplace list/buy/sell/delegate flow
  - 10. Reputation + validation receipt write/readback

## Follow-ups and research queue

### Strategy Loom / hackathon track

This is the SLF queue in `FOLLOWUPS.md`.
It is the main hackathon workstream on `hackathon/turing`.

- **SLF1** — cut `hackathon/turing` branch + initial scaffolding
- **SLF2** — execute ADR 0008 ops runbook on Mantle Sepolia
- **SLF3** — mint per-strategy NFT on `ab_compare` startup
- **SLF4** — per-cycle Reputation Registry write path
- **SLF5** — Validation Registry signed-oracle backtest receipts
- **SLF6** — onchain strategy baselines
- **SLF7** — TA baselines
- **SLF8** — strategy genealogy / `program.md` versioning
- **SLF9** — evening Karpathy loop
- **SLF10** — Next.js dashboard: ladder, genealogy, delegate
- **SLF11** — risk-preset configuration wired to `xvision-risk`
- **SLF13** — cross-pollination from other agents’ reputation
- **SLF14** — SMA(30) and SMA(90) on `IndicatorPanel`
- **SLF15** — pluggable Trader stage / `TraderBackend` trait
- **SLF16** — demo polish, pitch video, README, submission package
- **F34** — ERC-8004 reputation leaderboards / gamification surface

### Shared follow-ups still worth tracking

- **F5** — Orderly testnet credentials + smoke trade
- **F6** — `cycle_id` reuse guard in the harness
- **F7** — lookahead-bias boundary-condition test
- **F8** — document `MarketSnapshot` invariants
- **F19** — re-adopt `orderly-connector-rs` when the `zeroize` pin loosens
- **F20** — upstream PR to gate the Solana stack in `orderly-connector-rs`
- **F22** — add `VetoReason::TakeProfitTooTight`
- **F24** — DeepSeek via OpenAI-compat as the default hackathon intern path
- **F26** — bump GitHub Actions off Node 20 before the runner deprecation
- **F27** — install customizer / interactive module selection
- **F28** — plugin architecture for xvn
- **F29** — agent social feed
- **F30** — custom-scenario eval
- **F31** — scenario replay modes
- **F33** — chart rework
- **F38** — QA6 dashboard remediation
- **F39** — serve chart images over the CLI for evals
- **F41** — eval contract honesty + agent-graph composition
- **F42** — memory safety + observability (archived as merged)
- **F44** — Orderly market refresh

### Archived follow-ups for context

- **F35** — dashboard API auth: done
- **F36** — guided tours: done
- **F37** — remote CLI orphan recovery: done
- **F40** — scenario display name + eval provider preflight: done

## Source anchors

The underlying repo sources behind this page are:

- `FOLLOWUPS.md` — the operational queue.
- `team/board-v2.md` — the V2 execution board.
- `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` — the umbrella V2–V4 plan.

## Bottom line

If you want one place to see what is *done*, what is *next*, and what is still *waiting on research or follow-up*, this is the page to use.
