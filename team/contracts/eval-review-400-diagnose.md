---
track: eval-review-400-diagnose
lane: integration
wave: qa-operator-2026-05-19
worktree: .worktrees/eval-review-400-diagnose
branch: task/eval-review-400-diagnose
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/routes/eval/review.rs
  - crates/xvision-dashboard/src/routes/eval/mod.rs
  - crates/xvision-dashboard/src/error.rs
  - crates/xvision-dashboard/tests/eval_review_route.rs
  - frontend/web/src/features/eval-runs/review/ReviewPanel.tsx
  - frontend/web/src/features/eval-runs/review/ReviewPanel.test.tsx
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/review/**
  - frontend/web/src/features/eval-runs/review/AgentPicker.tsx
  - frontend/web/src/features/eval-runs/review/ReviewContent.tsx
  - frontend/web/src/features/eval-runs/review/FindingCard.tsx
  - frontend/web/src/features/eval-runs/review/VerdictBadge.tsx
interfaces_used:
  - DashboardError → 400 response shape
  - eval review request body (`{ agent_profile_id, force }`)
  - ApiError surfacing in frontend mutation layer (per #256)
verification:
  - cargo test -p xvision-dashboard --test eval_review_route
  - cargo test -p xvision-dashboard
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run ReviewPanel
  - pnpm --dir frontend/web build
acceptance:
  - **Phase 1 — investigation note.** Status note at
    `team/status/eval-review-400-diagnose.md` documents:
    - The exact JSON body of the 400 response on the operator's run
      (`01KRXY73XAE2NR65YVKJZ28JBK`). Capture via
      `curl -i -X POST .../api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK/review \
        -H 'content-type: application/json' \
        -d '{"agent_profile_id":"<id>","force":true}' | jq` against the
      live `xvn.tail2bb69.ts.net` deployment.
    - Which Validation branch in `crates/xvision-dashboard/src/routes/eval/review.rs`
      fires. The current branches as of intake: line 109 (agent profile
      disabled, dashboard-side), line 259 (agent profile disabled,
      engine-side), line 261 (`RunNotCompleted` — should not fire on a
      completed run), line 296 (load config failed), line 338
      (skip-with-remediation; provider-not-configured path that #256
      added). Identify which one (or a different one not in this list)
      is the source.
    - Whether the failure is (a) a branch that lacks the remediation
      hook #256 added, (b) the frontend dropping a body that does
      carry useful info, or (c) a different ApiError mapped to 400
      with an empty body. The status note picks one and shows the
      evidence.
  - **Phase 2 — fix.** Based on the diagnosis:
    - If (a): add the remediation hook to the offending branch,
      mirroring #256's pattern. Add a regression test that hits the
      branch and asserts the response body carries `error.code` and
      `error.message`.
    - If (b): patch `ReviewPanel.tsx` to surface `error.code` and
      `error.message` from the response body, not just
      `error.message ?? String(error)`. Add a component test that
      simulates a 400 response with a structured body and asserts the
      UI renders the structured message.
    - If (c): identify the `DashboardError` variant being mapped, give
      it a serialization path that carries operator-readable info.
      Regression test on the error-shape contract.
  - **Phase 3 — acceptance.** Clicking "Review with: <agent>" on a
    completed run either succeeds OR shows an operator-actionable
    error message — never the silent 400 the operator sees today.
    Verified by:
    - A passing integration test that exercises the operator's exact
      request shape and asserts a useful response.
    - A manual re-test against the live tailnet run id, captured in
      the status note as a curl-output paste post-fix.
  - No `try/catch` silencing (`feedback_alpha_root_cause`).
  - If the root cause turns out to be in a forbidden path
    (e.g., `crates/xvision-engine/src/review/**`), the status note
    documents the finding and the contract is updated (NOT freelanced)
    before any code lands.
parallel_safe: false
parallel_conflicts:
  - "frontend/web/src/features/eval-runs/review/ReviewPanel.tsx: single-writer for this track. If anyone else needs to edit it, coordinate via team/queue/."
---

# Scope

Operator hit a silent `POST /api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK/review
→ 400 Bad Request` on a completed eval run on 2026-05-19. The frontend
surfaces no actionable error; the operator-visible state is "click does
nothing". The previous QA wave shipped PR #256
(`qa-review-agent-provider-config`) which made provider misconfiguration
on the review agent surface as a classified remediation error, not a
raw 400 — so this 400 today is either a different validation branch
firing, a regression in #256, or a different `ApiError` shape mapped
to 400 with an empty body.

This is **investigation-first, fix-second**. The contract gates Phase 2
(patch) on a Phase 1 status note that documents the exact branch /
serialization path responsible. Do not freelance a fix without the
evidence — that's what the previous wave already did and the issue
came back.

Anchor reading:

- `team/intake/2026-05-19-qa-operator-round-4.md` item 3.
- `team/archive/2026-05-18-qa-rounds/contracts/qa-review-agent-provider-config.md`
  for the #256 pattern this track must extend or match.
- `crates/xvision-dashboard/src/routes/eval/review.rs` (808 lines) —
  the suspect branches are documented at the line numbers in the
  acceptance section.
- `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx:52` for
  the request body shape (`{ agent_profile_id, force: true }`).

# Out of scope

- Review-engine refactors (multi-profile review, fan-out, V2
  user-configurable review agent on `team/board-v2.md`).
- Re-architecting the review request body or response envelope.
- Frontend redesign of the review panel beyond the error-surfacing
  patch.
- Other 400-returning endpoints — this track is scoped to the
  review route's 400 only.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-review-400-diagnose status
git -C .worktrees/eval-review-400-diagnose log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-review-400-diagnose \
  -b task/eval-review-400-diagnose origin/main
```

# Notes

Append checkpoints / PR links below. The status note's Phase 1
investigation is acceptance-bearing — do not collapse it into the
PR description.
