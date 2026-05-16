# qa9-strategy-wizard-persistence claim

Claimed: 2026-05-14T08:12:45Z
Worktree: `.worktrees/qa9-strategy-wizard-persistence`
Branch: `qa9-strategy-wizard-persistence`

Scope:

- Fix live QA finding where the setup wizard/chat rail claims it updated a
  strategy draft for BTC / 6-hour cadence / risk, while the Strategy Inspector
  manifest still shows the original template values.
- Add a wizard-accessible manifest persistence tool for Inspector-visible
  manifest fields.
- Keep risk preset changes synchronized with `manifest.risk_preset_or_config`.

Verification plan:

- Rust authoring/API/wizard regression tests are added but run only in
  CI/non-deploy environments because `CLAUDE.md` forbids local Cargo on this
  deploy host.
- Run frontend typecheck and static diff checks locally.
