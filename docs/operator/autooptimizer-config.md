# Optimizer config (`autooptimizer.toml`)

> Derived from `AutoOptimizerConfig`
> (`crates/xvision-engine/src/autooptimizer/config.rs`) **as of 2026-06-11**.
> Field names, types, and defaults below are the serde contract of that struct.

The Optimizer cycle (`xvn optimize`, default action) reads a single TOML file
that configures the evaluation windows, the experiment writer (mutator), the
gate objective, and optional regime / scenario pools.

## File location & `--config`

- Default path: `$XVN_HOME/autooptimizer.toml` (honors `$XVN_HOME`, falling
  back to `~/.xvn/autooptimizer.toml`).
- `--config <path>` **REPLACES** the default file entirely — it is **not merged**
  with `$XVN_HOME/autooptimizer.toml`. When you pass `--config`, the default
  file is ignored and only the file you point at is loaded.
- A malformed config now surfaces a **field-level parse error** (U1/U15 fixed):
  the error message embeds the offending TOML field path and line/column
  directly (e.g. `parsing autooptimizer config at <path>: invalid type … for
  key 'min_improvement' at line 2`), not just a generic "could not parse"
  wrapper. Fix the named field and re-run.

If the file is absent, the cycle runs with the struct defaults documented below.

## Top-level keys

