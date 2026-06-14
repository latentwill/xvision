# Optimizer error-hunt — LOCAL MODELS ONLY — 2026-06-14

**Setup.** `xvn 0.21.0` in `xvn-app`. Root strategy `01KTYBP2KDQBHE131TVNMXWP14`
(`macd-adx-trend-1d`, trader bound to **ollama `Qwen3-4B-Instruct`**). Mutator+judge
forced local: `--provider ollama --model hf.co/unsloth/Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL`.
Window: day `2024-10-01..2024-12-01`, baseline `2024-12-01..2025-01-01` (both inside the
1Day/4Hour/1Hour cache overlap → no bar-cache-boundary hang). 1 cycle, 2 experiments.
Cycle `01KV3CGXMMWRP1FKT9J6VZT8DN`, session `01KV3CGWGJRRPMZCS1R4C6TGTB`.
**Fully local — `cumulative cost $0.0000`, zero paid tokens.** Wall clock ~4.5 min.

## Headline
On local models the optimizer **cannot complete a productive cycle**. The mutator can't
emit a parseable `MutationDiff` (qwen3-4b is too weak for the structured-diff contract), so
the cycle produced **0 candidates**; the no-candidate path then hit an **illegal session
state transition (`failed -> finished`, exit 5)** that left the cycle **unsealed and invisible
to `optimize ls`/`show`** even though its events were persisted. This is a stronger failure
than the known F32 (which collapses to *identical* candidates on gemini-flash-lite); here a
smaller local model produces *no parseable candidate at all*.

---

## CRITICAL

### C1 — Mutator parse failure → 0 candidates (optimizer non-functional on local models)
Both experiments failed identically (events 8 & 11):
```
no_candidate … reason: "experiment writer produced no usable candidate:
  mutator failed after 3 attempt(s): - [parse_error] failed to parse MutationDiff from LLM response"
```
qwen3-4b never produced a parseable `MutationDiff` across 3 internal retries × 2 experiments
(~62s + ~50s of LLM time wasted). With no candidate, the optimizer cannot explore — it is a
no-op on this model class.

### C2 — Illegal session state transition → exit 5
Final stdout line:
```
run session: illegal state transition failed -> finished
EXIT=5
```
When a cycle yields no candidate, the session is marked `failed` and then the seal tries
`failed -> finished`, which the state machine rejects. The process exits **5** (undocumented;
the only non-zero exit observed in the whole hunt).

### C3 — Split-brain persistence: events saved, cycle not sealed/indexed
- `optimize export 01KV3CGX… --format json` → **15 events**, full phase timeline. ✅ found
- `optimize ls` → `no optimizer cycles yet` ❌
- `optimize show 01KV3CGX…` → `no optimizer cycle with id …` ❌

The persisted timeline has **no terminal seal event** (last event = `honesty_check` finished);
the "dropped" verdict and the state-transition error were never written. So a real 4.5-min
cycle is retrievable by `export` but invisible to the primary list/inspect verbs. Caused by C2
aborting the seal.

---

## MEDIUM

### M1 — Error states return exit 0 (inverted vs internal bugs)
| Command | Output | Exit |
|---|---|---|
| `optimize run --strategy <nonexistent>` | `strategy … not found` | **0** |
| `optimize show <nonexistent ULID>` | `no optimizer cycle with id …` | **0** |
| `optimize show <malformed id>` | `no optimizer cycle with id …` (no validation) | **0** |
| `optimize export <nonexistent>` | silently writes empty doc | **0** |
| `optimize lineage show <bad>` | `invalid bundle_hash: hex decode …` | **0** |
User/not-found errors all exit 0, while the *internal* state bug (C2) exits 5 — backwards for
automation. The documented 4/10–15 exit-code taxonomy is not honored.

### M2 — `export <nonexistent-id>` fabricates an empty document
No existence check — `export` just queries events for the id; a typo'd id emits a valid-looking
`Events: 0` artifact (exit 0) instead of a not-found error.

### M3 — `session_id` empty everywhere but the banner
The banner prints `session: 01KV3CGWGJRRPMZCS1R4C6TGTB`, but **every** streamed event and the
persisted export carry `session_id: ""`/`null`. Session correlation is impossible downstream.

### M4 — parse-error events don't capture the offending model output
The `no_candidate` events record only the `[parse_error]` class, not qwen3-4b's raw response.
Debugging the mutator failure requires re-running with ad-hoc logging — the artifact is useless
for diagnosing *why* the diff didn't parse.

### M5 — local trader emits non-conforming output (recovered)
Parent day-window backtest logged `nodecision_recovery_succeeded … method="output_text_json_scan"`
(cycle0 & cycle1) — qwen3-4b didn't emit clean trader JSON; the engine recovered by scanning
text. Works, but same structured-output fragility class as OpenRouter `trader_output_schema_patch_recovered` (#933).

---

## LOW

- **L1** `optimize export --format markdown` is rejected; valid values are `md`/`json` (help/skill say "Markdown").
- **L2** `optimize lineage show` expects a hex `bundle_hash`, not a cycle/experiment id like `show`/`export`; passing an id gives a cryptic `hex decode: Odd number of digits`.
- **L3** `optimize lineage ls` shows a **2-day-old experiment still `active`** (`01fb8153`, 2026-06-12) and **empty Parent/Cycle columns** on every row (known F33 attribution gap).
- **L4** `honesty_check` events carry `parent_hash: null` while all other events carry the real hash — inconsistent.
- **L5** export-json `active_count`/`suspect_count`/`rejected_count` are `null` rather than `0`.
- **L6** `eval_untouched_window` finished in **952 ms** vs day-window **144,108 ms** — baseline likely evaluated **0 decisions** (filter didn't fire / warmup) so the held-out overfitting guard saw nothing; worth confirming.

---

## Working correctly (no bug)
- Bar-cache window selection (1d/4h/1h overlap) → **no boundary hang**.
- Lock **acquired and released cleanly even after the C2 failure** (no wedge); `unlock` with no lock held is a graceful no-op (`no optimizer cycle lock was held`).
- `optimize export` reliably reconstructs the event timeline; honesty-check phase ran and correctly self-skipped with no candidate.
- **Parent strategy untouched** (mtime unchanged) — no accidental mutation persisted.
- Fast-fail on nonexistent strategy *before* any eval/lock work.
- Truly local end-to-end: `$0.0000` cumulative cost.

## Artifacts (in `/root/xvn-work/optimizer-test-2026-06-14/`)
- `run1.log` — full stdout incl. the `illegal state transition` + `EXIT=5`
- `cycle_export.json` / `cycle_export.md` — the 15-event persisted record (visible only via `export`)
- this `FINDINGS.md`

## Top fixes
1. **C2**: handle the `failed -> finished` transition (or transition `failed -> dropped/sealed`) so no-candidate cycles seal cleanly and exit 0/seal-coded, not 5.
2. **C3**: write the terminal seal event on every exit path so `ls`/`show` and `export` agree.
3. **C1/M4**: capture the mutator's raw output on `parse_error`; add a repair/text-scan fallback for the mutator like the trader already has (`output_text_json_scan`), or gate the optimizer to models that can hold the `MutationDiff` contract.
4. **M1/M3**: non-zero exit on not-found/validation; stamp `session_id` on events.
