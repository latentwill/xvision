# QA Tracker Parity Import Note — 2026-06-12

This note is intended for the active `QA_TRACKER.md` owner. It should be folded
into the tracker only from that worktree to avoid overwriting in-flight Batch B
changes.

Source addendum:
`docs/research/2026-06-12-live-eval-parity-crosswalk.md`

## Add To `QA_TRACKER.md`

Add this section after `Batch Plan`:

```md
## Live/Eval Parity Gate

Profitability-relevant work cannot move from `not-started` to `in-progress`
until the item has an explicit live/eval parity plan.

Required evidence:

1. Backtest path: the historical eval code path that computes or emits the behavior.
2. Live path: the live-loop, broker, or paper/live path that computes or emits the same behavior.
3. Evidence path: persistence/export fields that let operators compare backtest and live after the run.
4. Operator surface: CLI/UI display, including missing/legacy/excluded states distinct from zero.
5. Parity test: a live parity harness test, or an explicit live exclusion marker with owner and dependency.

If live parity is intentionally deferred, the implementation must persist and
surface an exclusion marker such as `source: "backtest_only"` or
`borrow_cost_excluded: true`; silent omission is not acceptable.
```

Add this column to `Item Status`:

```md
| Live/eval parity |
```

Use one of these values:

| Value | Meaning |
|---|---|
| `required-before-start` | Item cannot start until parity owner/path/test is declared. |
| `covered-by-WS1` | Live-loop parity belongs to PR #948 WS1 / `xvision-914`. |
| `covered-by-WS2` | Metrics/trust parity belongs to PR #948 WS2 / `xvision-gzu`. |
| `covered-by-WS3` | Broker/account/live survivability parity belongs to PR #948 WS3 / `xvision-x6j`. |
| `shared-path-required` | Implementation must use a shared evaluator/snapshot/schema used by both backtest and live. |
| `explicit-exclusion-ok` | Backtest-only or live-only is acceptable only if persisted and surfaced. |
| `not-profitability-relevant` | No parity evidence required; explain briefly in notes. |

## Suggested Initial Column Values

| ID | Live/eval parity | Notes |
|---|---|---|
| PF-01 | `shared-path-required` | Run inspect, eval accounting, tokens, and costs need one source of truth or visible provenance. |
| PF-02 | `covered-by-WS2` | `total_return_pct` should settle on NAV/equity-curve basis before trust UI hardens. |
| PF-03 | `not-profitability-relevant` | Scenario card display bug; still needs CLI/UI verification. |
| PF-04 | `not-profitability-relevant` | Provider resolution affects execution reliability, not backtest/live semantics. |
| PF-05 | `shared-path-required` | Human eval output should read the same token fields used by export/API. |
| PF-06 | `shared-path-required` | Filter-fire stats must share event semantics with replay/backtest/live. |
| PF-07 | `shared-path-required` | Cost estimate provenance must be visible if estimate is missing or incomplete. |
| PF-08 | `explicit-exclusion-ok` | Classified regime can be eval-only initially if CLI surfaces that scope. |
| PF-09 | `covered-by-WS2` | Trade-count/liquidation semantics feed the trust frame. |
| PF-10 | `not-profitability-relevant` | CLI envelope consistency only. |
| PF-11 | `not-profitability-relevant` | Catalog filtering only. |
| PF-12 | `shared-path-required` | `fire.reason`/trigger context must appear in both seeds and event/export paths. |
| PF-13 | `not-profitability-relevant` | Lifecycle/status docs unless it changes eval behavior. |
| PF-14 | `not-profitability-relevant` | Profile catalog fix. |
| PF-15 | `not-profitability-relevant` | Provider catalog ergonomics. |
| PF-16 | `explicit-exclusion-ok` | Risk setter is authoring-only unless it adds new sizing semantics. |
| PF-17 | `covered-by-WS1` | Backtest fix is not sufficient; live SL/TP parity must land in WS1. |
| PF-18 | `shared-path-required` | Schema/repair/prompt fields must match between backtest and live briefing. |
| PF-19 | `shared-path-required` | Bracket persistence/export must cover backtest and live decisions. |
| PF-20 | `shared-path-required` | Strategy-level take-profit config must feed the same effective-bracket path. |
| CAP-941 | `required-before-start` | Position tokens/manage block must share a position snapshot across backtest/live. |
| CAP-942 | `shared-path-required` | Replay must call the same filter evaluator as eval and match event shape. |
| CAP-943 | `covered-by-WS3` | Risk-at-stop sizing intersects account-equity sync and live sizing. |
| CAP-944 | `shared-path-required` | Token math must live in the shared evaluator; live warmup gaps must be explicit. |
| CAP-945 | `shared-path-required` | Trigger context and branch attribution should be shared seed/event/export plumbing. |
| UF-01 | `required-before-start` | Depends on CAP-945 branch attribution. |
| UF-02 | `explicit-exclusion-ok` | Strategy family can be eval-only if called out; live short borrow/funding gaps matter later. |
| UF-03 | `covered-by-WS2` | No-LLM baseline should wait for metric basis/CI settlement. |
| UF-04 | `required-before-start` | Partial close requires realized exits and live parity first. |
| UF-05 | `required-before-start` | Filter-aware memory depends on trigger context and outcome persistence. |
| UF-06 | `shared-path-required` | Eval validate should use CAP-942 replay semantics. |
| UF-07 | `required-before-start` | Loss-pause tokens need realized PnL/live position accounting. |
| UF-08 | `required-before-start` | Win/loss cooldown asymmetry needs realized exits and manage-block parity. |

## Start Criteria For #945

#945 is the preferred first low-collision implementation after the tracker import.
Before starting it, confirm:

- Batch B is not editing `xvision_filters` trigger-context code.
- WS1 is not depending on a different live seed shape.
- The implementation plan names both seed consumers: backtest trader briefing and live trader briefing.
- Acceptance includes event/export propagation, not only model prompt context.
