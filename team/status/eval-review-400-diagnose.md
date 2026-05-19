# status: eval-review-400-diagnose

**phase:** Phase 1 complete, Phase 2 landed, Phase 3 verified — ready for PR
**claimed at:** 2026-05-19
**worker:** Claude Opus 4.7 (operator: Ed)
**worktree:** `.worktrees/eval-review-400-diagnose`
**branch:** `task/eval-review-400-diagnose`

## Phase 1 — investigation (acceptance-bearing)

### Reproduction (live tailnet)

Command:

```bash
curl -isS -X POST 'https://xvn.tail2bb69.ts.net/api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK/review' \
  -H 'content-type: application/json' \
  --max-time 30 \
  -d '{"agent_profile_id":"reasoning-agent","force":true}'
```

Response (captured 2026-05-19, full headers + body):

```
HTTP/2 400
content-type: application/json
date: Tue, 19 May 2026 03:16:18 GMT
vary: accept-encoding

{"code":"validation","message":"request: review skipped: agent profile `reasoning-agent` requires provider `anthropic` which is not configured in Settings → Providers (configured: openrouter). Add a compatible provider to run this review."}
```

Saved at `/tmp/eval-review-repro.txt` on the agent's workstation.

### Which Validation branch fires

`crates/xvision-dashboard/src/routes/eval/review.rs:338`, the
**skip-with-remediation path that #256 added** in
`build_dispatch_for_profile`. The resolver finds no provider named
`anthropic` in the runtime config; same-kind substitution fails because
the only configured provider is `openrouter` (kind `OpenaiCompat`,
not `Anthropic`); the resolver then returns
`DashboardError::from(ApiError::Validation(format!(...)))` which
serializes through `DashboardError::Validation { field: "request", msg }`
in `crates/xvision-dashboard/src/error.rs:25-29,41-43` as
`{"code":"validation","message":"request: review skipped: ..."}` with
HTTP status 400.

The other suspect branches did NOT fire:

- L109 (`profile.enabled == false`, dashboard-side disabled check):
  did not fire — the seeded `reasoning-agent` profile from migration
  016 is `enabled = true`.
- L259 (`ReviewError::ProfileDisabled` engine-side): did not fire —
  resolution failed before the engine was called.
- L261 (`ReviewError::RunNotCompleted`): did not fire — the run is
  `status = "completed"` (verified via
  `GET /api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK`).
- L296 (`config::load_runtime` failed): did not fire — the runtime
  config loads fine and contains an `openrouter` entry.

### Classification: (a), (b), or (c)?

This is **path (b)**: the BACKEND is already serializing an
operator-actionable error body (the #256 `review skipped: ...`
remediation copy that names the missing provider and tells the
operator to add one in Settings → Providers). The structured response
body carries both `code: "validation"` and a usable, multi-sentence
`message`. The HTTP status is 400 with a non-empty JSON body — there
is no missing remediation hook (path a) and no empty/silent error
serialization (path c).

The FRONTEND is the layer that's letting the operator down. Evidence:

- `frontend/web/src/api/client.ts:65-83` correctly parses the body
  as `{ code, message }` and throws an `ApiError(status, code,
  message)`. So the structured info reaches React land.
- `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx:91-97`
  renders the error like this:

  ```tsx
  {generate.isError && (
    <div className="text-danger text-[12px] mt-2">
      {generate.error instanceof Error
        ? generate.error.message
        : String(generate.error)}
    </div>
  )}
  ```

  Issues:
  1. The `error.code` discriminator is dropped on the floor. The
     operator never sees that this is a "validation" error vs an
     "internal" error vs a "not_found" — same visual treatment for
     all classes.
  2. The remediation copy starts with the literal prefix
     `"request: "` (from `DashboardError::Validation { field:
     "request", msg }` formatting in `error.rs:42`), which leaks
     server-side jargon into the operator's view.
  3. The rendering is small (`text-[12px]`), has no `role="alert"`,
     no border or background to anchor the eye, and sits below the
     agent picker pills — easy to miss after a click. Consistent
     with the operator's "click does nothing" report.
  4. No retry affordance and no link/CTA to Settings → Providers
     (where the operator can actually fix this).

The fix path is therefore (b): patch `ReviewPanel.tsx` to surface
both `error.code` and `error.message` in a visible, role-alerted
container, strip the `request: ` prefix when the code is
`validation`, and add a regression test covering the structured-body
case.

### Why this slipped past #256

