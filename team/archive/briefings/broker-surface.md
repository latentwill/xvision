# Briefing — `broker-surface` track

You are extracting **Plan 2c §Task 7 — `BrokerSurface` trait + Alpaca/Orderly impls** into v1 test scope. The rest of Plan 2c (scheduler, live daemon, deploy CLI) is **out of scope** for v1 test and stays deferred.

## Plan

[`docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md`](../../docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md) — read **only Task 7**.

## Why this matters

The Eval Engine (Plan #5, downstream) needs `BrokerSurface` to wrap Alpaca paper
fills behind a trait. Without this, the eval paper executor would have to call
the `apca` crate directly, polluting the eval crate with broker-specific code.
Extracting the trait now lets eval depend on `xvision-execution::BrokerSurface`
and stay broker-agnostic.

## Skills required

- `superpowers:executing-plans`
- `superpowers:test-driven-development`
- `superpowers:verification-before-completion`

## Scope

Inside `crates/xvision-execution/`:

1. Define `pub trait BrokerSurface` covering the paper-mode surface (place_order,
   query_position, cancel_order, list_open_orders — read Task 7 of Plan 2c for
   the full signature).
2. Implement `BrokerSurface` for the existing Alpaca client at `crates/xvision-execution/src/alpaca.rs`.
3. Implement `BrokerSurface` for the existing Orderly client (if one exists in
   `crates/xvision-execution/src/orderly.rs`; if not, leave a `todo!()`-bodied
   stub so the trait compiles — Orderly is post-v1).
4. Tests: a `tests/broker_surface_alpaca.rs` integration test that verifies the
   trait dispatch wires up correctly. Use Alpaca paper credentials from env
   vars (skip the test if not set, don't fail).

## What you do NOT do

- ❌ Scheduler crate (`xvision-scheduler`) — deferred entirely.
- ❌ Live daemon (`xvn live deploy`) — deferred.
- ❌ `xvn schedule` CLI — deferred.
- ❌ Migrations `006_scheduler.sql` — reserved but not claimed by you.

## Branch / worktree

- Worktree: `.worktrees/broker-surface`
- Branch: `feature/broker-surface-trait`
- PR title: `feat(execution): BrokerSurface trait extracted from Plan 2c (#4)`

## Cross-track contracts you own

When you commit, downstream tracks (especially eval-engine in Phase B) get:

1. `pub trait BrokerSurface` in `xvision_execution::broker_surface` (or
   wherever the plan specifies).
2. `impl BrokerSurface for AlpacaClient` — paper-mode operations work end-to-end
   when ALPACA_API_KEY/SECRET are set.
3. The trait does **not** include any Mantle / Orderly-Vault chain ops —
   that's wallet-plan territory.

When complete, post:

```
team/queue/broker-surface__<utc>__phase-a-complete.md
to: [eval-engine, coordinator]
ack_required: false

BrokerSurface trait landed. Eval Engine paper executor can now depend on it.
```

## Dependency on engine-api

**None.** This track works on `xvision-execution` only and does not touch the
engine API surface. You can run in parallel with engine-api from day one.

## Completion definition

- `pub trait BrokerSurface` defined and implemented for Alpaca.
- `cargo test -p xvision-execution` green.
- `cargo build --workspace` green.
- PR opened.
- `team/MANIFEST.md` updated to mark broker-surface complete.
- Queue message posted.
