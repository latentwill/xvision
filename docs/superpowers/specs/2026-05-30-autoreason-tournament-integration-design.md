# Autoreason tournament integration design

> Date: 2026-05-30
> Status: approved — implementation plan at
> `docs/superpowers/plans/2026-06-01-autoreason-tournament-implementation.md`
> Owner: autoresearcher layer (Phase 4 of the master spine)

## 1. Problem statement

The current AR-1 mutator is single-shot: one LLM call proposes a diff,
a validator checks it, the numeric gate screens it. This produces three
known failure modes:

- **Prompt bias.** The mutator prompt steers toward incremental prose
  tweaks. Bold structural changes are systematically under-proposed.
- **Scope creep.** Without a restraint option, every cycle commits at
  least one mutation even when no change is an improvement.
- **Noise acceptance.** A single LLM draw has high variance; a bad
  draw that happens to clear the numeric gate gets committed.

The autoreason tournament (Phase 4 of
`docs/superpowers/plans/2026-05-27-autoresearcher-master-implementation-spine.md`)
replaces the single-shot mutator with a three-candidate tournament that
addresses all three failure modes. The numeric gate from AR-1 still runs
against the tournament winner; the tournament is purely a
candidate-generation strategy upgrade.

## 2. Three candidates

Each tournament round generates exactly three candidates from a parent
strategy:

| Index | Kind | How produced | Restraint role |
|---|---|---|---|
| 0 | `Incumbent` | Clone of parent; empty diff | "Do nothing" first-class option |
| 1 | `Adversarial` | LLM call with adversarial system prompt | High-variance exploration |
| 2 | `Synthesis` | LLM call with synthesis system prompt | Low-variance refinement |

Candidates 1 and 2 are generated via parallel LLM calls using the same
JSON diff schema as the existing mutator. The adversarial prompt instructs
the model to propose a **bold, high-variance change** in a new direction.
The synthesis prompt instructs the model to identify the **strongest
existing element** and propose one small targeted change to amplify it.

Token budget: ~3× the single-shot cost per round. At Haiku pricing
this is still cheap per cycle.

## 3. Borda-count judging

After candidates are generated, three independent LLM judges each rank
all three candidates from best to worst. The judges are:

- Blind to the other judges' rankings
- Blind to performance metrics (no Sharpe/drawdown/profit_factor terms
  in the candidate summaries passed to judges)
- Each receives the parent strategy program view and a description of
  each candidate's proposed diff (kind label + rationale + diff JSON)

**Borda scoring** (3 candidates, 3 judges, max score 6):

| Rank position | Points |
|---|---|
| 1st | 2 |
| 2nd | 1 |
| 3rd | 0 |

Each judge's vote contributes independently. Total scores sum to 9
(3 judges × (2+1+0) = 9 points distributed across 3 candidates).

**Tie-breaking**: on equal Borda scores, the incumbent (index 0) wins.
This preserves the "do nothing" preference when judges are split.

## 4. API shape

### Rust types (developer surface)

```rust
pub enum CandidateKind {
    Incumbent,
    Adversarial,
    Synthesis,
}

pub struct TournamentCandidate {
    pub kind: CandidateKind,
    pub strategy: Strategy,
    pub diff: MutationDiff,   // empty for Incumbent
}

pub struct BordaVote {
    /// Candidate indices ordered best-first: ranking[0] = 1st place.
    pub ranking: [usize; 3],
}

pub struct TournamentResult {
    pub winner_kind: CandidateKind,
    pub winner_diff: MutationDiff,
    pub winner_strategy: Strategy,
    pub incumbent_wins: bool,
    pub borda_scores: [u32; 3],
}

pub struct TournamentRunner {
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub model: String,
    pub provider: String,
    pub max_retries: u32,
}
```

### Key methods

```rust
impl TournamentRunner {
    pub fn from_mutator(m: &Mutator) -> Self;
    pub async fn generate_candidates(&self, parent: &Strategy, config: &AutoresearchConfig) -> Result<Vec<TournamentCandidate>>;
    pub async fn borda_vote(&self, candidates: &[TournamentCandidate]) -> Result<Vec<BordaVote>>;
    pub fn tally(votes: &[BordaVote]) -> [u32; 3];
    pub async fn run_tournament(&self, parent: &Strategy, config: &AutoresearchConfig) -> Result<TournamentResult>;
}
```

