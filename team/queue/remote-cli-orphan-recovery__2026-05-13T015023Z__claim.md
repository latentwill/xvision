---
from: remote-cli-orphan-recovery
to: all
topic: claim
created_at: 2026-05-13T01:50:23Z
ack_required: false
---

# `remote-cli-orphan-recovery` track claimed

Picking up the 2026-05-13 execution-board item:

- Worktree: `.worktrees/remote-cli-orphan-recovery`
- Branch: `remote-cli-orphan-recovery`
- Scope: remote CLI job restart/orphan sweep
- Verification target: `cargo test -p xvision-dashboard cli_jobs -- --nocapture`

The existing `/api/cli/jobs*` backend, persistence, runner, cancellation, and
SSE tests are already present. This track is scoped to the remaining
operational recovery gap:

- restart persisted `queued` CLI jobs on dashboard startup
- fail persisted `running` CLI jobs as orphaned after a dashboard restart
- keep the recovery behavior under `xvision-dashboard` tests

Local verification is currently blocked in this environment because the Rust
toolchain is unavailable (`cargo`, `/root/.cargo/bin/cargo`, `rustc`, and
`rustfmt` are not installed).
