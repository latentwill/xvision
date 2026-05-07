# CV Extraction: xianvec → xianvec-play

**Date:** 2026-05-07
**ADR:** `decisions/0011-cv-extraction.md`
**Status:** Design accepted; implementation pending

## Goal

Move xianvec from a CV-driven trading agent to a CV-free multistrategy /
ERC-8004 marketplace project. Control-vector substrate migrates to
xianvec-play with full git history preserved, so the CV research line
continues in its own home. xianvec resumes hackathon work from a clean
baseline.

## In scope

1. Full-history merge of `xianvec/main` into xianvec-play.
2. Removal of all CV concerns from xianvec (code, data, tooling, docs).
3. Reconciliation of xianvec ADRs / FOLLOWUPS / implementation plan.

## Out of scope

- **Cleanup on the xianvec-play side.** xianvec-play inherits the full
  xianvec tree, including trading-domain crates (trader, risk, execution,
  intern, eval) and trading-domain docs. They sit dormant. Pruning them
  is a follow-up project after the hackathon, when xianvec-play's
  research direction is clearer.
- **Strategy Loom / marketplace feature work.** The hackathon-critical
  features (loom engine, evening Karpathy cycle, ERC-8004 marketplace
  surface, dashboard) are not part of this pivot. They resume on `main`
  once the slim-down lands.
- **Renaming xianvec.** The `-vec` suffix is now a historical artifact
  (it referred to control vectors). Renaming is deferred — too much
  collateral churn for a 5-week hackathon window.
- **xianvec-play language / framework decisions.** Whether xianvec-play
  rebuilds in Python, stays Rust, or becomes hybrid is its own design
  conversation, post-merge.

---

## Phase 1 — Copy (full-history merge)

### Mechanics

Run from xianvec-play:

```bash
cd /Users/edkennedy/Code/xianvec-play
git pull                                               # catch up on origin
git remote add xianvec /Users/edkennedy/Code/xianvec
git fetch xianvec
git merge xianvec/main --allow-unrelated-histories \
    -m "Import xianvec @ <SHA> (CV extraction, ADR 0011)"
git remote remove xianvec
git push origin master
```

Source SHA at time of merge: `ae1ffa1` (current head of `xianvec/main`).

### Conflict expectations

None expected. xianvec-play's existing files are:

- `README.md` (perspective-embedding research framing)
- `RESEARCH_LOG.md`
- `research/` (4 markdown files: book-problem, casting-director,
  perspectives-as-skills, soft-prompts)
- `.DS_Store` (untracked)

xianvec has none of these at root. The merge unites the trees without
overlap.

### State after Phase 1

xianvec-play contains:

- Its original 2 commits (Initial + research-links).
- ~100 xianvec commits including the full CV development trail.
- All 14 xianvec crates (CV substrate + trading-domain).
- All xianvec docs (`architecture.md`, decisions/, FOLLOWUPS.md, etc.).
- Its original research framing (`README.md`, `research/`,
  `RESEARCH_LOG.md`) untouched.

xianvec is unmodified. All branches (`main`, `hackathon/turing`,
`phase-0-1`) stay where they are.

### Validation

`cd xianvec-play && cargo build --workspace` should succeed with the
same warnings/output as xianvec. CI parity is the smoke test for the
merge integrity.

---

## Phase 2 — xianvec slim-down

Branch: `pivot/cv-extract` off `main`. Single PR. Merge to `main` after
review. Delete `hackathon/turing` post-merge.

### 2a. Code removed

**Crates:**

- `crates/xianvec-inference/` — candle wrapper + steering hooks + inline
  FAISS load. Entirely CV-specific. Deleted.
- `crates/xianvec-introspect/` — per-layer residual norms, logit lens,
  decision-token logits. Diagnostic surface for CV; no use without
  vectors. Deleted.
- `crates/xianvec-gating/` — entropy gating + alpha schedule for vector
  application. Dead without vectors. Deleted.

**Tools:**

- `tools/extract_vectors/` — Python repeng-based contrast extractor.
  Deleted.

**Data:**

- `data/vectors/` — FAISS `.index` files + manifest sidecars (Conviction
  + 3 extracted-but-inactive axes). Deleted.

**Notebooks:**

- `notebooks/inspect_vector.py` — multi-panel diagnostic plotter.
  Deleted. Other notebooks (eval plotting) survive.

