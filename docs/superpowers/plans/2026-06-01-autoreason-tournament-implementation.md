# Autoreason tournament implementation plan

> Date: 2026-06-01
> Status: ready to implement
> Spec: `docs/superpowers/specs/2026-05-30-autoreason-tournament-integration-design.md`
> Branch: `feat/autoreason-tournament`

## Numbered tasks

### Task 1 — Prompt files

Create three new prompt files under
`crates/xvision-engine/prompts/autoresearch/`:

- `tournament-adversarial-v1.md` — system prompt for bold, high-variance candidate
- `tournament-synthesis-v1.md` — system prompt for focused, synthesis candidate
- `tournament-judge-v1.md` — system prompt for Borda-count judge ranking

All three use the same JSON diff schema as `mutator-v1.md` (adversarial +
synthesis) or return `{"ranking": [i, j, k]}` (judge).

### Task 2 — `tournament.rs`

Create `crates/xvision-engine/src/autoresearch/tournament.rs`:

```
Types:
  CandidateKind (enum: Incumbent, Adversarial, Synthesis)
  TournamentCandidate { kind, strategy, diff }
  BordaVote { ranking: [usize; 3] }
  TournamentResult { winner_kind, winner_diff, winner_strategy, incumbent_wins, borda_scores }

Runner:
  TournamentRunner { dispatch, model, provider, max_retries }
  ::from_mutator(m: &Mutator) -> Self
  ::generate_candidates(parent, config) -> Result<Vec<TournamentCandidate>>
    - Incumbent: clone + empty_mutation()
    - Adversarial + Synthesis: tokio::try_join! two LLM propose_diff calls
  ::borda_vote(candidates) -> Result<Vec<BordaVote>>
    - 3 parallel judge calls via tokio::try_join!
  ::tally(votes) -> [u32; 3]   (pure fn, no await)
  ::run_tournament(parent, config) -> Result<TournamentResult>

Tests (in #[cfg(test)]):
  - tournament_produces_3_candidates: MockDispatch returning valid diffs → 3 candidates
  - borda_tally_picks_winner: unit-test tally() with fixed vote arrays
  - incumbent_wins_when_ranked_first: MockDispatch judges rank incumbent first → incumbent_wins = true
```

### Task 3 — `progress.rs` addition

Add one variant to `CycleProgressEvent`:

```rust
TournamentIncumbentWon { cycle_id: String, parent_hash: String },
```

### Task 4 — `cycle.rs` changes

1. Add `pub use_tournament: bool` to `CycleConfig`.
2. In `process_parent_mutations`, replace the direct `mutator.propose()` call:
   - If `cycle_config.use_tournament`: call `TournamentRunner::run_tournament()`;
     on `incumbent_wins` emit `TournamentIncumbentWon` and `continue`.
   - Else: call `mutator.propose()` as before.
3. No other changes to the gate/classify/lineage path.

### Task 5 — `mod.rs` update

Add `pub mod tournament;` and re-export:
```rust
pub use tournament::{
    BordaVote, CandidateKind, TournamentCandidate, TournamentResult, TournamentRunner,
};
```

### Task 6 — CLI and integration-test fixup

Update both `CycleConfig { ... }` construction sites to add
`use_tournament: false` (backward compatible, single-shot default):

- `crates/xvision-cli/src/commands/autoresearch.rs:1188`
- `crates/xvision-engine/tests/autoresearch_cycle.rs:369`

### Task 7 — Build + test

```bash
CARGO_TARGET_DIR=$HOME/.cargo-target/xvision-autoreason \
  scripts/cargo build --workspace

CARGO_TARGET_DIR=$HOME/.cargo-target/xvision-autoreason \
  scripts/cargo test -p xvision-engine
```

Both must pass with zero warnings treated as errors.

### Task 8 — Commit

```bash
git add -f docs/superpowers/specs/2026-05-30-autoreason-tournament-integration-design.md
git add -f docs/superpowers/plans/2026-06-01-autoreason-tournament-implementation.md
git add crates/xvision-engine/src/autoresearch/tournament.rs
git add crates/xvision-engine/src/autoresearch/cycle.rs
git add crates/xvision-engine/src/autoresearch/progress.rs
git add crates/xvision-engine/src/autoresearch/mod.rs
git add crates/xvision-engine/prompts/autoresearch/tournament-adversarial-v1.md
git add crates/xvision-engine/prompts/autoresearch/tournament-synthesis-v1.md
git add crates/xvision-engine/prompts/autoresearch/tournament-judge-v1.md
git add crates/xvision-cli/src/commands/autoresearch.rs
git add crates/xvision-engine/tests/autoresearch_cycle.rs
```

## Acceptance gate

Per the spec (§9):

1. `cargo test -p xvision-engine` passes (all tournament unit tests green)
2. `cargo build --workspace` passes with zero warnings
3. The existing `autoresearch_cycle.rs` integration test still passes
   (backward compat: `use_tournament: false` takes the original single-shot path)
4. Tournament test with a MockDispatch that returns sabotage diff as the
   adversarial candidate confirms incumbent wins when judges rank it last
