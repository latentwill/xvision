---
track: wire-min-notional-into-eval
worktree: .worktrees/wire-min-notional-into-eval
branch: task/wire-min-notional-into-eval
base: origin/main
phase: in-progress
last_updated: 2026-05-19T00:00:00Z
owner: claude
---

# Risk-config plumbing choice

**Decision: per-run, best-effort file read at `build_paper_executor`
time.** Resolved via a small `paper_min_notional_usd(ctx)` helper that
reads `$XVN_HOME/config/risk.toml` (with `XVN_RISK_CONFIG_PATH`
override) and returns `cfg.venue_limits("paper").min_notional_usd`,
defaulting to `0.0` on any of: file missing, parse failure, venue
absent.

## Why this and not the alternatives

Three plumbing options on the table:

1. **Per-run file read in `build_paper_executor`** (chosen). One
   helper, ~30 lines, no API changes anywhere else. Cost: one
   ~30-line TOML parse per paper run start (negligible next to
   executor setup, fixture loads, and DB writes).
2. **Inject `RiskConfig` (or its `min_notional`) via
   `run_with_deps` parameters.** Threads a new arg through the public
   testable surface and every call site that constructs a paper
   executor (CLI, MCP, dashboard handler). Forces a public API
   change for a single `f64`. Not justified for a one-line builder
   wire.
3. **Add a `risk_config: RiskConfig` field on `ApiContext`.** Most
   architecturally pure — the engine already loads `default.toml`
   on demand; risk.toml could be cached in the context. But:
   (a) `ApiContext` today holds no risk-layer state — adding one
   here is the start of a bigger refactor (engine doesn't depend
   on `xvision-risk` yet — this PR adds that dep); (b) the cycle
   would touch every `ApiContext::new` call site, all outside this
   contract's `allowed_paths`; (c) the engine never re-reads
   risk.toml during a run today, so per-run-start is the same
   cadence we already have for `default.toml` lookups
   (`runtime_config_path` → `load_runtime`).

Option (1) is the smallest viable wiring that delivers the
contract's acceptance criteria. If a future track needs the risk
config in more api paths (live executor wiring, dashboard surfaces,
MCP verbs), promoting it to `ApiContext` is the natural next step
and that track will already touch `ApiContext`.

## Default-on-missing → 0.0 (no-op)

Matches the contract from PR #324: `0.0` disables the rule for the
venue. Failures (missing file, parse error) emit a `tracing::warn`
but never panic and never bubble — risk-layer crate already
validates the file at the top of every run, so production paths
see a well-formed config. The graceful default keeps eval working
on hosts where `risk.toml` was never installed (e.g., fresh
checkouts where the operator hasn't run `xvn setup` yet).

## Files touched

- `crates/xvision-engine/Cargo.toml` — adds `xvision-risk = { path = "../xvision-risk" }`.
  Verified no circular dep: `xvision-risk` depends only on
  `xvision-core` + serde/toml/thiserror/tracing.
- `crates/xvision-engine/src/api/eval.rs` — chains
  `.with_min_notional_usd(paper_min_notional_usd(ctx))` on the
  `PaperExecutor::with_bars(...)` call site at the original
  contract line range (~1335). Adds `paper_min_notional_usd`
  helper.
- `crates/xvision-engine/tests/api_eval_min_notional.rs` —
  end-to-end test exercising `run_with_deps`. Two cases:
  veto-fires-when-risk-toml-present, no-op-when-risk-toml-absent
  (the control).

## Notes

- `XVN_RISK_CONFIG_PATH` env override is added so tests / operators
  can point at a non-`$XVN_HOME` config without symlinking. Mirrors
  the existing `XVN_CONFIG_PATH` override on `default.toml`.
- The CLI's `xvn risk show-config` already takes
  `--path config/risk.toml` (defaults to cwd-relative). The engine
  side now consistently looks under `$XVN_HOME/config/risk.toml`,
  which is what the eval API runs out of. If those diverge in
  practice, a follow-up should unify them — out of scope here.