**Identity manifests:**

- `identity/vectors_off.agent.json` — deleted.
- `identity/vectors_on.agent.json` — deleted.
- (Per-strategy manifests will be regenerated under the loom; SLF3.)

**Probes:**

- `probes/m0-byreal/`, `probes/m0-orderly/` — keep (executor probes,
  not vector probes).
- `probes/` CV-specific subdirs if any — delete. (Boundary probes for
  vectors per architecture §7.5; check at implementation time.)

**Cargo workspace:**

`Cargo.toml` `members` and `default-members` arrays drop the three
deleted crate paths.

### 2b. Code modified

**`crates/xianvec-eval/`:**

- `src/baselines/`: drop `VectorConfig{Off, On, Random, Orthogonal}` enum
  and the four-arm TraderArm wiring. `TraderArm` becomes a single
  `Strategy` impl: "LLM-without-steering on neutral briefing → decision."
  This survives as one strategy variant in the loom.
- `src/ab_compare.rs`: vectors-on/off arm construction is replaced with
  generic Strategy A vs Strategy B comparison. The `Δ-Sharpe` metric
  generalizes — now it's "Strategy X minus Strategy Y" rather than
  "vectors on minus vectors off." Internal `ArmConfig` no longer carries
  a `VectorConfig` field.
- `src/baselines/`: existing classical TA + onchain Strategy impls
  unchanged; they were always vector-agnostic.

**`crates/xianvec-cli/`:**

- `src/commands/explain_vectors.rs` — deleted.
- `src/commands/mod.rs` — drop the `explain_vectors` registration.
- `src/main.rs` — drop the `explain-vectors` subcommand.
- `src/commands/show_metrics.rs`, `src/commands/report.rs` — replace
  hardcoded `"vectors_off"` / `"vectors_on"` arm labels with generic
  strategy-name parameters. Test fixtures updated.
- `src/commands/show_decision.rs` — test fixture renamed from
  `"vectors_on"` to a generic strategy name (e.g., `"trader_arm"`).

**`crates/xianvec-identity/`:**

- `src/manifest.rs` — drop `VectorConfigSummary` struct and the
  `vector_config` field on `AgentManifest`. The manifest schema becomes:
  `(agent_id, strategy_name, code_commit, strategy_adapter_type,
  risk_preset)`.
- `src/lib.rs` — drop the `vectors_off.agent.json` /
  `vectors_on.agent.json` documentation references.
- ERC-8004 Validation Registry receipt schema (architecture §6.1)
  drops `active_vector_alphas` and `vector_manifest_hash` fields.
  Trade receipts reference `strategy_id` instead.

**`crates/xianvec-trader/`:**

- The crate currently has no `VectorConfig` references in its src
  directly — vectoring lives in `xianvec-eval/baselines/` and was
  injected via candle hooks in `xianvec-inference`. Trader src changes
  are minor: drop any `xianvec-inference` / `xianvec-introspect` /
  `xianvec-gating` deps from `Cargo.toml`. Stage 2 inference path uses
  the same OpenAI-compatible HTTP backend as Stage 1 by default;
  optional local `candle` path is preserved for the trader crate
  if it has its own (un-CV) candle wrapper.

**Workspace `Cargo.toml`:**

- `members` and `default-members` arrays prune three crates.
- Remove top-level dependency entries for `faiss-rs`, `repeng`-related
  Python deps (none direct), candle steering-hook patches if any.

### 2c. Doc reconciliation

**`architecture.md`:**

- §1 (Thesis) — rewritten. New thesis: "On a fixed set of trading setups,
  a multistrategy population evaluated through a deterministic loom with
  ERC-8004 reputation/validation produces a credible, on-chain auditable
  ranking of strategy variants. Karpathy-style evening autoresearch
  evolves the population." Drops "vectors meaningfully change trading
  behavior" framing.
- §2 (System overview) — diagram regenerated. Stage 2 Trader yellow
  block (vectors active) becomes ordinary blue/orange. Control Vectors
  block deleted. Reference to `architecture-diagram.mermaid` updated.
- §3 (Stage 1 Intern) — survives mostly intact; Intern was always
  vector-free.
- §4 (Stage 2 Trader) — vectors-on / no-thinking / hidden-state hooks /
  candle requirement all dropped. Trader becomes a standard LLM caller
  on the same HTTP backend as Intern. `active_vectors` field gone from
  output schema.
