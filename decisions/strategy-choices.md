# Strategy choices — deferred for review

A short, living queue of tactical decisions made during implementation that
deserve a strategy-level look before v1 ships. ADRs (`0001`–`0007`) record
*decided* choices with full context. This file records *defaulted* choices
that were taken to keep work moving and that the operator wants to revisit
once the system is end-to-end runnable.

Format: title → origin → current default → question to settle → owner /
when to revisit.

---

## 1. `Action::Close` treatment in risk-layer limit rules

- **Origin:** Phase 5, `crates/xianvec-risk/src/rules/{max_total_exposure,max_open_positions,daily_loss_circuit}.rs`. Implementation surface required a decision because `xianvec-core::Action` has four variants (`Buy`, `Sell`, `Flat`, `Close`) and the implementation-plan §5 rule descriptions reference only `Flat`/non-flat.
- **Current default:** `Action::Close` is treated identically to `Action::Flat` for limit checks — it passes `MaxTotalExposure`, `MaxOpenPositions`, and the `DailyLossCircuit` veto unconditionally. Rationale: closing a position can only *reduce* exposure or open-count, so a limit rule that vetoes a close would be self-defeating.
- **Question to settle:** is `Close` ever valid as a *new* directional action that should be subject to size/exposure caps? Specifically: in a multi-asset world, can the agent emit `Close` against an asset it does *not* currently hold (a "flatten if I'm in" hint), and if so should that no-op pass silently or be rejected as malformed? Current default lets it pass silently.
- **Impact if revisited:** if `Close` should be rejected when no matching position exists, that's a new `VetoReason::ClosingNonexistentPosition` and a rule check. v1 BTC-only path is unaffected because the trader prompt schema constrains `Close` to existing positions.
- **Revisit when:** before multi-asset is enabled (post-headline-result, see `whitelist.toml`). Owner: pipeline / risk semantics.

---

## 2. `TakeProfitRR` modification uses `VetoReason::Custom("rr_too_low")`

- **Origin:** Phase 5, `crates/xianvec-risk/src/rules/take_profit_rr.rs`. The R/R-widening modification needs a `VetoReason` to attach to `RiskDecision::Modified { reason }`, but `xianvec-core::trading::VetoReason` does not enumerate a dedicated variant for this case.
- **Current default:** `VetoReason::Custom("rr_too_low")`. Functionally correct — the verdict is recorded, downstream code can match on the string — but the catch-all `Custom` variant erodes the value of the enum's exhaustiveness.
- **Question to settle:** add a first-class variant (e.g. `RiskRewardTooLow` or `TakeProfitTooTight`) to `VetoReason` so the rule's modification reason appears in the schema, the decision divergence analysis can group on it cleanly, and audit dashboards don't need string parsing. One-line schema add + serde rename + cascade through any `match VetoReason {...}` blocks.
- **Impact if revisited:** schema migration in `xianvec-core::trading.rs` only; no SQL migration (the `risk_outcomes` table stores reasons as JSON-tagged enum). Trade-off: every `Custom(_)` site we add is a small claim that the enum is incomplete; the cleaner read is to add the variant now while the enum is still small enough to keep cohesive.
- **Revisit when:** any other `Custom` reason gets added in Phase 6 or Phase 8 — that's the trigger that says "the enum is no longer exhaustive enough." Owner: schema / pipeline.

---

## See also

- `decisions/0007-inference-throughput-routes.md` — option B (mlx-rs spike) is deferred until cold-start latency materially blocks forward paper. Not a strategy choice in the same sense (it's a measurable trigger), but related deferral.
- `decisions/0005-lookahead-audit.md` "Follow-ups" — three Phase 9 harness items (setup_id reuse guard, boundary-condition test, snapshot-invariant docs). Tactical, not strategic; tracked separately on the Phase 9 todo.