| Key | Type | Default | Meaning |
|---|---|---|---|
| `min_improvement` | float | `0.05` | Gate epsilon. The candidate must beat its parent by at least this much on the objective to be Kept. Must be `> 0`. Operator-surface name for `--gate-epsilon`/`--min-delta`. |
| `day_window` | table `{ start, end }` (ISO `YYYY-MM-DD`) | `2025-01-01` → `2025-04-01` | The training / candidate-evaluation date window. `start` must be before `end`; span capped at **120 days** (see [Window span cap](#window-span-cap)). |
| `baseline_untouched_window` | table `{ start, end }` (ISO `YYYY-MM-DD`) | `2025-04-01` → `2025-05-01` | The held-out comparison window (operator-surface: "untouched test period"). `start` before `end`; span capped at 120 days. Should be disjoint from / after `day_window`. |
| `mutator` | table | see [`[mutator]`](#mutator-table) | The experiment writer's provider/model/retries. Required. |
| `allowed_mutation_kinds` | array of string | `["prose", "param", "tool", "filter"]` | Which experiment kinds the writer may propose. `"filter"` is enabled by default (Phase 2). |
| `experiments_per_cycle` | u32 | `5` | Candidate experiments generated per parent per cycle (`CycleConfig.mutations_per_parent`). Validated to `1..=64`. **Also a CLI flag** — `--experiments-per-cycle` overrides this per run. |
| `loosening_schedule` | optional table `{ day_n_thresholds: [float] }` | absent (`None`) | Optional progressive-loosening thresholds. Each threshold must be `> 0`. |
| `lineage_root` | optional path | absent (`None`) | Override for where lineage artifacts are rooted. |
| `dspy_enabled` | bool | `false` | Enable the DSPy flywheel: write judge findings as Observations and compile DSRs into Patterns after each cycle. When the cycle runs the flywheel it emits `CycleProgressEvent::FlywheelCompiled` (SSE display "Findings compiled into prompt pattern"). |
| `dspy_pattern_cohort_threshold` | usize | `5` | Minimum Observations in the namespace before a DSPy compilation pass triggers. |
| `tournament_enabled` | bool | `false` | When true, each proposal runs through the three-candidate Borda-count tournament instead of a single `mutator.propose()` call. |
| `objective` | enum | `sharpe` | The metric the cycle optimizes (gate objective). One of `sharpe`, `total_return`, `max_drawdown`, `win_rate`. **Also a CLI flag** — `--objective` overrides per run. |
| `regime_set` | array of `RegimeWindow` | `[]` | Optional **exhaustive** regime matrix. Every regime is evaluated for every candidate; the gate requires improvement across **all** regimes. See [Regime set vs scenario pool](#regime-set-vs-scenario-pool). |
| `scenario_pool` | array of `ScenarioWindowPair` | `[]` | Optional **round-robin sampling** pool. One pair is picked per candidate (`pool[mutation_idx % pool.len()]`), spreading candidates across regimes so a strategy tuned to one fixed window can't dominate (overfitting guard). |
| `baseline_direction` | enum | `both` | Trade-direction the random-baseline edge metric mirrors: `long`, `short`, or `both`. Informational only — never gates promotion. A LONG-only strategy should set `long` so the random counterfactual picks long/flat (never short). |
| `gepa_enabled` | bool | `false` | Use the GEPA reflection+proposal algorithm instead of the single-call summarizer. Requires `dspy_enabled = true`. |
| `gepa_candidates` | usize | `3` | Candidate instructions GEPA generates per generation. |
| `gepa_generations` | usize | `2` | Reflection→proposal generations the GEPA loop runs. More = higher quality, more LLM calls. |

### `[mutator]` table

The experiment writer. All three fields required.

| Field | Type | Default (struct) | Meaning |
|---|---|---|---|
| `provider` | string | `"test"` | Provider id for the experiment-writer model. Must not be empty. |
| `model` | string | `"test-model"` | Model id for the experiment writer. Must not be empty. |
| `max_retries` | u32 | `2` | Retries on a failed proposal call. Must be `<= 10`. |

> The struct defaults (`test` / `test-model`) are test fixtures — set a real
> provider/model in your `autooptimizer.toml` for a live cycle.

### `[[scenario_pool]]` array — `ScenarioWindowPair`

Each entry is one labeled `(day, baseline-untouched)` window pair:

| Field | Type | Meaning |
|---|---|---|
| `label` | string | Unique label (appears in observability logs / the round-robin trace). Duplicates are rejected at config-load. |
| `day` | table `{ start, end }` ISO date | Training/candidate-eval window for this pair. `start` before `end`, span ≤ 120 days. |
| `baseline` | table `{ start, end }` ISO date | Held-out window for this pair. `start` before `end`, span ≤ 120 days. Must be disjoint from `day`. |

### `[[regime_set]]` array — `RegimeWindow`

Each entry adds a directional `side` and uses string dates:

| Field | Type | Meaning |
|---|---|---|
| `label` | string | Unique label. |
| `side` | enum | `bull`, `bear_or_shock`, or `chop`. |
| `day` | table `{ start, end }` (string `YYYY-MM-DD`) | Training/candidate-eval window. Span ≤ 120 days. |
| `baseline` | table `{ start, end }` (string `YYYY-MM-DD`) | Held-out window. Disjoint from `day`. |

## Config fields vs CLI-only flags

Some knobs are **only** CLI flags on `xvn optimize` and are **not** fields in
`autooptimizer.toml`. Putting them in the TOML has no effect (and, if they trip
the parser, will surface a field-level error). Conversely, a few config fields
have a matching CLI flag that overrides them per run.

**CLI-only flags (not config fields):**

| Flag | Notes |
|---|---|
| `--budget` | Per-run experiment/cost budget. CLI-only. |
| `--day-start` / `--day-end` | Override the `day_window` for one run. |
| `--baseline-start` / `--baseline-end` | Override the `baseline_untouched_window` for one run. |
| `--config <path>` | Replace the default config file (see above). |

**Config fields that also have an override flag:**

| Config field | Override flag |
|---|---|
| `experiments_per_cycle` | `--experiments-per-cycle` |
| `objective` | `--objective` (`sharpe`/`total_return`/`max_drawdown`/`win_rate`) |
| `min_improvement` | `--min-improvement` |

> The exact flag list lives on the `xvn optimize` clap definition; treat the
> running `xvn optimize --help` as authoritative if it diverges from this table.

## Scenario pool uses raw DATE WINDOWS, not named scenario IDs

**The optimizer's `[[scenario_pool]]` (and `[[regime_set]]`) take raw date
windows only.** You give each pair literal `day`/`baseline` date ranges.

You **cannot** reference a named scenario id from `xvn scenario ls` here — there
is no `scenario_id = "…"` field on `ScenarioWindowPair` or `RegimeWindow`, and
the parser will reject it. Named scenarios are an **eval-run** concept (`xvn eval
run --scenario <id>`); the Optimizer cycle synthesizes its own scenarios from
the date windows in this config through the same builders the single top-level
`day_window`/`baseline_untouched_window` pair uses.

If you want the cycle to cover a specific historical regime, express it as a
`day`/`baseline` date pair in `[[scenario_pool]]`, not as a scenario id.

## Regime set vs scenario pool

Both default to empty (single top-level window pair, 100% back-compat). They are
**distinct**:

- `regime_set` — **exhaustive**. Every regime is evaluated for every candidate;
  the gate demands improvement across **all** regimes.
- `scenario_pool` — **sampled**. Each candidate gets exactly one pair via
  `pool[mutation_idx % pool.len()]`, so different candidates see different
  regimes. This is the overfitting guard, not a cross-regime gate.

## Window span cap

Any single evaluation window (`day_window`, `baseline_untouched_window`, or any
regime's / pool pair's `day`/`baseline`) is capped at **120 days**
(`MAX_WINDOW_DAYS`). The whole window's bars load into memory per asset per
candidate during the backtest; a multi-month span can OOM the container *after*
the cycle lock is taken and strand the lock. The cap is enforced at
config-validation time — before the lock and before any bars load — so an
over-long window fails fast with a message naming which window to shrink.

## Agent-slot `max_tokens` for CoT / multi-turn models (U14)

This is an **agent-slot** setting (`xvn agent create --max-tokens`, or the
agent JSON), not an `autooptimizer.toml` field — but it bites Optimizer cycles
that use agentic models, so it is documented here.

For **agentic multi-turn / chain-of-thought models** (`deepseek-r1`, `gemma`,
`qwq`, and similar), the agent-slot `max_tokens` acts as a **cumulative output
budget across all tool-call turns**, not a per-message cap. A value that looks
fine for a single response (e.g. 512–1024) gets exhausted mid-reasoning across
turns, truncating the model before it emits a usable decision.

- **Recommendation: 8192–16384** for 8B-class CoT models.
- `xvn strategy diagnostics` now **warns** when an agent slot's `max_tokens`
  looks too small for a CoT model, so run it before launching a cycle with one
  of these models.

## Complete annotated example

```toml
# autooptimizer.toml — derived from AutoOptimizerConfig as of 2026-06-11.
# Lives at $XVN_HOME/autooptimizer.toml by default; pass --config <path> to
# REPLACE this file entirely (not merge).

# Gate epsilon: candidate must beat parent by >= this on the objective.
min_improvement = 0.05

# The metric the cycle optimizes. One of: sharpe | total_return | max_drawdown | win_rate.
# (CLI --objective overrides this per run.)
objective = "sharpe"

# Candidate experiments per parent per cycle (1..=64).
# (CLI --experiments-per-cycle overrides this per run.)
experiments_per_cycle = 5

# Which experiment kinds the writer may propose.
allowed_mutation_kinds = ["prose", "param", "tool", "filter"]

# Random-baseline direction the edge metric mirrors: long | short | both.
# Informational only — never gates promotion.
baseline_direction = "both"

# DSPy flywheel: compile judge findings into Patterns after each cycle.
# Emits CycleProgressEvent::FlywheelCompiled when it runs.
dspy_enabled = false
dspy_pattern_cohort_threshold = 5

# Three-candidate Borda tournament per proposal (default off).
tournament_enabled = false

# GEPA reflection+proposal (requires dspy_enabled = true).
gepa_enabled = false
gepa_candidates = 3
gepa_generations = 2

# Training / candidate-evaluation window (span <= 120 days).
[day_window]
start = "2025-01-01"
end   = "2025-04-01"

# Held-out "untouched test period" (span <= 120 days; disjoint from day_window).
[baseline_untouched_window]
start = "2025-04-01"
end   = "2025-05-01"

# The experiment writer (mutator). All three fields required.
[mutator]
provider    = "anthropic"      # real provider id for a live cycle
model       = "claude-..."     # real model id
max_retries = 2                # <= 10

# Optional: progressive loosening thresholds (each > 0).
# [loosening_schedule]
# day_n_thresholds = [0.1, 0.05]

# Optional ROUND-ROBIN scenario pool (raw date windows — NOT scenario ids).
# One pair is sampled per candidate; overfitting guard.
[[scenario_pool]]
label = "q1-2024"
[scenario_pool.day]
start = "2024-01-01"
end   = "2024-03-01"
[scenario_pool.baseline]
start = "2024-03-01"
end   = "2024-04-01"

[[scenario_pool]]
label = "q3-2024"
[scenario_pool.day]
start = "2024-07-01"
end   = "2024-09-01"
[scenario_pool.baseline]
start = "2024-09-01"
end   = "2024-10-01"

# Optional EXHAUSTIVE regime matrix (every regime evaluated for every candidate;
# gate requires improvement across ALL regimes). Note string dates + `side`.
[[regime_set]]
label = "bull"
side  = "bull"          # bull | bear_or_shock | chop
[regime_set.day]
start = "2024-01-01"
end   = "2024-03-01"
[regime_set.baseline]
start = "2024-03-01"
end   = "2024-04-01"
```