- §5 (Risk Layer) — survives intact.
- §6 (Stage 3 Execution) — survives intact except §6.1 Validation
  Registry receipt schema (CV alpha fields removed; `strategy_id`
  added).
- **§7 (Control vector strategy) — deleted entirely.** This is the
  single largest doc cut: ~9 subsections across ~150 lines.
  `steering-vector-architecture.md` companion doc — deleted.
- §8 (Data pipeline) — survives intact.
- §9 (Eval framework) — Δ-Sharpe primary metric kept; "vectors-on vs
  vectors-off" framing replaced with "Strategy X vs Strategy Y" across
  the loom. §9.3 Baselines section: "experimental controls" subsection
  (vectors-OFF / random / orthogonal) deleted; null + classical +
  onchain baselines survive.
- §10 (Tech stack) — drop `faiss-rs`, candle steering-hook references,
  Python `repeng` toolchain, OTel-introspection cross-references. Trader
  inference reverts to HTTP-first, local-candle-optional.
- §10.1 (Cargo workspace layout) — three crate entries removed.
- §10.2 (Lodestar boundary) — deleted; no longer relevant without CV
  substrate to extract.
- §11 (Out of scope) — most CV-related items removed (they're gone, not
  deferred). Karpathy self-improvement loop reframed (was about CV,
  now about strategy mutation in the loom).
- §12 (Open architectural questions) — CV-related rows removed
  (~10 of 17 entries). Add new row noting ADR 0011 supersedes the
  CV-as-headline framing.
- §13 (References) — drop CV-specific papers (SVF, SEAL, Mitra,
  Steer2Adapt, Conceptors, Glamin, repeng, dialz, faiss-rs).

**`steering-vector-architecture.md`:** deleted.

**`FOLLOWUPS.md`:**

- Track classification table: CVF column removed (track migrates to
  xianvec-play). SLF + Shared survive.
- F3 partial-close: TraderArm survives without VectorConfig; close the
  vectors-on/random/orthogonal arms portion.
- F27, F28, F29, F30, F31, F32 (and any other CVF-numbered items):
  closed in xianvec with cross-reference to xianvec-play.
- SLF3 (per-strategy NFT mint): simplified — was "TraderArm-Off, -On,
  -Random, -Orth = four NFTs"; becomes "TraderArm = one NFT".

**`implementation-plan.md`:**

- Phase 0 (CV spike validation gate): deleted.
- Phase 4 (vector ops, steering hook installation): deleted.
- Phase 8 (probe runner with introspection): deleted.
- Phase 9 (eval): vectors-on/off arms removed, generalized to
  multi-strategy.
- Strategy Loom + ERC-8004 phases (currently scattered in SLF
  follow-ups): consolidated into a fresh Phase A / B / C structure
  centered on loom + marketplace.
- Reference to `implementation-plan-python-archive.md`: file deleted
  (stale Python scaffolding from before the Rust pivot).

**`decisions/0001-inference-backend.md`:** revised. CV motivation
sections removed. `candle` retained as the local-inference option for
the Trader; `llama-cpp-rs` fallback retained. Steering-hook flexibility
no longer the primary justification.

**`decisions/0009-qwen3-next-runtime-options.md`:** moved to
xianvec-play (it was a CV-specific runtime question for the cvec
spike). Removed from xianvec/decisions/.

**`decisions/0010-hackathon-pivot-strategy-loom.md`:** kept as
historical record. Add a header note: "Partially superseded by ADR
0011. Strategy Loom + ERC-8004 framing remain valid; the
`--features control-vectors` gate is obsolete."

**`MANUAL.md`:**

- Remove operator instructions for vector extraction
  (`tools/extract_vectors/`).
- Remove vector-specific config sections.
- Remove FAISS index management instructions.
- Stage 2 Trader operator instructions simplified (no candle GPU
  requirements unless trader local-inference is opted into).

**`v1-build-steps.md`:** revised — drop CV phases.

**`scripts/setup_runpod.sh`:** revised — drop FP16 weights download
(64GB), drop `repeng` install, drop FAISS deps. RunPod path becomes
optional (only needed if trader runs local inference).

