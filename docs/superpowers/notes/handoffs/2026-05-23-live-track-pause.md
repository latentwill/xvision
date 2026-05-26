# LIVE track — paused 2026-05-23

Paused mid-Phase-A of the LIVE-track parallel build-out. This note
captures everything needed to resume cleanly. The 4 architecture
adjudications and the locked `Executor::live(...)` signature live here
too so the next session doesn't have to rediscover them.

Pair with: LE PR (https://github.com/latentwill/xvision/pull/572) which
is the foundation everything else builds on.

## State at pause

| Track | Branch | Status | Where the work is |
|---|---|---|---|
| **LE** (executor-collapse-paper + executor-live-shell) | `live-engine` (commit `7fc5c82`) | **PR #572 open** against `main` | https://github.com/latentwill/xvision/pull/572 |
| **LL1** (Executor::live body + classification lift + re-enable 21 tests) | `live-engine-internals` (off `live-engine`) | **Paused mid-implementation** — Steps 1-4 ~90% done, Step 5 (new tests) just started | Stash `stash@{1}` (on branch `live-engine-internals`): "WIP LL1 live-engine-internals: Executor::live() body + FillRecord.broker_error + classification lift (Step 5 new tests in progress when paused)" |
| **LL2** (migration 037 + `Run.live_config` + store round-trip) | `live-storage` (off `live-engine`) | **Paused mid-implementation** — migration drafted, store changes drafted, hit downstream test-fixture regression | Stash `stash@{0}` (on branch `live-storage`): "WIP LL2 live-storage: migration 037_eval_runs_live_config + Run.live_config + store insert/load (test fixture regression in pool_with_022 hand-picked migration lists when paused)" |
| **LL3** (launch endpoint) | `live-launch-endpoint` (not yet created) | Not started — depends on LL1 + LL2 merge | — |
| **LL4** (freeze + brokers status) | `live-freeze-and-status` (not yet created) | Not started — depends on LL2 merge | — |
| **LF** (frontend launch form + enabled Live radio) | `live-frontend` (off `main` @ `1fa19e6`) | Not started — handled by separate orchestrator | Worktree exists at `.worktrees/live-frontend`; the brief is in this session's transcript |

## Stash references (resumption command)

Stash indices shift if new stashes land between now and resumption. Use the message-pattern lookup form, not the bare index:

```bash
# Resume LL1
cd /Users/edkennedy/Code/xvision/.worktrees/live-engine/.worktrees/live-engine-internals
git stash apply 'stash^{/WIP LL1 live-engine-internals}'

# Resume LL2
cd /Users/edkennedy/Code/xvision/.worktrees/live-engine/.worktrees/live-storage
git stash apply 'stash^{/WIP LL2 live-storage}'
```

If `git stash apply` succeeds and you want to drop the stash afterward, use `git stash drop 'stash^{/...}'`.

## Pre-pause stash diff size (so you know what you're recovering)

- **LL1 stash:** 3 files modified, 289 inserts / 51 deletes in `crates/xvision-engine/src/eval/executor/{backtest.rs, real_broker_fills.rs, traits.rs}`. No new files. Final live-shell tests (Step 5) are not yet in the stash — they were being authored when paused. Last visible work-in-progress was: "I'll replace this test with the new live-shell tests we need (Step 5)".
- **LL2 stash:** 6 files modified + 3 new files. New: `crates/xvision-engine/migrations/037_eval_runs_live_config.sql`, `037_eval_runs_live_config.down.sql`, `crates/xvision-engine/tests/eval_store_live_config.rs`. Modified: `crates/xvision-engine/src/api/{agents.rs, mod.rs, search.rs}`, `crates/xvision-engine/src/eval/{run.rs, store.rs}`, `team/MANIFEST.md` (claimed 037). +149 lines net before the regression turned up.

## Worktree topology (note: doubly-nested by accident — leave it)

```
.worktrees/
├── live-engine/                                # LE worktree (PR #572)
│   └── .worktrees/
│       ├── live-engine-internals/              # LL1
│       └── live-storage/                       # LL2
├── live-frontend/                              # LF (off main)
├── qa23-reconcile/                             # qa23 — separate work, not part of LIVE track
└── (legacy worktrees from other tracks)
```

The nested-paths situation happened because the worker dispatcher was inside `.worktrees/live-engine` when it ran `git worktree add` with a relative path. Functional; just don't be surprised by it.

## The 4 locked architecture adjudications

Captured here because LL1's first pass surfaced them and they need to survive into any future session.

### A1 — `Executor::live(...)` signature (LOCKED)

```rust
impl Executor {
    pub fn live(
        live_config: &LiveConfig,
        broker: Arc<dyn BrokerSurface>,
        bar_source: LiveStream,
        clock: WallClock,
        obs_emitter: Option<ObsEmitter>,
    ) -> anyhow::Result<Self>;
}
```

`CostModel` is **dropped from the signature.** It does not exist in the workspace; live runs use real broker fills so there is no cost simulation in-flight. The empirical cost model is computed by LL4's freeze path from `FillProvenance`, after the run.

LL3 calls this signature verbatim.

### A2 — Broker-error carrier on `FillRecord` (LOCKED)

Add to `FillRecord`:
```rust
#[serde(default)]
pub broker_error: Option<(BrokerErrorClass, String)>,
```

- `BrokerErrorClass` is the existing enum (variants: `BrokerAuth`, `BrokerUnsupported`, `BrokerInsufficientFunds`, `BrokerTimeout`, `BrokerRejected`, `RepeatedBrokerError`).
- `SimulatedFills` always returns `broker_error: None`.
- `RealBrokerFills` populates `Some((class, reason))` on rejection.
- Loose invariant: `broker_error.is_some() ⇒ order_state == Some(Rejected)`. Reverse not enforced.
- `#[serde(default)]` lets historical persisted `FillRecord` rows load with `None`.

### A3 — `ScriptedFillSink` test utility (BLESSED)

Place in `crates/xvision-engine/tests/common/scripted_fill_sink.rs` (create the `tests/common/` module if it doesn't exist). Exposes `submitted() -> Vec<FillRequest>` and `position() -> Position` accessors so the 21 currently-`#[ignore]`d tests can migrate from `MockBrokerSurface` to `Executor::backtest(...).with_fill_sink(ScriptedFillSink::new(...))` without weakening their assertions.

### A4 — `LiveRuntime` storage (BLESSED)

Add to `Executor`:
```rust
live_runtime: Option<tokio::sync::Mutex<LiveRuntime>>,

struct LiveRuntime {
    bar_source: LiveStream,
    clock: WallClock,
    fill_sink: RealBrokerFills,
}
```

Per-bar loop locks once at the top of each tick. `tokio::sync::Mutex` (not `parking_lot`) because the lock is held across `.await`. Backtest mode keeps `live_runtime = None` and uses the existing inline `SimulatedFills` path unchanged.

## Known downstream issue LL2 surfaced (real, must be fixed on resume)

LL2 added a new column to `eval_runs` in migration 037 and updated `store::insert` to write it. This breaks test fixtures that hand-pick which migrations to run via `pool_with_022()` (and similar): the store's INSERT now references `live_config_json` but those test pools never apply migration 037.

Affected (at least): `crates/xvision-engine/tests/eval_runs_agents_agent_id.rs::run_store_round_trips_agents_agent_id`. Almost certainly others using the same `pool_with_xxx()` pattern.

**Fix on resume:** every hand-picked migration list in the test suite that exercises `store::insert(eval_runs, ...)` must include 037. Grep for `pool_with_` to find them all. This is a fixture migration, not a product-behavior change — just a tedious one.

## Pre-existing breakages on `main` that are NOT this track's problem

LE flagged these on commit `7fc5c82`; they were already broken on `main` before the LIVE track started. Resume should flag-not-fix:

- `crates/xvision-cli/tests/strategy_add_filter.rs` — references `CreateAgentRequest.scope_strategy_id` added by agent-firing-filter
- `crates/xvision-cli/tests/strategy_validate_warnings.rs` — same
- `crates/xvision-engine/tests/agent_prompt_schema_drift.rs` — references `PublicManifest.color` added by charts-B0

## Resumption order

1. **Wait for LE PR #572 to merge** (or rebase onto whatever lands). LL1 and LL2 both branch off `live-engine`; if `live-engine` gets squashed-merged into `main`, LL1 and LL2 need to rebase onto the new `main` tip.
2. **Resume LL2 first** — it surfaced the test-fixture regression and resolving it cleanly is a pre-req for any downstream work. After resume:
   - `git stash apply` the LL2 stash
   - Fix the `pool_with_*` regression (grep and patch every hand-picked migration list)
   - Run `cargo test -p xvision-engine --lib eval::store::` + `cargo test -p xvision-engine --test eval_store_live_config` to confirm green
   - Commit + push + PR
3. **Resume LL1 in parallel with LL2's PR review** — LL1 doesn't depend on LL2's storage changes (it touches only `eval/executor/**`). After resume:
   - `git stash apply` the LL1 stash
   - Finish Step 5 (new live-shell tests: timeout-on-bar, broker_auth class trigger, circuit breaker fires at 3 rejections)
   - Verify all 21 re-enabled tests pass
   - Commit + push + PR
4. **Phase B — LL3 + LL4 in parallel** once Phase A merges:
   - `LL3` = `POST /api/eval-runs` mode=live launch endpoint + broker-creds preflight (`GET /v2/account`). Worktree: create off the Phase A integration tip.
   - `LL4` = `POST /api/eval-runs/:id/freeze` + `GET /api/settings/brokers/:ref/status`. Worktree: same.
   - The full briefs for LL3 and LL4 are in the session transcript. Re-derive from the spec table at the top of this note + the locked signatures in A1–A4 if the transcript isn't available.
5. **LF (frontend)** — separate orchestrator owns this. The LF brief lives in the session transcript; it codes against the published endpoint shapes from LL3 (launch) and LL4 (freeze + broker status) and the 11 `LiveConfigValidationError` variants.

## Done-criteria for "Live is completely done"

(So the next session knows when to stop.)

- [ ] LE PR #572 merged
- [ ] LL1 + LL2 merged
- [ ] LL3 + LL4 merged
- [ ] LF merged
- [ ] User can click "Live" in the eval-runs picker (no longer disabled)
- [ ] Launch form collects a `LiveConfig`; submit hits the launch endpoint; broker preflight runs; on success, a Live run starts and produces decisions, fills, trace events through the existing SSE bus
- [ ] Completed Live run shows "Save as historical scenario"; click freezes it to `ScenarioSource::Frozen` and routes to the new scenario
- [ ] All 21 originally `#[ignore]`d tests are green
- [ ] All `pool_with_*` test fixtures include migration 037
- [ ] `cargo test --workspace` green on `main` (modulo the 3 pre-existing breakages flagged above, unless those tracks also fix them in parallel)

## Cleanup TODO before any future deploy

`team/MANIFEST.md` has LL2's claim on migration 037 written (the stash carries the edit but LL2 also wrote the registry entry separately — verify on resume). If LL2's resume changes the number for any reason, update the registry too. The conductor pattern flags migration-number collisions as a recurring failure mode.
