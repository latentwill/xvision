# XVN Scaper - Final Incident Note

Date: 2026-06-01
Linked raw audit log: `docs/audits/2026-06-01-xvn-scaper-worklog.txt`
PR: `https://github.com/latentwill/xvision/pull/709`

## What was verified

The raw audit was checked against the live strategy/scenario/eval identifiers that were captured during the strategy work, plus the current `xvn-app` state on the dev server.

### Exact identifiers

- Strategy ID: `01KT1HQ47W91ZQS0K4GBSF6B72`
- Trader agent ID referenced by the failed eval: `01KT1JJRF6YNMFKY0WRYNHEASY`
- Scenario ID referenced by the failed eval: `sc_01KT1N64PW8G3HXF7F4YRFZDKP`
- Earlier scenario ID preserved in the raw worklog: `sc_01KT1HFCZVB3A7950WKPVQPQSH`

The raw worklog preserves an earlier scenario ID, while the final failed eval references a later scenario ID. Treat both IDs as part of the incident trail instead of assuming there was only one scenario object.

### Exact log points used for comparison

- Scenario creation completed at `2026-06-01T13:15:20.156702386Z`.
- Eval attempt ran with:
  - strategy `01KT1HQ47W91ZQS0K4GBSF6B72`
  - scenario `sc_01KT1N64PW8G3HXF7F4YRFZDKP`
  - mode `paper` requested, but the runtime reported `mode=backtest`
- The run failed with:
  - `eval run: not found: agent 01KT1JJRF6YNMFKY0WRYNHEASY`

### App-side findings from `xvn-app`

- The dev server app container was healthy when checked.
- The app database had strategies and providers, but no eval runs were recorded for the incident path.
- Repeated app warnings showed strategy summaries being skipped because referenced `AgentRef` records were missing.
- A representative live strategy had an `agents[]` entry, but the referenced agent record was absent from the workspace agent library.
- `xvn strategy diagnostics <strategy-id> --json` correctly reported `launchable=false` for a missing model/agent binding.
- `xvn strategy validate <strategy-id>` and `xvn eval validate --strategy <strategy-id> --scenario <scenario-id> --json` were weaker than diagnostics and could report success even when the strategy was not actually launchable.

## Comparison result

The audit trail and app-side evidence agree on the core failure mode: the strategy object existed and the eval request referenced a scenario, but the runtime could not resolve the trader agent ID required by the strategy.

The remaining blocker is therefore not log preservation. It is a strategy-agent registration and validation gap: the strategy could reach eval launch with a dangling agent reference that should have been caught by preflight validation.

## Incident summary

The raw audit file is preserved in this PR. It documents the agent's CLI/API probing, the strategy/scenario attempts, and the eventual eval failure.

The strategy is not eval-ready until its trader agent reference resolves to a real workspace agent record, or until the strategy is intentionally switched into a no-agent mechanical execution mode that does not require a trader agent.

## Follow-up

1. Reconcile where the eval runtime expects the trader agent to be registered, then retry the eval using the exact IDs above.
2. Make `xvn strategy validate` fail when a strategy references a missing agent.
3. Make `xvn eval validate` run the same launch-readiness checks as `xvn strategy diagnostics` before it returns success.
4. Add explicit CLI/UI language for no-agent mechanical execution so missing-agent errors are not confused with intentional rules-only mode.
5. Update agent-facing docs and skills so agents learn the safe path: inspect provider readiness, run diagnostics, validate the scenario, then launch eval.
