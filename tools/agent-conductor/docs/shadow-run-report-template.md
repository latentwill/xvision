# Shadow-run report — Phase-1 cohort

> Template. Copy to
> `team/archive/agent-cicd-phase-1-shadow/report.md` and fill in
> when the cohort completes. The contract requires this file to
> exist for the verification step to pass.

## Summary

- **Cohort intake:** `team/intake/2026-05-18-agent-cicd-shadow-cohort.md`
- **Mode:** replay (fixture-driven) / live (operator-driven) — choose one
- **Daemon revision:** `<git sha at run time>`
- **Daemon shadow mode confirmed:** ✅ / ❌
- **Mutations attempted during shadow:** `<must be 0>`
- **Total transitions scored:** `<n>`
- **Agreement rate:** `<n/total = nn.n%>` — threshold ≥90%
- **Pass / fail:** ✅ / ❌

## Per-track scoring

For each track in the cohort, one section.

### Track: `<track-slug>`

- Historical PR: `#<n>`
- Merged at: `<utc>`
- Replay window: `<first transition>` → `<last transition>`
- Per-transition table:

| # | from | observed | proposed kind | proposed to | actual kind | actual to | agreement | notes |
|---|---|---|---|---|---|---|---|---|
| 1 |  |  |  |  |  |  |  |  |
| 2 |  |  |  |  |  |  |  |  |
| 3 |  |  |  |  |  |  |  |  |

For any row with `agreement: no` or `partial`, append a one-paragraph
root cause below the table. Not every disagreement is a daemon bug —
intentional operator overrides, race conditions, and missing data
all count. The point is to explain it.

---

(repeat the block per cohort track)

---

## Aggregate transition coverage

Confirm every Phase-1 transition was exercised at least once, and
every Phase-2/3 status was observed as `observe-only` at least once:

| Transition | Times proposed | Times actual | Agreement |
|---|---|---|---|
| READY → CLAIMED (`claim`) |  |  |  |
| CLAIMED → CODING (`begin-coding`) |  |  |  |
| CODING → PR_OPEN (`pr-open`) |  |  |  |
| MERGED → ARCHIVED (`archive`) |  |  |  |
| Any → REVIEWING (`observe-only`) |  |  | n/a (guardrail) |
| Any → CHANGES_REQUESTED (`observe-only`) |  |  | n/a (guardrail) |
| Any → FIXING (`observe-only`) |  |  | n/a (guardrail) |
| Any → APPROVED (`observe-only`) |  |  | n/a (guardrail) |
| Any → MERGE_READY (`observe-only`) |  |  | n/a (guardrail) |
| Any → DEPLOYED (`observe-only`) |  |  | n/a (guardrail) |

## Disagreement triage

List every row above where agreement is `no` or `partial`, with
root-cause classification:

- `daemon-bug`: planner produced the wrong target state or kind given the input.
- `missing-observation`: planner output was right given what it saw, but it didn't see something the operator did.
- `intentional-override`: operator deviated from the rule for a domain reason.
- `race-condition`: the state changed between the daemon read and the action.

Each disagreement maps to one of the four classes. If a class
doesn't appear, omit it. If `daemon-bug` appears, the live flip is
blocked until each bug has a follow-up track filed.

## Flip-to-live checklist

The contract requires this checklist to be signed off before
live mode is enabled. Tick each box once verified.

- [ ] Agreement rate ≥ 90% over the cohort.
- [ ] Zero mutations during shadow (digest confirms `transitions executed: 0`).
- [ ] Every cohort track has a per-transition row filled in.
- [ ] Every disagreement is classified above.
- [ ] No `daemon-bug` disagreements outstanding (or each has a filed track).
- [ ] `team/archive/agent-cicd-phase-1-shadow/digest.md` committed.
- [ ] `team/archive/agent-cicd-phase-1-shadow/final-board.json` committed and validates against `team/schema/board.schema.json`.
- [ ] `launchd/com.xvision.agent-conductor.plist` installed (see `tools/agent-conductor/README.md` for the substitution + `launchctl bootstrap` recipe).
- [ ] `AGENT_CONDUCTOR_SHADOW` unset in the live plist.
- [ ] `AGENT_CONDUCTOR_ENABLE=1` set in the live plist.
- [ ] Operator on standby for the first live cohort.

When every box is ticked, the operator runs the flip — outside this
report — and Phase-1 is live.
