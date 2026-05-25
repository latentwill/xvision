# Phase 4.3 (tune & mint) + 4.4 (metrics & holdout discipline) — backend vertical

Branch: `task/p4-tune-mint-holdout`
Worktree: `/Users/edkennedy/Code/xvision-p4-mint`
Migration added: **046** (`046_holdout.sql` / `046_holdout.down.sql`), wired into
`ApiContext::open` via `migrate_holdout`.

## What was built

### Engine `mint` module (dspy-free, pure + testable)
`crates/xvision-engine/src/mint/`
- `metrics.rs` — per-capability required-metric registry.
  - trader battery: `forward_return_agreement, sharpe, max_drawdown, profit_factor,
    calibration, action_validity, selectivity, net_of_cost`
  - filter battery: `precision, recall, f1, auroc, wake_rate, token_savings,
    false_suppression`
- `holdout.rs` — `HoldoutStore` (record / get / waive_overfit) over the new
  `optimization_holdout_results` table + deterministic overfit detection
  (`detect_overfit`: relative drop `(train - holdout)/|train|` vs threshold
  `DEFAULT_OVERFIT_THRESHOLD = 0.30`). `record` is `INSERT OR REPLACE` and resets
  any prior waiver (a fresh measurement must be re-justified).
- `gate.rs` — pure decision functions + typed refusals:
  - `check_accept(AcceptInputs) -> Result<AcceptDecision, AcceptRefusal>` —
    refuses (`accept_missing_holdout`) when there is no holdout result and no
    non-empty override reason.
  - `check_marketplace_mint(MintInputs) -> Result<MintDecision, MintRefusal>` —
    refuses in fixed precedence: `mint_missing_lineage` → `mint_missing_eval_proof`
    → `mint_unwaived_overfit` → `mint_incomplete_metrics`.

Exported from `xvision_engine` lib root.

### Dashboard routes (thin over the engine gates; stays dspy-free)
`crates/xvision-dashboard/src/routes/`
- `optimizations.rs` — EXTENDED the existing `accept` (did NOT duplicate the
  clone-parent / swap-prompt / record-lineage logic): added the holdout-presence
  gate before the clone, `override_reason` recorded on the child's description,
  and `holdout_present` / `override_reason` / `overfit_warning` on the response.
  New endpoints:
  - `POST /api/optimizations/:id/snapshots/:sid/holdout` — record paired
    train/holdout values; engine computes the overfit verdict.
  - `POST /api/optimizations/:id/snapshots/:sid/waive-overfit` — record a
    non-empty waiver reason lifting the overfit mint-block.
  - `POST /api/optimizations/:id/mint` — the marketplace-mint **refusal barrier**.
- `strategies.rs` — new `POST /api/strategy/:id/swap-agent` `{role, child_agent_id,
  session_id?}`: checkpoints the strategy (`CheckpointKind::Other("pre_swap")`)
  via the existing `Checkpointer`, swaps the `AgentRef.agent_id` at `role`, saves,
  returns a reversible diff (`previous_agent_id`, `new_agent_id`, `checkpoint_id`,
  `session_id`). Restore (`/api/chat-rail/checkpoints/:cid/restore`) recovers the
  original strategy byte-for-byte (including the original AgentRef).

Routes registered in `server.rs`.

## Discipline proven

- **accept-without-holdout is REFUSED (typed)** unless `override_reason` given.
  HTTP test `accept_without_holdout_is_refused_typed` → 400 `{code: validation,
  field: accept_missing_holdout}`. `accept_without_holdout_allowed_with_override_reason`
  → 200, override recorded on the child agent description.
- **overfit blocks marketplace mint unless waived.**
  `overfit_blocks_mint_until_waived`: record train 1.0 / holdout 0.4 (ratio 0.6 >
  0.30 → `overfit_warning=true`) → accept allowed (flags overfit) → mint **blocked**
  (`mint_unwaived_overfit`) → waive with reason → mint **succeeds**
  (`decision.overfit_waived=true`).
- **mint refused without lineage / metric coverage** — `mint_missing_lineage`,
  `mint_incomplete_metrics` (short metric battery rejected).
- **swap is checkpointed + reversible** — `swap_agent_is_checkpointed_and_reversible`:
  swap parent→child on disk, restore the checkpoint, AgentRef reverts to parent
  and the strategy file is byte-identical to the original.

## Marketplace-mint refusal transcript (from `overfit_blocks_mint_until_waived`)

