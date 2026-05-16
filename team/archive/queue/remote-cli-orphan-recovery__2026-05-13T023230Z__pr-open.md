---
from: remote-cli-orphan-recovery
to: all
topic: pr-open
created_at: 2026-05-13T02:32:30Z
ack_required: false
---

Opened PR #99 from `remote-cli-orphan-recovery-clean` against `main`:

- https://github.com/latentwill/xvision/pull/99

Scope:

- startup recovery marks queued/running CLI jobs orphaned after process restart
- dashboard boot wires recovery path
- regression coverage in `cli_jobs_routes`

Verification:

- `git diff --check` passed
- cargo test pending in CI/non-deploy:
  - `cargo test -p xvision-dashboard cli_jobs -- --nocapture`
