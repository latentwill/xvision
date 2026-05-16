# subagent-mcp-runtime

Status: needs platform/runtime diagnosis

## Observed behavior

- MCP-backed subagents can remain reported as "booting MCP" or otherwise fail
  to deliver completion status promptly.
- While that happens, additional subagent spawns may be blocked by the runtime
  agent/thread limit even though repo worktrees and board claims are fine.
- In the 2026-05-13 recovery pass, the final worker had an active patch and
  eventually committed successfully, but `wait_agent` did not report completion
  before the parent turn was interrupted.

## What this is not

- This is not caused by `team/execution-board-2026-05-13.md`.
- This is not caused by `team/queue/*__claim.md` files.
- The board/claim/status files are plain git coordination artifacts.

## Working policy until fixed

- Keep board claims and per-track status files.
- Use one worktree per track.
- Avoid relying on long `wait_agent` calls as the only source of truth.
- If a subagent appears stuck, inspect its worktree status and status file
  before assuming the track is idle.
- Close completed subagent sessions promptly so the runtime thread limit does
  not block future spawns.