```
POST /api/optimizations/<run>/mint
  body: { child_agent_id, eval_run_id: "ev-123", eval_metric: "sharpe",
          metrics_present: [<full trader battery>] }
→ 400 { "code": "validation", "field": "mint_unwaived_overfit",
        "message": "child agent <id> has an unwaived overfit warning (ratio Some(0.6));
                    marketplace mint blocked until waived" }

POST /api/optimizations/<run>/snapshots/<snap>/waive-overfit
  body: { reason: "acceptable for high-vol regime; reviewed" }
→ 200

POST /api/optimizations/<run>/mint   (same body)
→ 200 { "decision": { "child_agent_id": "<id>", "capability": "trader",
                       "eval_run_id": "ev-123", "overfit_waived": true,
                       "holdout_snapshot_id": "<snap>" } }
```

## Test output (verbatim)

### Engine — `cargo test -p xvision-engine --lib mint::`
```
running 24 tests
test mint::gate::tests::accept_refused_without_holdout_or_override ... ok
test mint::gate::tests::mint_refused_without_eval_proof ... ok
test mint::gate::tests::mint_refused_without_lineage ... ok
test mint::gate::tests::mint_allowed_when_overfit_waived ... ok
test mint::holdout::tests::detect_overfit_trips_above_threshold ... ok
test mint::holdout::tests::detect_overfit_custom_threshold ... ok
test mint::holdout::tests::detect_overfit_silent_within_threshold ... ok
test mint::holdout::tests::detect_overfit_holdout_better_never_trips ... ok
test mint::gate::tests::accept_refused_with_blank_override ... ok
test mint::gate::tests::accept_allowed_with_override_reason ... ok
test mint::gate::tests::accept_allowed_with_holdout_carries_overfit_flag ... ok
test mint::holdout::tests::detect_overfit_zero_train_is_undefined ... ok
test mint::gate::tests::mint_refused_with_incomplete_metric_coverage ... ok
test mint::gate::tests::mint_allowed_full_proof_no_overfit ... ok
test mint::gate::tests::mint_blocked_by_unwaived_overfit ... ok
test mint::metrics::tests::unknown_capability_imposes_no_metric_requirement ... ok
test mint::metrics::tests::missing_metrics_reports_gaps_in_registry_order ... ok
test mint::metrics::tests::missing_metrics_empty_when_all_present ... ok
test mint::metrics::tests::is_known_capability_tracks_registry ... ok
test mint::metrics::tests::required_metrics_maps_known_capabilities ... ok
test mint::metrics::tests::trader_and_filter_have_disjoint_nonempty_sets ... ok
test mint::holdout::tests::record_persists_and_computes_overfit ... ok
test mint::holdout::tests::waive_overfit_records_reason ... ok
test mint::holdout::tests::record_replaces_and_resets_waiver ... ok

test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 831 filtered out
```

### Engine — migration registry (`migration_registry_tests`)
```
test api::migration_registry_tests::migrate_holdout_creates_table_idempotently ... ok
test api::migration_registry_tests::migrate_optimization_store_creates_tables_idempotently ... ok
(+ 4 pre-existing) test result: ok. 6 passed; 0 failed
```

### Dashboard — `cargo test -p xvision-dashboard --test mint_holdout`
```
running 8 tests
test mint_refused_without_eval_proof_is_typed ... ok
test overfit_blocks_mint_until_waived ... ok
test swap_agent_is_checkpointed_and_reversible ... ok
test accept_allowed_with_holdout_present ... ok
test accept_without_holdout_allowed_with_override_reason ... ok
test accept_without_holdout_is_refused_typed ... ok
test swap_agent_unknown_role_is_validation_error ... ok
test mint_refused_without_lineage_is_typed ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### dspy-free invariant
```
$ cargo tree -p xvision-dashboard | grep -i dspy || echo clean
clean
```

## What the conductor must wire

The marketplace-mint gate checks **presence** of an eval proof and **coverage** of
the holdout metric battery; it does not produce them. The conductor must wire:

1. **Eval-proof source.** `POST /api/optimizations/:id/mint` takes `eval_run_id` +
   `eval_metric` in the body. The FE mint flow (Phase 4.5) must pass the id of the
   eval run that scored the child agent's strategy. The engine treats it as an
   opaque pointer — a later hardening pass can resolve the run and assert it
   actually scored *this* child (currently presence-only).
2. **Holdout metric values.** The eval/optimizer (CLI side) must call
   `POST /api/optimizations/:id/snapshots/:sid/holdout` with the paired
   train/holdout metric values per snapshot. The values may be stubbed in tests;
   in production they come from the holdout-split eval run.
3. **`metrics_present` at mint.** The mint call passes the metric-name battery the
   holdout proof carries; the conductor should source this from the eval run's
   reported metric set so the per-capability required-set check is meaningful.
4. **FE mint UI (4.5, separate track)** consumes `MintDecision` to stamp
   marketplace metadata; this backend only enforces the refusal barrier.
```
