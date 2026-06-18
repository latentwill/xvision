# Phase 4: Real-Eval GEPA Scoring — Grounded Revision

Grounded on: the GEPA paper (Agrawal et al. 2025, arxiv:2507.19457), DSPy GEPA API docs, and the GEPA candidate selection guide.

## What the Research Says

GEPA outperforms GRPO by **+10% on average (up to +20%)** while using **up to 35× fewer rollouts**. The key innovations beyond what xvision currently implements:

| Technique | What GEPA does | xvision gap |
|---|---|---|
| **Pareto frontier selection** | Samples candidates proportionally to how many frontier keys they're best on — not greedy max-mean | Current: `max_by` picks single best candidate. Collapses to local optima. |
| **Minibatch → full eval** | Scores on 3-5 examples first; only promotes to full eval if minibatch improves | Current: scores all observations every time |
| **Separate reflection model** | `reflection_lm` is a stronger model (GPT-5 at temp=1.0) than the one being optimized | Current: same model for REFLECT, PROPOSE, and SCORE |
| **Text feedback alongside score** | `ScoreWithFeedback = {score: float, feedback: str}` — explains WHY, not just score | Current: only 0-1 scalar scores |
| **Merge (crossover)** | `use_merge=True` combines two successful candidates into a new variant | Current: no crossover operation |
| **Round-robin component selection** | Optimizes one component at a time, cycling through them | Current: optimizes the single mutator prefix |
| **Skip perfect scores** | `skip_perfect_score=True` avoids wasting reflection budget | Current: all observations fed to REFLECT |
| **Checkpointing** | `log_dir` enables resumption from last checkpoint | Current: snapshot DAG provides this |

## Revised Phase 4 Implementation Plan

### 4a: Pareto Frontier Selection (replaces edit-distance diversity)

**Current:** `max_by(|a, b| a.mean.partial_cmp(&b.mean))` — greedy single best.

**Fix:** Track per-observation best scores across all candidates in a generation. Build a frontier mapping: `observation_id → [candidate_indices with best score]`. Sample candidate proportionally to how many observations it's best on.

```rust
// Per-generation frontier tracking
let mut frontier: HashMap<usize, Vec<usize>> = HashMap::new(); // obs_idx → best_candidates
for (obs_idx, scores) in all_candidate_scores.iter().enumerate() {
    let best_score = /* max score for this observation */;
    frontier.insert(obs_idx, candidates_with_best_score);
}
// Sample: candidate appears in N keys → N× selection probability
let selected = weighted_sample_by_frontier_count(&frontier);
```

**Config:** `gepa_selection_strategy: "pareto" | "current_best"` (default `"pareto"`)

### 4b: Minibatch → Full Eval Two-Tier Pipeline

**Current:** All observations scored every generation.

**Fix:** 
1. **Minibatch tier** (cheap): Score on `reflection_minibatch_size` (default 3) observations first. LLM scoring only.
2. **Full tier** (expensive): Only if minibatch mean > current best, promote to full observation pool scoring.
3. **Real tier** (config-gated): Only if `gepa_real_eval = true`, run actual backtest on benchmark pool for candidates that pass full tier.

```
for candidate in candidates:
    mb_score = score_llm(candidate, minibatch_obs)     // cheap
    if mb_score <= best_mb_score: continue               // early cull
    full_score = score_llm(candidate, all_obs)           // verify
    if full_score > best_score:
        if gepa_real_eval:
            real_score = score_real(candidate)            // backtest
        update_best(candidate, full_score)
```

### 4c: Separate Reflection Model

**Current:** `self.dispatch` used for REFLECT, PROPOSE, and SCORE.

**Fix:** Add `reflection_dispatch: Arc<dyn LlmDispatch>` to `GepaBridge`. REFLECT uses `reflection_dispatch` (stronger, higher-temp model). PROPOSE and SCORE use `self.dispatch` (the mutator's model).

**Config:** `gepa_reflection_provider: Option<String>` / `gepa_reflection_model: Option<String>`. Falls back to mutator provider/model.

**Rationale from paper:** "GEPA benefits from a strong reflection model. Consider using `dspy.LM(model='gpt-5', temperature=1.0, max_tokens=32000)` for optimal performance." Higher temperature (1.0 vs 0.5) for more creative pattern recognition in REFLECT.

### 4d: Text Feedback in Scoring (replaces dimensional rubric)

**Current:** SCORE returns `Vec<f64>` — just 0-1 numbers per observation.

**Fix:** SCORE also returns a text feedback string per observation explaining WHY the instruction does or doesn't address it. This enriches the next generation's REFLECT.

```rust
struct ScoreWithFeedback {
    scores: Vec<f64>,
    feedback: Vec<String>,  // one per observation
}
```

The SCORE prompt becomes:
```
For each observation, give a score (0-1) AND one sentence explaining WHY.
Return: {"results": [{"score": 0.8, "why": "Addresses the ATR-overfitting pattern directly"}, ...]}
```

The feedback strings are aggregated into the next generation's REFLECT prompt as "Key insights from scoring: ..."

### 4e: Merge (Crossover) Operation

**Current:** No crossover — candidates evolve independently from the same parent reflection.

**Fix:** After every `merge_frequency` generations (default 3), pick two high-scoring candidates and merge them:

```
merge(candidate_a, candidate_b):
    prompt = "Combine these two instruction prefixes into one that captures both strengths.
              Instruction A: {a}
              Instruction B: {b}
              Output the merged instruction."
    return llm(prompt)
```

Config: `gepa_use_merge: bool` (default true), `gepa_merge_frequency: usize` (default 3)

### 4f: Skip Perfect Scores + Checkpointing

**Skip perfect scores:** In `score()`, skip observations with score = 1.0 from the reflection prompt. They provide no improvement signal. Config: `gepa_skip_perfect: bool` (default true).

**Checkpointing:** Already handled by DSPy snapshot DAG (`PatternSnapshot`). Each compile produces a snapshot with lineage to the prior. No change needed.

### 4g: Benchmark Pool (unchanged from original plan)

`[autooptimizer.gepa_benchmark]` config section:
- `parent_strategy_ids: Vec<String>` — 2-3 diverse parent strategies
- `scenario_ids: Vec<String>` — 2-3 standard scenarios

Pre-seeded, static, used only for `gepa_real_eval = true`.

## Revised Token Cost Estimate

| Step | New calls per compile | Estimate |
|---|---|---|
| 4a (pareto) | +0 | $0 |
| 4b (minibatch) | -calls (early cull) | slight savings |
| 4c (reflection model) | same count, different model | depends on model |
| 4d (text feedback) | +output tokens (why strings) | ~+500 tokens/call |
| 4e (merge) | +`merge_frequency` LLM calls | ~2 extra calls/compile |
| 4g (real eval) | +backtests × candidates_completing_full_tier | ~$0.20/compile |

## Files Changed

- `gepa.rs`: Pareto frontier tracking, minibatch tier, reflection dispatch, text feedback, merge, skip-perfect
- `dspy_bridge.rs`: `CompileResult` gains per-observation feedback strings
- `config.rs`: `gepa_selection_strategy`, `gepa_reflection_provider/model`, `gepa_use_merge`, `gepa_skip_perfect`, benchmark pool
- `optimize.rs` (CLI): `--gepa-selection-strategy`, `--gepa-reflection-provider`, `--gepa-real-eval`
- New prompt: `prompts/autooptimizer/gepa-score-v2.md` (score + why)
- New prompt: `prompts/autooptimizer/gepa-merge-v1.md` (crossover merge)
