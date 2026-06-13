# CE-Plan: Byreal venue wiring + live/backtest path parity

**Date:** 2026-06-13
**Orchestrator:** Claude (workflow-driven)
**Tracking:** beads epics `xvision-ym9v` (Byreal) and `xvision-ar6u` (parity)
**Builds on:** PR #962 (ByrealPerpsExecutor + filter perps + feed), PR #963 (shared seed constructor)

Two tracks, each its own branch/PR, both based on `refactor/shared-seed-constructor`
(#963). Implemented via background workflows that edit isolated worktrees and
self-verify; the orchestrator commits, pushes, opens PRs, and closes beads only
when code is committed+pushed (per the `beads-closed-but-code-uncommitted` lesson).

---

## Track 1 — Byreal venue wiring (epic `xvision-ym9v`)

**Problem.** `ByrealPerpsExecutor` exists (PR #962) but is wired into nothing —
no CLI venue, config, live-eval, settings, or docs path reaches it. Gap analysis
(subagent, 2026-06-13) found 8 concrete gaps. Goal: a self-hosted `xvn` user can
select Byreal as their execution venue end-to-end.

**Branch:** `feat/byreal-venue-wiring` → PR base `refactor/shared-seed-constructor`.
**Worktree:** `.worktrees/byreal-wiring`.

| Bead | Item | Sites |
|---|---|---|
| `.1` | CLI `Venue::Byreal` + `executor_from_env` + `--help` | `xvision-cli/.../venue.rs:12` |
| `.2` | `fire_trade` Byreal arm | `fire_trade.rs:92` |
| `.3` | config `ExecutorKind::Byreal` | `xvision-core/src/config.rs:283` |
| `.4` | live eval `LiveVenue::ByrealLive` + resolve/build | `eval.rs:3582,3591` |
| `.5` | `doctor` Byreal env/cred check | `doctor.rs` |
| `.6` | settings backend (`BrokersReport.byreal` + dashboard routes) | `api/settings/brokers.rs:36` |
| `.7` | settings frontend (Byreal card + venue picker) | `routes/settings/index.tsx:89`, `eval-runs.tsx:811` |
| `.8` | docs + **Skills** + CLI help (operator-requested) | `MANUAL.md`, `docker/README`, `.claude/skills/xvision-cli/SKILL.md` |
| `.9` | testnet live-order smoke — **manual operator gate** | requires real creds/network; not agent-run |

**Acceptance:** `cargo build --workspace` + `npm run build` green; every
`Venue`/`LiveVenue`/`ExecutorKind` match is exhaustive with a Byreal arm; a
power-user can run `xvn fire-trade --venue byreal` and a live run with
`broker_creds_ref="byreal"` using `BYREAL_*` env vars; secrets never logged;
no-popups + dark-border UI rules honored. `.9` (real testnet order) is the final
operator-run acceptance, parallel to `xvision-wsf` (Orderly testnet validation).

**Smallest-unblock note:** items `.1`/`.2`/`.4` alone let a power-user trade via
env vars + CLI/payload without any UI. The rest is the "make it nice" layer.

---

## Track 2 — Live/backtest path parity (epic `xvision-ar6u`)

**Problem.** PR #963 unified the seed *derivation* (`from_context`), but
`run_inner` (backtest) and `run_inner_live` still duplicate the per-decision
*prologue* (history-slice windowing, `bar_history_limit` truncation,
`book.position`/`entry_price` reads). Same drift trap, larger surface. Plus stale
`paper::` "lock-step" comments referencing a module that no longer exists.

**Branch:** `refactor/seed-context-prologue` → PR base `refactor/shared-seed-constructor`.
**Worktree:** `.worktrees/seed-prologue`.

| Bead | Item | Sites |
|---|---|---|
| `.1` | extract shared `build_seed_context` prologue | `backtest.rs:556` (run_inner ~1100-1145) vs `:2908` (run_inner_live ~3457-3496) |
| `.2` | remove stale `paper::` lock-step comments | `backtest.rs:5329,5352,5363` |
| `.3` | reconcile `active_assets` "kept in sync" | `eval.rs:3450` |
| `.4` | extend parity golden test to the prologue | `tests/eval_causal_input_sanitization.rs` |

**Acceptance:** behavior-preserving — existing byte-shape regression tests
(`raw/causal/oracle_top_level_seed_*`) unchanged; both loops build the same
`SeedContext` via one function; parity golden test extended with the
`INTENTIONALLY_DIVERGENT` allowlist; no phantom-mirror comments remain.

---

## Execution structure (workflows)

Each track = one background workflow editing its worktree:
- **Implement** — compile-coupled Rust serialized (one cluster sees the prior's
  edits), frontend in parallel (disjoint tree), each agent self-verifies with
  `scripts/cargo` / `npm`. Agents do **not** commit.
- **Docs** — docs/skills/help after implement.
- **Verify** — parallel adversarial reviewers (compile+completeness; cred-safety
  + UI conventions / parity-preservation).

Workflows run **serially** (Byreal then parity) to avoid two branches thrashing
the shared `CARGO_TARGET_DIR` engine cache.

Orchestrator owns: full-workspace build/test, git commit/push, PR creation,
beads transitions (a bead closes only after its code is committed+pushed).

## Branch/PR stack & merge order

```
main ─ #962 perps ─ #963 seed-refactor ─┬─ feat/byreal-venue-wiring   (PR, base #963)
                                         └─ refactor/seed-context-prologue (PR, base #963)
```
Merge order: #962 → #963 → {byreal, parity, either order}. Retarget the two new
PRs' base to `main` once #963 merges.

## Out of scope / deferred
- Real Byreal testnet order placement (`.9`) — operator-run with live creds.
- Orderly-side changes — untouched; byreal mirrors orderly's pattern.
