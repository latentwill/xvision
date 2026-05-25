# Evidence ledger — chat rail, DSPy, strategy agents

Wave plan: `docs/superpowers/specs/2026-05-24-chat-rail-and-strategy-agents-evaluation.md`
Branch: `feat/chat-rail-dspy-strategy-agents` (based on `feat/cline-runtime-unification`)
Opened: 2026-05-24

This ledger is the proof-of-work surface for the wave. Every task closes with
evidence dropped into the matching subdirectory. Per the plan's non-negotiable
gates, a task is not done because it compiles or because an agent says so — it
is done when the evidence below exists and is honest about what is automated,
what is manual, and which live-provider proof is intentionally opt-in.

## Layout

```text
chat-rail/        Phase 1 + 2 evidence (unified stream, Research/Act, checkpoints)
dspy/             Phase 3 evidence (dependency spike, offline optimizer, lineage)
strategy-agents/  Phase 4 evidence (capability diagnostics, tune/mint, holdout)
final/            Phase 5 release gate (surface matrix, commands run, risk notes)
```

## How evidence is captured

- SSE / event-stream proofs: `scripts/capture-sse.py` (added in Phase 0). It
  captures a live SSE stream to JSONL, asserts expected event kinds, redacts
  secrets, and writes into this ledger. See `scripts/README.md`.
- CLI transcripts: raw `xvn <verb>` stdout/stderr, captured verbatim.
- API transcripts: `scripts/xvn_api.py` GETs/POSTs, or `curl` with redaction.
- Browser proofs: Playwright traces / screenshots from `frontend/web`.
- Schema proofs: before/after JSON for persisted artifacts; migration proofs
  run against a fresh DB and a migrated existing DB.

## Per-surface tracking checklist

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done · `[N/A]` not
applicable (record rationale in `final/surface-matrix.md`).

### Phase 0 — inventory & harness
- [x] `final/surface-matrix.md` — every UI/CLI/API/MCP/docs/skill/script touchpoint mapped
- [x] `scripts/capture-sse.py` — SSE→JSONL evidence helper added and named in `scripts/README.md`
- [x] Docs-freshness coverage assessed for new docs/skill files (see surface matrix §Docs gaps)

### Phase 1 — chat rail foundation
- [x] Rust event round-trip tests — `chat-rail/unified-taxonomy-tests.txt`
      (6 obs tests incl. adjacent-tag kind-collision regression; 4 dashboard
      projection tests)
- [x] Frontend reducer tests for out-of-order + duplicate events —
      `chat-rail/frontend-reducer-tests.txt` (11 vitest, tsc clean)
- [x] Durable session rail-state + migration 041 —
      `chat-rail/session-persistence-tests.txt` (11 store tests)
- [x] `chat-rail/unified-stream-api.jsonl` — live capture via `capture-sse.py`
      against the real dashboard over real seeded `session_events` (10 events,
      replay bounded by `replay_complete` last_seq=9), exit 0
- [x] `chat-rail/unified-stream-dashboard.png` — headless-Chrome screenshot of
      the rail rendering the seeded rows (assistant + completed tool + checkpoint)
- [x] Reconnect/resume by session_id — `chat-rail/unified-stream-resume.txt`
      (dashboard integration test through real `ApiContext::open`)
- [x] Runtime migration wiring (041) fixed + regression test —
      `migrate_chat_session_rail_state` in `ApiContext::open`

**Phase 1 COMPLETE** (2026-05-24): unified taxonomy + both projections + live
session stream w/ resume + one-source rail/dock reducer, all surfaces tested,
live SSE + screenshot evidence captured.

### Phase 2 — chat rail safety
- [ ] `chat-rail/research-act-deny-write.txt` (server-side denial, spoofed frontend mode)
- [ ] `chat-rail/restore-checkpoint.txt` (mutate → restore → byte-compare)
- [ ] Tool policy migration test + ask/auto-approve API tests
- [ ] Focus-chain filesystem test + reinjection event
- [ ] Hook timeout / fail-open / fail-closed tests

### Phase 3 — DSPy foundation
- [ ] `dspy/dependency-spike.txt` + `cargo tree` excerpt (where dspy-rs enters)
- [ ] `dspy/dummy-lm-compile.txt` (deterministic, no network)
- [ ] `dspy/optimize-cli-baseline.json` / `dspy/optimize-cli-candidate.json`
- [ ] `dspy/optimization-lineage-row.json`
- [ ] `cargo tree -p xvision-engine` proof: no `dspy-rs`

### Phase 4 — strategy agents
- [ ] `strategy-agents/missing-capability-proof.txt`
- [ ] `strategy-agents/optimized-agent-mint.json`
- [ ] `strategy-agents/ab-holdout-report.md`
- [ ] `strategy-agents/dashboard-agent-diff.png`

### Phase 5 — release gate
- [ ] `final/surface-matrix.md` (complete, with N/A rationales)
- [ ] `final/commands-run.txt`
- [ ] `final/risk-decisions.md`

## Honesty notes

Anything captured against a mock sidecar, seeded store, or deterministic LM
double rather than a live provider/run is labeled as such in the file itself
and in `final/risk-decisions.md`. The wave plan forbids satisfying real-evidence
gates with mock-only proof.
