# qa-retention-prompt-storage-bug — status

**Owner:** worker (claude session, 2026-05-18)
**Branch:** `task/qa-retention-prompt-storage-bug`
**Worktree:** `.worktrees/qa-retention-prompt-storage-bug`
**Base:** `origin/main` (9ea5361 post-sweep)

## Snapshot

Root-cause investigation done. **Producer-side fix is out of scope**
for this contract — files belong to other tracks. Shipped the
in-scope frontend correctness fix + filed a queue handoff for the
producer wire-in.

## Investigation — where does the prompt drop?

Traced the prompt payload end-to-end: emit → redactor → sqlite write →
dashboard fetch route → SpanInspector render. Found the gap at the
producer boundary, **not** in any of the layers the contract scoped
me to fix.

### What I expected to find
One of (a) redactor mode-gating, (b) sqlite column projection,
(c) dashboard read route mismatch, (d) SpanInspector keyed on the
wrong field.

### What's actually broken
**Nothing writes prompt or response bodies to the blob store
anywhere in production code.** Specifically:

- `crates/xvision-engine/src/agent/observability.rs:255` hardcodes
  `prompt_payload_ref: None` and `response_payload_ref: None` when
  publishing `ModelCallFinishedEvent`.
- `BlobStore::write` is called only by tests in
  `crates/xvision-observability/src/blobs.rs:114,124,147,158`. No
  production caller.
- `crates/xvision-observability/src/sqlite.rs:167-189` persists
  whatever `prompt_payload_ref` and `response_payload_ref` the event
  carries — both `None`. The schema is correct; the write columns
  receive null because the events themselves carry null.
- `crates/xvision-observability/src/redactor.rs` is a stateless
  secret-pattern scrubber (looks for API keys, JWTs, mnemonics);
  it doesn't gate prompt vs response storage. It would scrub a
  body only if someone called it on one — and nobody does, because
  bodies aren't stored.

### Why the operator sees responses but not prompts
- **Responses** stream live via
  `crates/xvision-engine/src/agent/observability.rs::emit_assistant_text_delta`,
  which publishes `AssistantTextDelta` events carrying chunked text.
  The frontend's `bodiesBySpan` accumulator (`stores/trace-dock.ts`)
  reconstructs the full body and `SpanInspector`'s streaming /
  post-hoc fallback renders it. So responses render even though
  `response_payload_ref` is null.
- **Prompts** have no analogous live-stream path. There's no
  `prompt_text_delta` event. The only render paths are
  `span.prompt` (literal string, used by fixtures only) or
  `span.prompt_payload_ref` (always null). Falling through both,
  `SpanInspector` rendered the misleading "hash-only retention —
  prompt body not stored on disk" placeholder even when retention
  is configured for `full_debug`.

The "asymmetry" the operator named is real but is **not** a
retention bug. It's a producer-write gap.

## What landed in this PR

Per the contract's last acceptance bullet ("If stale rows in the
local sqlite were written under the old `hash_only` default and
cannot be retroactively backfilled, the status note documents this
AND surfaces a one-line operator notice ... in the SpanInspector
fallback path. Do not silently render empty.") — covered, plus a
generalization to **all** rows since the producer-write gap means
every row lacks payload refs today, not just stale ones.

### Frontend (`frontend/web/src/features/agent-runs/SpanInspector.tsx`)

- New exported helper `payloadPlaceholderReason(retentionMode, kind)`
  returns operator-readable copy keyed on retention mode:
  - `full_debug`: `"{prompt|completion} body not captured for this run — re-run to capture"`
  - `redacted`: `"redacted retention — {prompt|completion} body suppressed"`
  - `hash_only`: `"hash-only retention — {prompt|completion} body not stored on disk"` (historical copy)
  - undefined / cache miss: `"{prompt|completion} body not stored on disk"` (neutral fallback)
- Both the PROMPT and RESPONSE placeholder branches now route through
  the helper. The inspector reads `retention_mode` off the active
  run's cached `AgentRunDetail` via `useQueryClient` so it doesn't
  need a prop wiring change in the (out-of-scope) `TraceDock.tsx`
  or `routes/agent-runs-detail.tsx` callers.
