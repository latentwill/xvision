---
track: eval-launch-concurrency-cap
lane: integration
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-launch-concurrency-cap
branch: task/eval-launch-concurrency-cap
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/api/eval.rs                     # start_run entry point (line 1496) — add concurrency gate
  - crates/xvision-engine/src/api/mod.rs                      # ApiContext gets a concurrency-cap struct
  - crates/xvision-engine/src/eval/concurrency.rs             # NEW — semaphore-per-(provider,model)
  - crates/xvision-engine/src/eval/mod.rs                     # `pub mod concurrency;`
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/mod.rs   # F-3 (PR #345) owns this; stay out
  - crates/xvision-engine/src/agent/llm.rs           # F-2 (PR #347) added per-call 429 retry there; this is the launch-time complement
  - frontend/web/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - xvision-engine::api::eval::start_run (line 1496)
  - tokio::sync::Semaphore
parallel_safe: true
parallel_conflicts:
  - eval-run-watchdog-and-stuck-running (PR #345, F-3 — touches executor/mod.rs, not start_run; should be disjoint)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::concurrency
  - cargo test -p xvision-engine api::eval::start_run
acceptance:
  - New `eval::concurrency` module exposes a `LaunchConcurrencyGate` keyed by `(provider, model)`. Each key gets its own `tokio::sync::Semaphore` with a configurable permit count (default 4) that callers acquire via `gate.acquire(provider, model).await -> PermitGuard`. Holding the guard for the full run lifecycle gates *new launches* — running runs keep their permits until finalize.
  - Config:
    * Global default: `XVN_EVAL_MAX_CONCURRENT_PER_MODEL` (env, default 4).
    * Per-(provider, model) overrides via an in-memory map seeded from engine config (initially empty; can be populated later from a config file without breaking the API).
  - `start_run` (`api/eval.rs:1496`) acquires the permit **after** preflight (broker/dispatch built) and **before** spawning `execute_in_background`. The permit is held until the eval finalizes (success or failure) so a 27-runs-in-15-seconds burst queues at the gate instead of fanning out to the provider.
  - **No serial-finalize-write fix here** — the slow-statement hotspot (`UPDATE eval_runs SET status='failed' … elapsed=1.029s` observed in the audit) is a separate downstream concern; document the deferral in the PR body with a TODO referencing intake F-1.
  - **No 429 retry here** — that landed in F-2 (PR #347). This contract is the *launch-time* complement: don't let so many launches happen that even per-call 429 retries can't catch up.
  - Tests:
    * Unit: gate with permits=2 admits 2 concurrent acquires; a 3rd blocks; releasing one admits the blocked acquire.
    * Unit: distinct (provider, model) keys do not block each other (separate semaphores).
    * Integration: simulate 10 `start_run` calls with the same (provider, model) and permits=4; assert only 4 are in-flight at any time and total wall-clock ≥ ceil(10/4) × per-run time minus jitter.
    * Stress (best-effort, behind `#[ignore]` if slow): the audit's 27-in-15s burst no longer produces a 429 storm; instead it serializes through the gate.
  - Acceptance audit: re-running the audit's launch pattern with these defaults bounds total token burn at `permits × per-run-tokens`, not `bursts × per-run-tokens`.
---

# Scope

Intake F-1 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The audit found a P0 incident: at 2026-05-19T14:22:52–14:23:07Z, 27 eval
runs launched in a 15-second window all hit `google/gemini-3.1-flash-lite`
on OpenRouter (450 RPM limit) and burned ~18.3M input tokens before
failing. No launch-time gate existed.

This contract adds the launch-time gate. The per-call 429 retry (also
audit F-1) was already landed by F-2 (PR #347). The serial-finalize-write
hotspot (also audit F-1) is deferred — document the TODO in the PR body
referencing intake F-1; it's a separate change to the SQLite write path
that needs its own contract.

# Out of scope

- Per-call 429 retry (already in F-2, PR #347).
- Serial finalize-write hotspot (deferred — separate contract).
- Frontend visualization of queued vs in-flight runs.
- Cross-process concurrency (single-process gate is sufficient for v1;
  multi-replica work is a future track).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-launch-concurrency-cap status
git -C .worktrees/eval-launch-concurrency-cap log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-launch-concurrency-cap -b task/eval-launch-concurrency-cap origin/main
```

# Notes

Keep the gate keyed on `(provider, model)` not just `provider` — different
model slugs have different RPM budgets even within OpenRouter. The
`LaunchConcurrencyGate` lives on `ApiContext` so it survives between
`start_run` calls in the same process.
