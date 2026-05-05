# ADR 0010 — Hackathon Pivot: Strategy Loom + ERC-8004 Marketplace

**Date:** 2026-05-05
**Status:** Accepted
**Phase:** Hackathon sprint (May 5 → Jun 15, 2026)

## Context

Mantle launched the **Turing Test Hackathon 2026**, Phase 2 "AI Awakening" —
$100K prize pool, six tracks, submission window **May 1 → June 15, 2026**
(~5.5 weeks from this ADR's date). Every submitted agent receives an ERC-8004
identity NFT; every decision and outcome is logged on-chain for evaluation.
Judging panel weights on-chain analytics (Nansen, Allora, Caladan, Hashed),
AI infra (Z.ai, Elfa AI, Virtuals), ecosystem (Animoca, BGA, DoraHacks), and
academic (HKU).

The current xianvec stack treats the **control-vector trading arm** —
`TraderArm` with `VectorConfig{Off,On,Random,Orthogonal}` (see `architecture.md`
§7.1, FOLLOWUPS F3) — as the thesis-defining experiment. This requires:

- FP16 weights for vector extraction (~64 GB).
- Q4 GGUF for inference (~20 GB) plus steering hooks (vendored
  `quantized_qwen3`, see ADR 0001).
- Either a beefy local box (M4 Max bottoms at ~0.64 tok/s decode at Q4_K_M
  on candle's Metal kernels) or a rented GPU pod with operator setup
  (RunPod; see `scripts/setup_runpod.sh`).

Three reasons that's the wrong shape to ship as the hackathon submission:

1. **Demoability.** Judges will not provision a GPU pod to evaluate the agent.
   Phase 1 was scored automatically on volume + ROI; Phase 2's "Human vs. AI"
   mechanic implies usability.
2. **Non-technical accessibility.** Prize structure rewards consumer DApps
   and agentic wallets; control-vector tuning is not the right surface for
   the dashboard track.
3. **ERC-8004 fit.** Control vectors don't naturally produce reputation-bearing
   artifacts — a vector is one direction in residual space, not a track
   record. *Strategies* are reputation-bearing units: provenance, performance,
   fork lineage, all map cleanly to the Identity / Reputation / Validation
   registries (ADR 0008).

What's already in the repo that the pivot leans on:

- `xianvec-eval::ab_compare` orchestrator + `xvn ab-compare` runner (Phase 9.1/9.2).
- `xianvec-trader` `Strategy` adapter (TraderArm, F3).
- `xianvec-intern` `AcpxIntern` subprocess backend (F21) — the LLM operator.
- `xianvec-identity` scaffolded against ERC-8004 stub registries (ADR 0008).
- `xianvec-mcp` server exposing indicator tools (F22).
- 7 baseline strategies in `crates/xianvec-eval/src/baselines/`, plus 4 classical-TA
  queued (F15) and 4 onchain queued (F14).

## Decision

**Pivot the hackathon submission to a two-layer system — Strategy Loom (engine)
plus ERC-8004 Strategy Marketplace (surface). Control vectors remain as ONE
strategy variant inside the loom, gated behind a `control-vectors` cargo
feature.**

### Engine layer — Strategy Loom

Multi-strategy evaluation engine. Day cycle runs N strategies through Mantle
DEX flow (Merchant Moe / Agni / Fluxion). Evening cycle runs a Karpathy-style
autoresearch loop: `xianvec-intern` reads each strategy's `program.md` plus
the day's trade ledger, proposes a mutation, paper-tests it on the day's data
plus a held-out window, commits the new variant only if it improves. Variants
that diverge meaningfully fork into new lineages — producing a public genealogy.

Reuses:
- `xianvec-eval::ab_compare` for the day cycle.
- `xianvec-intern` for the evening propose-mutate-evaluate loop.
- The existing baseline + queued strategy library as seed population.
- **TraderArm (vectors-on) as one strategy in that population.** The
  control-vector experiment competes against everything else on equal
  terms, with on-chain receipts. The personal project lives *inside* the
  hackathon submission, not next to it.

### Surface layer — ERC-8004 Strategy Marketplace

Each strategy variant mints an ERC-8004 NFT (Identity Registry) via
`xianvec-identity`. End-of-day performance receipts are written to the
Reputation Registry, signed by the operator. Held-out backtest results
are written to the Validation Registry as signed-oracle receipts —
TEE / zkML attestation deferred to v2. A separate Next.js dashboard reads
the registries and renders: live ladder, per-lineage genealogy tree,
one-click delegate with Conservative / Balanced / Aggressive risk presets
(drawn from `xianvec-risk`).

The marketplace narrative is **structural** — the system is *capable* of
hosting external strategies, demonstrated by 5–10 internal strategies
competing in the live hackathon window. No external participants required
for the demo.

### Feature-flag boundary

Today, `xianvec-identity` is opt-in via workspace `default-members` because
its alloy v2 stack is heavy. Pre-merge-back, the same pattern is lifted to
a named cargo feature `control-vectors`, gating `xianvec-introspect` and
`xianvec-inference`'s steering paths. Hackathon build = `cargo build` (skips
CV crates, light + reproducible for judges and non-technical users). Personal
build = `cargo build --workspace --features control-vectors` (full TraderArm
with vectors-on inference path). CI runs both. The `Strategy` trait stays
the same; only the inference backend differs.

### Branch / sequencing

- **Now:** Cut `hackathon/turing` off `main`. Hackathon-specific work lands
  there — ERC-8004 testnet/mainnet wiring, dashboard repo, marketplace
  surface, loom evening cycle, signed-oracle validation.
- **During sprint:** `main` continues to evolve the CV experiment (F27 Python
  eval, F28 llama.cpp `--control-vector` spike, ADR 0009 runtime resolution).
  Cherry-pick non-CV improvements bidirectionally — new strategies, new MCP
  tools, eval harness fixes.
- **Post-submission (after Jun 15):** Merge `hackathon/turing` → `main` with
  the `control-vectors` feature gate in place. Both modes coexist permanently.

## Why this and not a control-vector-headline submission

Restated for the record:

1. Control vectors require GPU access for live demo. Signed dashboards do not.
2. ERC-8004's three registries are designed for reputation-bearing units.
   A strategy is one. A vector is not.
3. Cross-track positioning (AI Trading & Strategy + Agentic Wallets & Economy
   + Consumer & Viral DApps) is achievable with Marketplace framing. Pure
   CV submission lives in one track only.
4. The repo is already scaffolded for this pivot — `ab_compare`, intern,
   identity, MCP. Less *new* code than a CV-focused submission would need
   to add a comparable consumer surface.
5. The control-vector thesis is preserved as TraderArm-with-vectors competing
   inside the loom. If it wins the ladder, that's the headline. If it doesn't,
   the loom prunes it like any other variant — which is honest and
   demonstrates the system works.

## Consequences

- `hackathon/turing` becomes the active sprint surface. `main` continues
  the personal CV track.
- The `control-vectors` cargo feature is a one-time refactor (~0.5–1 day);
  deferred until pre-merge.
- F27 / F28 / ADR 0009 (Qwen3-Next cvec spike + runtime question) remain
  on `main` — not blocked by, and not blocking, the hackathon.
- **F14 onchain strategies become hackathon-critical** (Nansen-flow,
  funding-rate fader, stablecoin exchange-inflow, liquidation-cascade
  fader). They're the seed population of the loom and they're what makes
  the marketplace credibly Mantle-native. Onchain data sourcing must land
  week 1.
- **ADR 0008 ops runbook executes week 1** — Mantle Sepolia deployment
  was deferred to Phase 11.5; pulled forward.
- `xianvec-identity` needs non-trivial wiring to mint per-strategy NFTs
  and post per-cycle reputation receipts. This is the riskiest seam and
  gets end-to-end smoke first.
- TraderArm-vectors-off (the existing falsification control) becomes a
  marketplace-level strategy with its own NFT, not just an A/B arm.
  TraderArm-vectors-on requires `--features control-vectors` to run; the
  feature is opt-in for judges.
- Demo content: live ladder + genealogy tree + one-click delegate, NOT
  a steering-vector animation.

## References

- ADR 0001 — Inference backend (CV stack still lives here, will be gated)
- ADR 0008 — ERC-8004 registry deployment ops runbook
- ADR 0009 — Qwen3-Next runtime options (parallel CV track)
- FOLLOWUPS F3 (TraderArm), F14 (onchain strategies), F15 (TA strategies),
  F21 (intern), F22 (mcp), F27/F28 (cvec spike)
- `architecture.md` §7.1 (control-vector thesis)
- `vector-strategies.md` (LatentWill notes — vector × strategy pairings)
- `LatentWill/Xianvec/pivot1-strategyloom.md` (long-form rationale)
- Turing Test Hackathon 2026 announcement (Chainwire, 2026-04-23)
- ERC-8004 EIP (eips.ethereum.org/EIPS/eip-8004; mainnet live 2026-01-29)
- Karpathy autoresearch (github.com/karpathy/autoresearch, March 2026)