- Backwards-compat alias `promptPlaceholderReason` retained.
- Added `data-testid="span-inspector-{prompt|response}-placeholder{,-reason}"`
  to stabilise the test surface.

### Tests (`frontend/web/src/features/agent-runs/SpanInspector.test.tsx`)

- New `render` helper wraps every render in a `QueryClientProvider`
  with an optional `{ activeRunId, retentionMode }` seed for the
  cache.
- 4 new unit tests on `promptPlaceholderReason` covering every
  retention mode + undefined fallback.
- 4 new component tests on the placeholder copy per retention mode:
  `hash_only`, `full_debug` (operator repro), `redacted`, undefined.
- Updated 3 pre-existing assertions on the response placeholder to
  the new testid-based selector (more stable than string matching
  the retention-aware copy).
- Existing 17 tests still pass: 25/25 in this file, 120/120 across
  all `features/agent-runs/` + `lib/format.test.ts`.

### Queue handoff

`team/queue/qa-retention-prompt-storage-bug__20260518T052422Z__producer-never-writes-payload-refs.md`
documents the producer-side gap with file:line citations and a
suggested fix path. Routes to the harness wave (`harness-prompt-
hash-real-digest` track) which already owns `agent/observability.rs`
and is currently GATED on the operator's image build.

## Why the contract scope can't fix the root cause

Contract `allowed_paths`:
```
crates/xvision-observability/src/redactor.rs
crates/xvision-observability/src/sqlite.rs
crates/xvision-observability/src/export.rs
crates/xvision-observability/tests/**
crates/xvision-dashboard/src/routes/agent_runs.rs
crates/xvision-dashboard/tests/agent_runs_blob_route.rs
frontend/web/src/features/agent-runs/SpanInspector.tsx
frontend/web/src/features/agent-runs/SpanInspector.test.tsx
```

The producer files (`crates/xvision-engine/src/agent/observability.rs`,
`crates/xvision-engine/src/agent/execute.rs`) are **not** in this
contract's allowed_paths. They're owned by `harness-prompt-hash-
real-digest` (per `team/OWNERSHIP.md` after the 2026-05-18 conductor
sweep). The harness wave is **GATED** by the operator until an
image build ships.

Routing the wire-in there (rather than expanding this contract's
scope) keeps the merge order sane: the harness wave can land the
producer-side payload write + the real prompt_hash digest in the
same change, since both touch the same emit point.

## Verification

```
pnpm --dir frontend/web typecheck                      # clean
pnpm --dir frontend/web test -- src/features/agent-runs # 120/120 pass (14 files)
pnpm --dir frontend/web build                          # clean
```

No Rust changes in this PR — the observability + dashboard test
suites were not touched.

## Operator-visible impact

Before:
- Trace dock prompt cell on a `full_debug` run: `hash-only retention
  — prompt body not stored on disk` (false; retention is full_debug).

After:
- Trace dock prompt cell on a `full_debug` run: `prompt body not
  captured for this run — re-run to capture` (honest about the
  pre-fix state; remediation pointer).
- Trace dock prompt cell on a `hash_only` run: `hash-only retention —
  prompt body not stored on disk` (unchanged for correct case).
- Trace dock prompt cell on a `redacted` run: `redacted retention —
  prompt body suppressed` (new, matches what the operator chose).

When the producer-side fix ships from the harness wave, new runs
under `full_debug` will populate `prompt_payload_ref` and the
`PayloadRefDetails` blob-preview UI takes over automatically — no
further frontend change needed.

## Open follow-ups

- Producer wire-in: see queue note. Should land via the harness
  wave once ungated.
- Optional polish: once the producer fix lands, the `full_debug`
  copy ("re-run to capture") can be narrowed to only fire on rows
  with a pre-fix timestamp. Defer until the producer fix ships;
  not worth a date-comparison branch on the frontend before then.