## 5. Numeric gate interaction

The tournament selects a winner. The numeric gate from AR-1
(`crates/xvision-engine/src/autoresearch/gate.rs`, `evaluate()`) then
runs against the winner before any lineage commit. The gate's role is
unchanged: the tournament only changes HOW the candidate is selected,
not whether it clears the quality bar.

Incumbent wins bypass the gate entirely (no diff to test).

## 6. "Do nothing" as first-class outcome

If the tournament incumbent wins:
- `TournamentResult { incumbent_wins: true }` is returned
- `cycle.rs` records a `CycleProgressEvent::TournamentIncumbentWon` SSE
  event and skips the lineage commit for this round
- No lineage node is written (no ghost, no active)
- The parent's lineage entry is unchanged

This is the primary mechanism by which the tournament reduces noise
acceptance: when no proposed change is better than the status quo, the
cycle leaves the parent untouched.

## 7. CycleConfig opt-in flag

Tournament mode is opt-in via `CycleConfig { use_tournament: bool }`.
When `false` (default), the cycle uses the existing single-shot
`mutator.propose()` path unchanged. When `true`, the cycle uses
`TournamentRunner::run_tournament()`.

This ensures backward compatibility and allows operators to control the
3× token budget increase.

## 8. Allowed implementation paths

Per Phase 4 of the master spine:

| Path | Change |
|---|---|
| `crates/xvision-engine/src/autoresearch/tournament.rs` | New — all types + runner |
| `crates/xvision-engine/src/autoresearch/cycle.rs` | Wire in tournament; add `use_tournament` to CycleConfig |
| `crates/xvision-engine/src/autoresearch/progress.rs` | Add `TournamentIncumbentWon` event variant |
| `crates/xvision-engine/src/autoresearch/mod.rs` | `pub mod tournament; pub use tournament::{...}` |
| `crates/xvision-engine/prompts/autoresearch/tournament-adversarial-v1.md` | New prompt |
| `crates/xvision-engine/prompts/autoresearch/tournament-synthesis-v1.md` | New prompt |
| `crates/xvision-engine/prompts/autoresearch/tournament-judge-v1.md` | New prompt |

Forbidden: `frontend/`, `xvision-cli/` (CLI uses CycleConfig from the engine;
no new CLI verbs needed), marketplace code.

## 9. Acceptance criterion

Tournament beats single-shot on a synthetic cohort under this metric:

**Noise rejection rate** on a cohort of 50 parent strategies where a
known-bad mutation (the sabotage variant from `build_sabotaged_strategy`)
is injected as one of the proposals:

- Single-shot (current): the sabotaged mutation is proposed ~33% of the
  time (1 in 3 random draws); the gate is the only screen.
- Tournament: even if the sabotaged mutation is proposed as the
  adversarial candidate, the Borda judges are expected to rank it last
  ≥80% of the time (based on diff quality alone, not metrics), so the
  incumbent or synthesis candidate wins instead.

**Target**: tournament achieves ≥80% noise rejection rate on the
synthetic sabotage cohort vs ~33% for single-shot (before the gate).
The gate's contribution is separate and not counted toward this metric.

This is verifiable via a unit test with `MockDispatch` that injects the
sabotage diff as the adversarial candidate and checks that the incumbent
or synthesis wins the tournament when judges correctly rank the sabotage
diff last.

## 10. Operator-surface names

Per the terminology lock
(`docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`),
the following operator-facing labels apply:

| Developer name | Operator label |
|---|---|
| `TournamentRunner` | (internal) |
| `CandidateKind::Incumbent` | "No change" |
| `CandidateKind::Adversarial` | "Bold experiment" |
| `CandidateKind::Synthesis` | "Focused experiment" |
| `TournamentIncumbentWon` SSE event | "No change won" |
| `use_tournament: true` | tournament mode |

No new entries need to be added to the terminology lock document because
the only new operator-visible surface is the `TournamentIncumbentWon`
SSE event, and the lock's existing pattern covers it via the
"Evening summary" and "Experiment" vocabulary.
