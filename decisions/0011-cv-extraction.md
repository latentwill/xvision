# ADR 0011 — CV Extraction: xianvec → xianvec-play

**Date:** 2026-05-07
**Status:** Accepted
**Supersedes (in part):** ADR 0010
**Spec:** `docs/superpowers/specs/2026-05-07-cv-extraction-design.md`

## Context

ADR 0010 (2026-05-05) accepted a hackathon pivot — Strategy Loom + ERC-8004
Marketplace — and gated control-vector code in xianvec behind a planned
`--features control-vectors` cargo feature so it could ship in the same repo.

Two days later (2026-05-07) the boundary is being moved. Control-vector
research is going home: a sibling repo, **xianvec-play**, was already
scaffolded around perspective-embedding research (casting director eval,
Buddhist-pattern framing, soft-prompt foundations). It is the natural host
for the CV substrate. xianvec stays focused on the multistrategy /
marketplace shape ADR 0010 already endorsed, but without the feature gate —
CV is gone from xianvec entirely.

What this changes from ADR 0010:

- The `--features control-vectors` cargo gate is no longer needed; CV does
  not exist in xianvec to gate.
- TraderArm's `VectorConfig{Off, On, Random, Orthogonal}` arms collapse to
  a single LLM-without-steering trader. SLF3's "four NFTs per
  TraderArm-config" decision collapses to one.
- xianvec-play picks up the full xianvec history (`git merge
  --allow-unrelated-histories`) so the CV development trail (~100 commits
  including the FP8 / gradient flow / VRAM debugging trail) stays intact
  in its new home.

## Decision

**Extract all CV concerns from xianvec into xianvec-play.** xianvec-play
inherits xianvec's full code + history; xianvec slims down; the two
projects evolve independently.

### Two-phase execution

**Phase 1 — Copy.** Full-history merge of `xianvec/main` into xianvec-play.
xianvec-play's existing two commits + research docs survive. xianvec is
unmodified.

**Phase 2 — Slim xianvec.** Remove CV crates (`xianvec-inference`,
`xianvec-introspect`, `xianvec-gating`), Python extraction tooling, FAISS
substrate, vector notebooks. Modify `xianvec-eval` (drop `VectorConfig` +
the four-arm TraderArm), `xianvec-cli` (drop `explain-vectors` + rename
hardcoded `vectors_on/vectors_off` labels), `xianvec-identity` (drop
`VectorConfigSummary` + per-config manifests). Reconcile `architecture.md`
(§7 deleted), `FOLLOWUPS.md` (CVF queue closed in xianvec, opened in
xianvec-play), `implementation-plan.md` (CV phases dropped), ADRs, and
operator docs.

The spec at `docs/superpowers/specs/2026-05-07-cv-extraction-design.md`
enumerates removals, modifications, and doc reconciliation in detail.

### Branch strategy

Slim-down lands on `pivot/cv-extract`, merges to `main`. `hackathon/turing`
is deleted post-merge — its premise (CV stays in xianvec under feature flag)
is obsolete and `main` becomes the hackathon submission surface.
`phase-0-1` is left alone.

### xianvec-play side

No simultaneous slim-down. xianvec-play accepts the full xianvec tree —
trading-domain crates dormant, CV substrate live. Future cleanup
(removing trading crates from xianvec-play) is its own follow-up after
the hackathon.

## Why this and not the ADR 0010 plan

1. **Cleaner boundary.** Cargo feature flags add maintenance burden every
   commit (dual-build CI, `cfg(feature=...)` proliferation, doc
   conditionals). A fork eliminates it.
2. **Research home.** xianvec-play's framing — perspective embeddings,
   casting director, Buddhist patterns — is the right narrative container
   for CV work. Forcing CV under "trading agent" was always a mismatch.
3. **Reduced cognitive load on hackathon.** The submission is now a
   single-purpose codebase (multistrategy + marketplace). Judges,
   collaborators, and operator scripts all see one thing.
4. **Reversible.** History is preserved on both sides. If a CV-trading
   agent is later built in xianvec-play, it can re-enter xianvec as a
   `Strategy` (importing via crate path-dep, MCP/HTTP, or a compiled
   artifact). The `Strategy` trait is the integration boundary; it
   survives the slim unchanged.

## Consequences

- xianvec's tech stack simplifies: no candle hidden-state hooks, no
  `faiss-rs`, no `repeng` toolchain, no Python subprocess for vector
  extraction. Stage 2 Trader becomes a vanilla LLM caller against
  the same OpenAI-compatible HTTP backend as Stage 1 Intern.
- ADR 0010 is preserved as historical record and partly superseded:
  feature-flag plan is dead; Strategy Loom + Marketplace + Karpathy
  evening cycle survive intact.
- ADR 0009 (Qwen3-Next runtime options for cvec spike) moves to
  xianvec-play. ADR 0001 (inference backend) revised to drop CV
  motivation while keeping `candle` as the trader's local-inference
  option.
- FOLLOWUPS: F3 partial-close (TraderArm survives without
  `VectorConfig`); F27/F28/F29/F30/F31/F32 (CVF queue) closed in
  xianvec, re-opened in xianvec-play if applicable. SLF3 simplified
  (one TraderArm NFT, not four).
- The `Strategy` trait is now load-bearing as the integration boundary
  for any future re-import from xianvec-play. Treated as a stable
  public API post-pivot.
- Hackathon work resumes on `main` from a clean baseline once the
  pivot lands.

## References

- Spec: `docs/superpowers/specs/2026-05-07-cv-extraction-design.md`
- ADR 0010 (predecessor pivot — partially superseded)
- ADR 0001 (inference backend; revised by this ADR)
- ADR 0008 (ERC-8004 deployment; unchanged)
- ADR 0009 (moves to xianvec-play)
- FOLLOWUPS — F3, F27, F28, F29, F30, F31, F32, SLF3
- `architecture.md` §7 (control-vector strategy; deleted by this ADR)
