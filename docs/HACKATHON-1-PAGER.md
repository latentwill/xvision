# xvision — Non-custodial AI trading agents that improve themselves

**For:** Mantle hackathon judges + sponsors + first-100 users.
**Status:** Draft. The operator iterates this file directly.

---

## The single-sentence pitch

Unlike FTX, Binance, or any custodial trading platform, **xvision never holds
your trading capital — only the authority to trade with it.** You fund your own
Orderly account, xvision signs orders with a scoped key it can't withdraw with,
and every decision is on-chain attestable. An overnight autooptimizer
generates new strategy variants, evaluates them against a held-out judge, and
seals the survivors as immutable lineage NFTs.

## What's running

- **Trading rail:** Orderly Network on Mantle. Non-custodial — your USDC stays
  in your account, xvision holds a trading-only Ed25519 key with explicit scope
  enforcement at the broker layer.
- **AutoOptimizer:** mutates a seed strategy across the configuration manifold
  (briefing format, prompt scaffolding, model selection, risk envelope), runs
  each variant against a backtest harness, gates survivors through an LLM judge,
  and seals only the variants that beat their parent on out-of-sample data.
- **Provenance:** every variant has a lineage NFT (ERC-8004) recording its
  parent, its mutations, and its sealed performance. Reputation is portable
  across platforms.
- **Operator surface:** kill switch, emergency-close, per-agent budget caps,
  audit log of every order's full lifecycle (emit → risk → simulate → sign →
  submit → fill → close).

## What's load-bearing in the demo

1. **Live autooptimizer run** — show one mutator iteration: variant in,
   judge verdict out, lineage NFT minted.
2. **Kill switch** — `xvn kill --all` halts every dispatcher in <1s.
3. **Audit log replay** — recover position state from the audit log alone.
4. **Marketplace browser** — show how a depositor would discover and delegate
   to a sealed agent. (Marketplace contract is Plan 5; demo uses staged data.)

## Why now

Three converging things make this the right week:
- The wallet rail (this plan) lands the trust story.
- ERC-8004 with reputation portability is a real economic primitive that the
  marketplace turns on.
- LLM judge quality has crossed the threshold where automated evaluation of
  trading-strategy edge is more reliable than human review.

## Risks we're explicit about

- **Single-process key custody.** v1 uses an encrypted-at-rest Ed25519 key in
  the operator's process. v2 paths: MPC, smart-account scoping, browser-issued
  ephemeral keys.
- **Cross-margin contagion.** v1 ships either margin-mode isolation (if Orderly
  supports it for our setup) or an aggregate-margin guard that fails closed.
  Not the only safety mechanism.
- **Reputation gaming.** Attestations are gated to operator + judges in v1.
  Open governance is a v2 problem.

## What we're asking from sponsors

(Operator fills in: judge time / Orderly testnet credits / Mantle priority
support / etc.)

## Where to read more

- Architecture: `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`
- Implementation: `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md`
  + amendments
- Research lineage: `docs/superpowers/research/2026-05-10-ideonomy-explorations.md`
- Marketing follow-ups: `docs/marketing-followups.md`
