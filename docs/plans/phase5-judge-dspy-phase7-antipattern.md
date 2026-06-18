# Phases 5 & 7: DSPy Judge Optimization + Anti-Pattern Memory

Grounded on the AutoResearch self-play paper (Chen 2026) and the GEPA reflection pattern.
Phase 5 depends on Phase 1 (persona judges already shipped).
Phase 7 depends on Phase 3 (preflight suite already shipped).

---

## Phase 5: DSPy for Tournament Judge Prompt Optimization

**Effort:** Medium | **Impact:** Medium | **Risk:** Low (observation emission only)

### Current state

The tournament dispatches 3 persona-differentiated judges (Phase 1). Each returns a `BordaVote`.
The gate verdict (numeric sharpe delta + drawdown comparison) determines whether the
winning candidate is actually promoted. Judges and gate operate independently — when a
judge's #1 pick disagrees with the numeric gate, that disagreement is lost.

### What changes

**5a. Emit JUDGE_MISMATCH findings after gate evaluation**

In `process_parent_mutations`, after `gate_and_classify` returns a `MutationOutcome`,
compare each persona's #1 pick against the numeric gate:

```
if tournament_used:
    for (persona, vote) in tournament_result.per_persona_votes:
        judge_top_pick = vote.ranking[0]
        gate_passed = (outcome.status == LineageStatus::Active)
        if (judge_top_pick == winner_idx) != gate_passed:
            findings.push(Finding {
                code: "JUDGE_MISMATCH",
                summary: "{persona} ranked candidate {judge_top_pick} #1
                          but numeric gate {passed/rejected} candidate {winner_idx}
                          (Δsharpe={delta}, effective_min={min_improvement})",
            })
```

This requires threading `tournament_result` (or at least `per_persona_votes`)
through to the gate evaluation point. Currently `diff_result` only returns
`MutationDiff`, not the full `TournamentResult`.

**5b. Add judge_instruction DSPy target alongside dsr_instruction**

Currently only one DSPy target: `dsr_instruction` (mutator prompt prefix).
Add a second target: `judge_instruction` — the judge prompt DSPy compiles from
JUDGE_MISMATCH observations.

When `handle_cycle_dspy` sees `JUDGE_MISMATCH` findings:
1. Write to `JUDGE_MEMORY_NS` as observation (already happens)
2. Trigger compile for `judge_instruction` namespace (new)
3. Persist compiled prompt as a `Pattern` with `kind = "judge_instruction"`
4. On future cycles, prepend `judge_instruction` to the tournament judge system prompt

### Files changed

- `cycle.rs`: thread `per_persona_votes` from tournament to gate evaluation;
  emit JUDGE_MISMATCH findings
- `dspy_flywheel.rs`: accept `judge_instruction` namespace alongside `dsr_instruction`;
  handle_cycle_dspy routes to both
- `tournament.rs`: expose per_persona_votes on TournamentResult (already done in Phase 1)
- `mutator.rs` / `cycle.rs`: prepend `judge_instruction` DSR to tournament judge prompt

### Paper mapping

Maps to the "parallel workflows stalled" lesson — judges need feedback from reality
(the numeric gate), not just each other's opinions. The DSPy-compiled judge prompt
will learn to weigh numeric reality more heavily.

---

## Phase 7: Anti-Pattern Memory

**Effort:** Medium | **Impact:** Medium | **Risk:** Low (read-only preflight addition)

### Current state

Findings are written to the DSPy memory store (`JUDGE_MEMORY_NS`) and the GEPA flywheel
compiles them into improved mutator prompts. But there's no structured anti-pattern
database — recurring failure patterns ("ATR period set to 0", "filter has 0 conditions",
"parameter explosion > 8 items") are re-discovered each cycle.

The preflight suite (Phase 3) catches structural issues (agent slots, empty filters)
but only the issues explicitly coded. It can't learn new patterns from cycle experience.

### What changes

**7a. Anti-pattern registry module**

New `anti_pattern.rs` in `crates/xvision-engine/src/autooptimizer/`:

