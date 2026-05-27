# Spec amendment handoff — autoresearcher operator vocabulary

> For: spec maintainer (anyone — this is doc work)
> Date: 2026-05-27
> Source of truth: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`

## TL;DR

Add one new §0.1 "Operator vocabulary" section to the karpathy
autoresearcher spec that points at the terminology lock, then add a
footnote (or parenthetical) at the first mention of every
renamed term in §3–§9 of the spec so future readers know which
operator-facing name they'll see on screen. Same data, same
architecture; just a vocabulary cross-reference.

This is the lowest-risk patch in the rollout — single file, doc-only,
no code, no tests to update.

## Files in scope

- `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`
  (the only file you touch)

## Files NOT in scope

- Any AR-1/AR-2/AR-3 plan file (those reference the spec; they don't
  need their own footnotes — the spec is the canonical source)
- `docs/superpowers/specs/2026-05-24-cortex-memory-cline-dspy-flywheels.md`
  (covered by the operator-facing names section in CLAUDE.md; only
  needs a one-line pointer if any audit reveals jargon — separate
  ticket)
- The terminology lock itself (already canonical, don't duplicate)

## The amendment to insert

Place this section between the existing §1 and §2 (or wherever the
spec keeps its preamble — adjust numbering as needed):

```markdown
## §0.1 Operator vocabulary

Every named concept in this spec has two valid names: the
developer-surface name (used here, in Rust types, SQLite columns, and
API fields) and the operator-surface name (used in CLI flags and help
text, UI labels, SSE event display names, MANUAL.md, and the
dashboard wiki). The complete mapping is locked at
`docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`.

Throughout the rest of this spec, the developer-surface name is the
primary term. The first appearance of a renamed concept is followed
by a parenthetical noting the operator-surface name, e.g. "Mutator
(operator-facing: Experiment writer)." Subsequent mentions use the
developer-surface name only.

Cryptographic primitives (BLAKE3, Ed25519, Merkle, canonical JSON)
must never appear on an operator surface. Any reference to them in
this spec is developer-surface only by definition.
```

## The footnote insertions

For each renamed concept, find its first appearance in the spec and
add the parenthetical. Subsequent appearances are not annotated (one
parenthetical per concept). The complete list, taken from the
terminology lock:

| First-appearance term (search the spec) | Parenthetical to add |
|---|---|
| Mutator | (operator-facing: Experiment writer) |
| Mutation, MutationDiff | (operator-facing: Experiment) |
| LineageStatus::Ghost / "Ghost branches" | (operator-facing: Rejected) |
| LineageStatus::Quarantined / "quarantined" | (operator-facing: Suspect) |
| Mutator-skill ladder | (operator-facing: Proposer scoreboard) |
| Evening cycle | (operator-facing: Evening run) |
| `bundle_hash` / ContentHash | (operator-facing: Strategy fingerprint) |
| Merkle root | (operator-facing: Cycle proof) |
| CycleSeal | (operator-facing: Evening summary) |
| SessionCommitment | (operator-facing: Session ground rules) |
| Operator signature | (operator-facing: Operator sign-off) |
| Gate, numeric gate | (operator-facing: Gate — verb unchanged; verdict values "passed"/"failed" render as "Kept"/"Dropped") |
| epsilon, ε | (operator-facing: Minimum improvement) |
| Holdout window | (operator-facing: Untouched test period) |
| Null-result canary / "canary" | (operator-facing: Honesty check) |
| Inversion-pair eval | (operator-facing: Reverse-mutation check) |
| Diversity-decay rate | (operator-facing: Variety score) |
| Counterfactual-chain Merkle root | (operator-facing: Lineage proof) |
| Tier (in `tier='observation'` / `tier='pattern'`) | (operator-facing: Kind) |
| promotion_state | (operator-facing: Status) |
| Demote / demoting | (operator-facing: Retire) |
| Promote / promoting | (operator-facing: Activate) |
| Memory mode `Off` / `Global` / `AgentScoped` | (operator-facing: "Off" / "Shared across all agents" / "This agent only") |
| Optimization (the run) | (operator-facing: Training run) |
| `target_agent_id` | (operator-facing: Parent agent) |
| Demos / "demo source" | (operator-facing: Examples / Example source) |
| Priors / "prior patterns" | (operator-facing: Background patterns) |
| Train / Dev / Holdout (the three splits) | (operator-facing: Training / Validation / Untouched test) |
| Operator attestation | (operator-facing: Operator sign-off) |

## Acceptance criteria

1. §0.1 exists, contains the cross-reference paragraph, and points at
   the terminology lock by path.
2. Every term in the table above has its first-mention parenthetical
   added exactly once. Search the spec for the term; insert the
   parenthetical inline after the first occurrence; do not add it to
   subsequent occurrences.
3. The cryptographic-primitives sentence ("BLAKE3, Ed25519, Merkle,
   canonical JSON … must never appear on an operator surface") is
   present in §0.1.
4. No content moved or renumbered beyond inserting §0.1.
5. Read the spec end-to-end after the patch: it should still flow as
   a technical design doc — the parentheticals shouldn't read as
   intrusive editorial markup.

## Things to push back on

- If a term has a first appearance inside a code block (e.g.,
  `LineageStatus::Quarantined` inside a Rust snippet), don't insert
  the parenthetical in the code — insert it in the prose paragraph
  immediately above or below the code block.
- If two terms appear together in the same sentence (e.g., "the
  Mutator proposes Mutations…"), use one combined parenthetical:
  "(operator-facing: the Experiment writer proposes Experiments)."
- If a term genuinely never appears in prose (only in code), skip
  it — the developer reading the spec is already in the code
  surface.

## Reference

- Terminology lock: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
- Audit context: `docs/superpowers/notes/2026-05-27-autoresearcher-plain-language-audit.md`
- Project-wide terminology note: `/CLAUDE.md` §Terminology → "Operator-facing names (autoresearcher subsurface)"
