---
track: eval-reviews-auto-runner
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-reviews-auto-runner
branch: task/eval-reviews-auto-runner
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/review/**                # existing review-related code (already a module)
  - crates/xvision-engine/src/eval/postprocess.rs           # findings postprocess is the natural sibling
  - crates/xvision-engine/src/api/eval.rs                   # expose runner + trigger
  - crates/xvision-engine/src/eval/store.rs                 # eval_reviews read/write helpers
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**         # schema already exists (016_eval_reviews + 017_eval_findings_review_columns)
  - frontend/web/**
  - crates/xvision-engine/src/eval/executor/**  # this is a post-run workflow, not an executor change
interfaces_used:
  - xvision-engine::eval::store::list_findings_for_run / write_review
  - xvision-engine::eval::postprocess (findings-postprocess provider routing, qa-round-5 F-5, merged in PR #316)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::review
  - cargo test -p xvision-engine eval::postprocess
acceptance:
  - A new `eval::review::auto_runner` (or extension of an existing module if one is closer) consumes the `eval_findings` rows for a completed run and writes a single `eval_reviews` row with:
    * `verdict` ∈ `promising | weak | failed | inconclusive` — derived from `eval_findings.severity` counts (rule: any `critical` → `failed`; ≥2 `warning` and no `critical` → `weak`; only `info` → `promising`; no findings → `inconclusive`).
    * `score` ∈ 0..=100 — a coarse mapping from the verdict and severity counts (e.g. `failed → 0..=25`, `weak → 25..=50`, `promising → 75..=100`, `inconclusive → 50`). Tunable constants in the module.
    * `summary` — short string (≤ 240 chars) listing the top 3 findings by severity, kind, and the first sentence of `eval_findings.summary`.
    * `raw_output_json` — JSON serialization of the source findings used as the audit record.
    * `agent_profile_id` — pass through whichever profile was in scope at run time (look at the existing `eval_reviews.agent_profile_id` FK target — if there's no clean accessor at the seam, accept `NULL` and document).
  - The runner is invoked at eval-finalize time (success path) — find the success branch via `rg 'status = .completed.\|finalize_run\|complete_run' crates/xvision-engine/src/eval`. Failure path does NOT write an eval_review (consistent with the existing `eval_findings` postprocess gate).
  - Runner is **best-effort**: failures (DB errors, missing findings) log at `warn` and never fail the run. The existing pattern `"findings postprocess failed (run still ok)"` from PR #316 is the model.
  - The runner does NOT call the LLM. This is rule-based scoring over already-extracted findings; the LLM-based `extract_findings` (PR #316 fixed its provider routing) writes the source rows.
  - Tests:
    * Unit: verdict mapping for each `(severity-count-vector)` → expected verdict.
    * Unit: score mapping is monotone w.r.t. severity counts.
    * Integration: insert findings for a fake run and trigger the runner; assert exactly one `eval_reviews` row exists with the expected verdict.
    * Integration: two invocations on the same run are idempotent (UPSERT or pre-existence guard — pick one and document).
  - Audit acceptance: the wave's 88 `eval_findings` rows finally produce `eval_reviews` rows on next eval finalize.
---

# Scope

Intake F-11 (sub-bullet: eval_reviews runner) of
`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

`eval_reviews` schema is fully defined (migrations 016 + 017) with
`verdict`, `score`, `summary`, `raw_output_json`, but `eval_reviews`
table is **empty** in the audit's DB snapshot. Nothing populates it.
This contract adds the rule-based auto-runner.

The blocker that previously made this a dead path was the
`extract_findings` provider routing issue (qa-round-5 F-5 / PR #316,
`findings-postprocess-provider-routing`). That's merged, so findings
now reliably get extracted — making a downstream reviews runner viable.

# Out of scope

- LLM-based review summarization (this is rule-based; an LLM pass over
  the findings can be a future track).
- Backfilling `eval_reviews` for the audit's existing 56 runs (next eval
  finalize is sufficient).
- Frontend rendering of the new `eval_reviews` rows (the dashboard
  already has the schema in its DTOs from the same earlier wave).
- Migration changes — `eval_reviews` and `eval_findings.eval_review_id`
  already exist.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-reviews-auto-runner status
git -C .worktrees/eval-reviews-auto-runner log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-reviews-auto-runner -b task/eval-reviews-auto-runner origin/main
```

# Notes

Keep `verdict` / `score` constants tunable in the module (one place to
adjust). The dashboard team can iterate on thresholds without touching
this file's logic.