```rust
pub struct AntiPattern {
    /// Content-hash of the finding (code + summary, canonicalized).
    pub pattern_hash: String,
    /// Human-readable description from the first occurrence.
    pub description: String,
    /// Finding code (e.g. "SIMPLICITY", "REGIME_DEGRADED").
    pub code: String,
    /// How many cycles have produced this finding (regardless of strategy).
    pub occurrence_count: u64,
    /// The finding's hash when last seen (for dedup within a cycle).
    pub last_content_hash: String,
    /// When this was first observed.
    pub first_seen: chrono::DateTime<chrono::Utc>,
    /// Whether this has been promoted to preflight blockade.
    pub auto_reject: bool,
    /// Human-readable remediation guidance.
    pub remediation: String,
}
```

**7b. Anti-pattern database table**

New SQLite table `autooptimizer_anti_patterns`:
```sql
CREATE TABLE autooptimizer_anti_patterns (
    pattern_hash TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    code TEXT NOT NULL,
    occurrence_count INTEGER NOT NULL DEFAULT 1,
    last_content_hash TEXT NOT NULL,
    first_seen TEXT NOT NULL,
    auto_reject INTEGER NOT NULL DEFAULT 0,
    remediation TEXT NOT NULL DEFAULT ''
);
```

**7c. Promotion logic**

In `handle_cycle_dspy` (or a new `handle_cycle_anti_patterns` function),
after findings are emitted:

```
for each finding in cycle findings:
    hash = content_hash(code + summary)
    if already exists in DB:
        increment occurrence_count
        if occurrence_count >= 3 AND NOT auto_reject:
            promote to auto_reject = true
            remediation = "This pattern has recurred across 3 cycles. \
                           Add a preflight check or adjust the mutator prompt."
    else:
        insert new anti-pattern with occurrence_count = 1
```

**7d. Preflight integration**

In `preflight_cycle.rs`, add a 4th check:

```
check_anti_patterns(pool, strategy_id):
    patterns = query all anti_patterns WHERE auto_reject = true
    for each pattern:
        check if this strategy's structure/params match the pattern
        if match:
            return PreflightReject {
                message: "anti-pattern '{pattern.code}' ({pattern.description}) \
                         has recurred {pattern.occurrence_count} times. \
                         Remediation: {pattern.remediation}"
            }
```

For v1, the matching is simple: if the finding code matches a known dimension gate
failure (e.g., "SIMPLICITY") and the strategy has > 8 params, block it. Future versions
can add structural pattern matching (e.g., "ATR period = 0 → block any strategy
with ATR multiplier ≤ 0.1").

**7e. Migration**

New migration `058_autooptimizer_anti_patterns.sql`.

### Files changed

- New: `anti_pattern.rs` — struct, CRUD, promotion logic
- New: `migrations/058_autooptimizer_anti_patterns.sql`
- `preflight_cycle.rs`: add `check_anti_patterns` (4th check)
- `cycle.rs`: call `handle_cycle_anti_patterns` alongside `handle_cycle_dspy`
- `mod.rs`: register `pub mod anti_pattern;`

### Paper mapping

Direct map to the "baked the lesson into operating constraints" pattern (Chen 2026, §V16):
the fourth recurrence of the scientific-notation type error triggered a pre-submission
check script. Same pattern here — 3rd recurrence promotes to auto-reject blockade.

---

## Implementation Order

```
Phase 5a: Emit JUDGE_MISMATCH findings              ← thread per_persona_votes
Phase 5b: Add judge_instruction DSPy target          ← depends on 5a
Phase 7a: Anti-pattern registry module + DB table    ← independent
Phase 7b: Promotion logic                           ← independent
Phase 7c: Preflight integration                     ← depends on 7a+7b
```

## Token Cost Estimate

| Step | New LLM calls per cycle | Estimate |
|---|---|---|
| 5a | +0 (findings piggyback on existing judge memory) | $0 |
| 5b | +1 DSPy compile per ~5 cycles (same cost as dsr_instruction) | ~$0.10/5 cycles |
| 7a-c | +0 (read-only DB queries, no LLM calls) | $0 |

Net cost: negligible. Phase 5b adds a second DSPy compile target at the same
compilation frequency as the existing `dsr_instruction`.

## Files Changed Summary

| File | Change |
|---|---|
| `cycle.rs` | Thread `per_persona_votes` → emit JUDGE_MISMATCH; call anti-pattern handler |
| `dspy_flywheel.rs` | Add `judge_instruction` target alongside `dsr_instruction` |
| `anti_pattern.rs` | New: struct, CRUD, promotion logic |
| `058_*.sql` | New migration: anti-patterns table |
| `preflight_cycle.rs` | New `check_anti_patterns` (4th check) |
| `mod.rs` | Register `anti_pattern` module |
