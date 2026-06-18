# Optimizer Improvements: Self-Play Personas + Real DSPy Evaluation

Grounded on the AutoResearch self-play paper (Chen 2026, §8.6 median, 16 review rounds, 285B RL validation) and the current xvision optimizer architecture.

---

## Phase 1: Persona-Differentiated Tournament Judges

**Effort:** Low | **Impact:** High | **Risk:** Minimal (additive, no existing behavior changes)

### Current state
The tournament dispatches `JUDGE_COUNT = 3` parallel judges via `borda_vote`, all using one prompt (`tournament-judge-v1.md`). They're three samples from the same distribution — diversity comes only from sampling noise.

### What changes
Split the three judge calls into distinct personas sharing a base prompt plus a persona-specific directive:

| Index | Persona | Directive |
|---|---|---|
| J0 | **Numeric** | Did backtest numbers improve? Was the improvement worth the added complexity? |
| J1 | **Consistency** | Is the rationale coherent with the diff? Are parameter ranges sane? Filter changes logically justified? |
| J2 | **Risk/Safety** | Does this increase drawdown, position count, or leverage? Any overfitting smell to a single regime? |

Each judge still returns a Borda ranking — but through their biased lens. The winner is still determined by Borda tally. The signal gain: when judges disagree, we know *why* (the risk judge flagged something the numeric judge didn't care about).

### Files changed
- `tournament.rs`: `borda_vote` dispatches 3 persona prompts instead of 1 repeated
- New prompt files: `prompts/autooptimizer/tournament-judge-numeric-v1.md`, `...-consistency-v1.md`, `...-risk-v1.md`
- `TournamentResult`: add `per_judge_rankings: [BordaVote; 3]` with persona labels

### Paper mapping
Maps to R1 (Experimentalist), R3 (Perfectionist), R4 (Synthesizer). The "theorist" persona (R2) maps to Phase 2's per-dimension gates.

---

## Phase 2: Per-Dimension Candidate Gates (The "Binding Constraint")

**Effort:** Medium | **Impact:** High | **Risk:** Medium (adds rejection reasons; backward-compat via config)

### Current state
The gate is purely numeric: `sharpe delta > effective_min_improvement` + `max_drawdown ≤ parent × 1.5`. A candidate can pass the gate while having a nonsensical rationale, parameter explosion, or regime-specific overfitting.

### What changes
After the tournament picks a winner, run a structured dimension check. The candidate must pass **all** gates (binding constraint — weakest dimension determines outcome):

| Dimension | Check | Threshold | Failure action |
|---|---|---|---|
| **Numeric** | sharpe delta > `effective_min_improvement` | Config | Reject (existing) |
| **Drawdown** | max_drawdown ≤ parent × 1.5 | Code | Reject (existing) |
| **Coherence** | Judge rates rationale as coherent | LLM (1-5, ≥ 3 passes) | Reject with finding |
| **Simplicity** | Parameter count change ≤ +N | Config (default 5) | Warn only |
| **Regime** | Performance doesn't degrade on ≥ 2 regimes | ≥ 2 regimes must be net-positive | Reject with finding |
| **Overfitting** | Baseline-untouched sharpe delta ≥ 0 | Must not be negative | Reject with finding |

The `regime` and `overfitting` dimensions use existing data (the scenario pool already computes per-regime and baseline metrics). No new backtests needed — these are already in the cycle result.

### Files changed
- `gate.rs`: new `DimensionGate` struct, `check_all_dimensions()` function
- `cycle.rs`: call dimension check after tournament, before node promotion
- `cycle_export.rs`: include per-dimension pass/fail in cycle document

### Paper mapping
Maps to the theorist (R2) as binding constraint. The median is pinned by the weakest dimension — same principle.

---

## Phase 3: Pre-Cycle Validation Suite

**Effort:** Medium | **Impact:** Medium | **Risk:** Low (read-only checks, no mutation)

### Current state
Scattered checks: `preflight_trader_provider` (cross-provider), `max_consecutive_errors` breaker, the new 0-keep termination guard. Checks run inside the cycle after the mutator has already burned tokens.

### What changes
Add `preflight_cycle()` — runs *before* the mutator fires, catches known failure modes with zero token burn:

```
1. Strategy filter vs scenario window: does filter fire on ≥ 1 bar?
2. Strategy trader has model binding (not legacy unbound trader_slot)
3. Last eval run had ≥ 1 bar triggered (not a silent 0-bar strategy)
4. Scenario window ≥ N bars for decision cadence (e.g., ≥ 20 bars of 1h for a 1d strategy)
5. Provider has pricing catalog if --budget is set (already done, move here)
6. Agent slots all resolve (no dangling agent refs)
7. Strategy has ≥ 1 non-empty prompt (not a prompt-less shell)
```

Each check produces an actionable diagnostic. All must pass before the cycle launches. Failures are surfaced as `PreflightReject` with the paper's "diagnose → fix → retry" pattern.

### Files changed
- New: `preflight_cycle.rs` — `preflight_cycle()` with 7 checks
- `optimize.rs` + `autooptimizer_cycle.rs` (dashboard): call before cycle spawn

### Paper mapping
Maps to the "pre-submission check script" pattern — recurring type errors baked into operating constraints. Catches the 0-kept pattern before ~4.2M token burn.

---

## Phase 4: Real Eval Scoring for GEPA (Not LLM Scoring)

**Effort:** High | **Impact:** High | **Risk:** Medium (adds backtest pipeline to DSPy compile path)

### Current state
`GepaBridge::score()` rates candidate instructions via LLM call — "Rate how well this instruction prefix addresses each observation (0.0–1.0)." This is a proxy. The instruction's actual quality can only be measured by running it through the mutator and evaluating the resulting strategy's backtest.

### What changes

**Architecture:** Replace LLM-based scoring with a two-tier eval scorer:

```
GepaEvalScorer
├── Fast tier (LLM): existing score() for rapid culling
│   Filters out obviously bad instructions (score < 0.3)
│   Same as current, but labeled "fast pass"
└── Real tier (Backtest): for candidates passing fast tier
    ├── Inject instruction into mutator prompt prefix
    ├── Run mutator on a standard benchmark parent strategy
    ├── Run paper-test backtest on a standard benchmark scenario
    ├── Score = sharpe_improvement over parent
    └── Cache results per instruction hash
```

**Scoring function:**
```
real_score = clamp((child_sharpe - parent_sharpe) / max(0.01, |parent_sharpe|), -1.0, 1.0)
```

This maps directly to the paper's key method — the 285B GRPO experiment *tested the claim* rather than having another LLM judge it.

**Benchmark pool:** A small, fixed set of parent strategies + scenarios (pre-seeded in the config) that serve as the GEPA evaluation harness. These never change — they exist only to score DSPy instructions.

**Cost:** Each GEPA compile would run `candidates × generations` real backtests (default: 5 × 2 = 10). At ~$0.02/backtest for a 30-day scenario, that's ~$0.20 per compile — insignificant next to a ~$20 cycle.

**Fallback:** When `gepa_real_eval = false` (default), current LLM-only scoring is used. Operators with budget concerns keep the fast path.

### Files changed
- `gepa.rs`: new `GepaEvalScorer` with `real_score()` path
- New: `gepa_eval.rs` — benchmark parent + scenario pool, backtest runner
- `config.rs`: `gepa_real_eval: bool`, `gepa_benchmark_parent_ids: Vec<String>`

### Paper mapping
Direct map to the 285B GRPO experiment — the paper's central claim was validated with a real experiment, not another LLM's opinion.

---

## Phase 5: DSPy for Tournament Judge Prompt Optimization

**Effort:** Medium | **Impact:** Medium | **Risk:** Low (additive DSPy target)

### Current state
DSPy only optimizes the mutator's instruction prefix (DSR). The tournament judge prompt is static.

### What changes
Add a second DSPy target: the tournament judge prompt. Observations come from judge disagreements with the numeric gate:

```
Observation format:
"JUDGE_MISMATCH: judge ranked candidate #1 (sharpe +0.03, rationale: 'wider ATR period')
 but numeric gate rejected it (sharpe -0.01 vs parent, effective_min=0.005)"
```

When the judge ranks a candidate #1 that the gate rejects, or ranks a candidate #3 that the gate would have accepted, that's a judging error. DSPy compiles these into an improved judge prompt that better aligns with numeric reality.

### Files changed
- `tournament.rs`: emit `JUDGE_MISMATCH` observations as `Finding` entries
- New prompt target: `judge_instruction` alongside `dsr_instruction` in `PatternSnapshot`

### Paper mapping
Maps to the "parallel workflows stalled" lesson — judges need feedback from reality, not just each other's opinions.

---

## Phase 6: Multi-Generational DSPy with Held-Out Validation

**Effort:** Medium | **Impact:** Medium | **Risk:** Low (additive validation step)

### Current state
GEPA runs `generations` iterations of REFLECT → PROPOSE → SCORE, selecting the highest-scoring instruction. There's no held-out validation — the winning instruction might overfit to the observations it was trained on.

### What changes
Split the observation pool 80/20:
- **Train pool (80%):** used for GEPA compilation
- **Held-out pool (20%):** used only for final validation

After compilation, score the winning instruction against the held-out pool. If held-out score < train score by > 20%, the instruction is overfit — fall back to the `LiveDspyBridge` (simple summarizer) and log a warning.

This maps to the paper's held-out KL-endpoint study — "there is a real tradeoff between training-distribution gains and held-out performance."

### Files changed
- `dspy_flywheel.rs`: split observation pool, held-out validation
- `gepa.rs`: accept held-out observations for final validation score

---

## Phase 7: Pattern Memory / Anti-Pattern Recall

**Effort:** Medium | **Impact:** Medium | **Risk:** Low

### Current state
Judge findings are written as observations into the DSPy memory store. But there's no structured anti-pattern database — recurring failure patterns (e.g., "ATR period set to 0") are re-discovered each cycle.

### What changes
Add a persistent anti-pattern registry:

```
AntiPattern {
    pattern_hash: ContentHash,       // hashed canonical form
    description: String,             // "ATR period set to 0 causes div-by-zero"
    occurrence_count: u32,
    last_seen_cycle: String,
    auto_reject: bool,               // escalate to preflight check?
}
```

When a finding repeats across ≥ 3 cycles, it's promoted to `auto_reject = true` and added to the pre-cycle validation suite (Phase 3). The preflight check reads the anti-pattern registry and blocks cycles that would reproduce known failures.

### Files changed
- New: `anti_pattern.rs` — registry CRUD + promotion logic
- `preflight_cycle.rs`: load anti-patterns with `auto_reject = true`
- `dspy_flywheel.rs`: write findings to anti-pattern registry on promotion

### Paper mapping
Maps to the "baked the lesson into operating constraints" pattern — fourth recurrence of the scientific-notation type error triggered a pre-submission check script.

---

## Risk / Dependency Map

```
Phase 1 (persona judges) ──────────────────────┐
Phase 2 (dimension gates) ─────────────────────┤ independent
Phase 3 (preflight suite) ─────────────────────┘
                                                │
Phase 4 (real GEPA scoring) ───────────────────┤ depends on Phase 2
Phase 5 (DSPy for judge prompt) ───────────────┤ depends on Phase 1
Phase 6 (held-out DSPy validation) ────────────┤ depends on Phase 4
Phase 7 (anti-pattern memory) ─────────────────┘ depends on Phase 3
```

---

## Token Cost Estimate

| Phase | New LLM calls per cycle | Est. tokens/cycle |
|---|---|---|
| 1 | +0 (replaces 3 identical calls with 3 persona calls) | +0 |
| 2 | +1 (coherence judge call) | ~2K |
| 3 | +0 (read-only DB checks) | +0 |
| 4 | +10 backtests per DSPy compile (every ~5 cycles) | ~$0.20/compile |
| 5 | +0 (observations piggyback on existing judge call) | +0 |
| 6 | +0 (held-out validation reuses existing observations) | +0 |
| 7 | +0 (read-only DB checks) | +0 |

Net cost increase: negligible. Phase 4 is the only cost adder and it's config-gated (`gepa_real_eval = false` by default).