PR #256 ensured the backend stopped silently 500'ing on a missing
provider and started emitting a structured 400 with remediation copy.
It did not audit the frontend's existing rendering of that error;
the panel's tiny red-text mutation-error block was already there
before #256 and didn't get touched. The operator's "click does
nothing" report on 2026-05-19 reflects the fact that a 12px line of
red text near a pill picker is easy to miss compared to the previous
silent-failure baseline (which, unfortunately, looks identical from
the operator's perspective).

## Phase 2 plan (forward look)

- Patch `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx`
  to render a proper `role="alert"` error block that shows
  `error.code` (as a small badge / tag) alongside `error.message`,
  strips the `request: ` server-side prefix when `code === "validation"`,
  and uses the same visual weight as the existing list/detail error
  alerts already in this panel (lines 116-133, 156-173).
- Add `frontend/web/src/features/eval-runs/review/ReviewPanel.test.tsx`
  with at minimum:
  - 400 with `{code: "validation", message: "request: review skipped: ..."}`
    body → assert the structured copy renders without the `request: `
    prefix and `code` is visible.
  - 500 with `{code: "internal", message: "internal error"}` → assert
    the internal-error path also renders.
- No backend changes needed; this is a frontend-only fix.

## Phase 3 plan

- `cargo test -p xvision-dashboard` (sanity — no backend touch but
  exercise the existing review-route test matrix is in
  `crates/xvision-dashboard/src/routes/eval/review.rs`'s inline
  `#[cfg(test)] mod tests`; the contract's
  `crates/xvision-dashboard/tests/eval_review_route.rs` integration
  test does not exist yet, will be created if a backend change is
  required — but for path (b) only the frontend changes).
- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web test -- --run ReviewPanel`
- `pnpm --dir frontend/web build`
- Re-curl after deploy (or via local dev) and paste the unchanged
  backend response + the post-fix screenshot/HTML snippet into this
  note before merging.

## Decisions

- Stay scoped to `allowed_paths`. The diagnosis lands entirely in
  `ReviewPanel.tsx` + a new co-located test file, both inside
  the contract's allow-list. No `forbidden_paths` touched. No queue
  note required.
- No `try/catch` silencing — the patch surfaces the error more
  loudly, never less.

## Phase 2 — fix landed

Patch in `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx`:

- Added a `GenerateErrorAlert` component that renders the
  `generate.mutate` failure in a `role="alert"` block matching the
  existing list/detail error styling already in the panel.
- New `describeReviewError(error)` helper unwraps `ApiError` into
  `{ code, message }` and strips the server-side `"request: "` prefix
  from `code === "validation"` messages so the operator-facing copy
  starts with the actual remediation sentence.
- Both `error.code` (rendered as an uppercase tag, e.g. `VALIDATION`)
  and `error.message` are now visible — previously only the message
  was rendered and the code was dropped.
- Added a `retry` button bound to the last `generate.variables`
  agent profile id (re-runs the same mutation; no-op if the operator
  never picked an agent).
- Both the alert component and the helper are exported so the test
  file can exercise the helper in isolation as well as via the full
  panel.

Test in `frontend/web/src/features/eval-runs/review/ReviewPanel.test.tsx`
(new file inside the contract's `allowed_paths`):

- `GenerateErrorAlert` unit tests cover the validation-prefix strip,
  the non-validation pass-through, the plain-`Error` fallback,
  `role="alert"` presence, and retry button wiring (5 cases).
- A panel-level integration test mocks `fetch` to return the exact
  `POST /api/eval/runs/:id/review → 400 { code, message }` body the
  operator hit on 2026-05-19, then asserts:
  - the `validation` code badge renders;
  - the cleaned message (no `request: ` prefix) renders;
  - the `Settings → Providers` remediation CTA copy renders;
  - the alert is reachable via `role="alert"`.

## Phase 3 — verification

Backend response is unchanged by the fix; verified by re-running the
same curl post-patch (path is frontend-only):

```
HTTP/2 400
content-type: application/json
date: Tue, 19 May 2026 03:22:30 GMT
vary: accept-encoding

{"code":"validation","message":"request: review skipped: agent profile `reasoning-agent` requires provider `anthropic` which is not configured in Settings → Providers (configured: openrouter). Add a compatible provider to run this review."}
```

- `pnpm --dir frontend/web typecheck` → **passes** (clean tsc -b).
- `pnpm --dir frontend/web test -- --run ReviewPanel` → **6/6 passed**.
- `pnpm --dir frontend/web build` → **passes** (Vite bundle built into
  `crates/xvision-dashboard/static/`).
- `cargo test -p xvision-dashboard --lib routes::eval::review` →
  **13/13 passed** (no review-route regressions; this fix doesn't
  touch the backend route, but the existing #256 regression coverage
  was re-run to confirm).
- Full `cargo test -p xvision-dashboard` shows 4 pre-existing failures
  on `tests/http.rs` (scenario CRUD + eval-compare seeded run) that
  reproduce on `origin/main` and are unrelated to this contract.
  Documented here so reviewers don't chase them as a regression.

## Operator follow-up (not in scope of this contract)

The backend's `DashboardError::Validation { field: "request", msg }`
format prefixes every validation error with `"request: "` (see
`crates/xvision-dashboard/src/error.rs:42`). The frontend strips it
locally for the review-error path, but other validation alerts across
the dashboard still inherit the prefix. A future track should either
drop the `field` formatting at the response layer or update other
alert sites to strip it the same way. Not blocking; flagged so the
inconsistency doesn't surprise the next QA round.