**`README.md`:** xianvec doesn't currently have one. Optional add: a
short README pointing at `architecture.md` + ADR 0011 for the new
direction. Out of scope unless trivially cheap to add during the
slim-down PR.

### 2d. Branch strategy

- Slim-down branch: `pivot/cv-extract` off current `main` (`ae1ffa1`).
- Merge to `main` via PR after review.
- `hackathon/turing`: deleted post-merge. Premise (CV stays under
  feature flag) is obsolete; `main` is now the hackathon submission
  surface.
- `phase-0-1`: left alone. Predates this work; no clear reason to
  touch.

### Validation

After Phase 2 lands on `main`:

- `cargo build --workspace` succeeds with no CV crates referenced.
- `cargo test --workspace` passes (CV-specific tests are gone with
  their crates; remaining tests should not have referenced CV).
- `xvn --help` no longer lists `explain-vectors`.
- `architecture.md` grep for "vector", "control vector", "steering",
  "FAISS", "repeng" returns no substantive matches outside ADR 0011's
  references and ADR 0010's preserved text.
- ERC-8004 stub tests still pass (manifest schema changes propagated).

---

## Result

### xianvec post-pivot

A Rust workspace for multistrategy trading evaluation + ERC-8004
marketplace. 11 crates (was 14). No CV code, no Python extraction
tooling, no FAISS substrate, no candle steering hooks, no
introspection. Trader survives as one LLM-driven strategy adapter in
the loom, alongside classical TA and onchain strategies. Hackathon
work resumes from a clean baseline.

### xianvec-play post-merge

Full xianvec git history preserved. CV substrate intact (`-inference`,
`-introspect`, `-gating`, `tools/extract_vectors/`, `data/vectors/`,
`notebooks/inspect_vector.py`, all ADRs and architecture text
referencing CV). Trading-domain crates dormant. xianvec-play's
existing perspective-embedding research framing
(`README.md`, `research/`) untouched. Future direction (Python rebuild,
hybrid, Rust-stay) is its own decision, post-pivot.

---

## Future re-integration (sketch)

If a CV-driven trading agent is later built in xianvec-play, it can
re-enter xianvec as a Strategy. Three plausible integration shapes,
not committed by this spec:

1. **Crate path-dep** — xianvec adds `xianvec-play-cv-trader = { path =
   "../xianvec-play/crates/..." }`. Pulls CV code back into xianvec's
   build closure. Cleanest from a Rust-types perspective; defeats some
   of the slim-down's narrative cleanliness.
2. **MCP / HTTP boundary** — xianvec-play exposes the CV-trader as an
   MCP server or HTTP endpoint; xianvec calls it as a remote strategy.
   Preserves slim. Adds latency + ops surface.
3. **Compiled artifact** — xianvec-play produces a binary or library
   artifact; xianvec consumes it as a side-loaded module. Heaviest
   ops, weakest type-safety, but allows independent versioning.

The `Strategy` trait in `xianvec-eval` is the integration surface for
all three shapes. It is treated as a stable public API post-pivot.
Changes to it require a doc note that flags the cross-repo impact.

---

## Risks / open questions

- **Doc rewrite scope is large.** `architecture.md` is ~786 lines;
  removing §7 + revising §1, §4, §6.1, §9, §10, §11, §12, §13 is a
  significant edit. Splitting the slim-down PR into "code cuts" +
  "doc reconciliation" is a candidate for the implementation plan.
- **Lurking CV dependencies.** Some trader / eval / cli code may have
  CV-specific assumptions not surfaced by grep
  (`VectorConfig` / `vectors_on` / etc.). Implementation plan should
  include a "scan for CV-shaped types and labels" pass.
- **Identity manifest schema migration.** Changing
  `AgentManifest` is a breaking change for any persisted manifests.
  Confirm at implementation time whether any production manifests
  exist (likely no, given testnet-only); if any, document the
  migration.
- **Karpathy autoresearch reframing.** ADR 0010's evening cycle was
  framed around mutating `program.md` per strategy. With CV gone,
  some of that scope (e.g., proposing new `VectorConfig` magnitudes)
  evaporates. Implementation plan should walk the autoresearch loop
  and confirm it remains coherent without CV-tunable knobs.

---

*Document: `docs/superpowers/specs/2026-05-07-cv-extraction-design.md`*
*Authority: ADR 0011 (`decisions/0011-cv-extraction.md`)*
