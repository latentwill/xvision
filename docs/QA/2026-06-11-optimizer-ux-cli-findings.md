# xvn UX / Docs / CLI Findings — 2026-06-11

Issues encountered during overnight operator session that could be addressed
via improved docs, CLI error messages, or UI changes. Grouped by surface.

---

## CLI Errors & Messages

### U1 — `autooptimizer.toml` gives silent exit 2 with no field-level error

**Symptom:** Any non-empty `/data/autooptimizer.toml` causes `xvn optimize run-cycle`
to exit 2 with only:
```
load config: parsing autooptimizer config at /data/autooptimizer.toml
```
No indication of which field is invalid or what the expected schema is.

**Impact:** Operators spending significant time trying valid-looking TOML variations
with no debugging signal. The file must be renamed away as a workaround.

**Fix:** Surface the serde/toml parse error in the exit message. At minimum:
`load config: parsing autooptimizer config at /data/autooptimizer.toml: unknown field 'experiments_per_cycle' at line 3`

---

### U2 — `--config` flag does NOT override `$XVN_HOME/autooptimizer.toml`

**Symptom:** Even when `--config /tmp/other.toml` is passed, the default path
`/data/autooptimizer.toml` is loaded first. If it has invalid content, the cycle
exits 2 regardless of `--config`.

**Impact:** Operators assume `--config` replaces the default; it doesn't. Two
config sources silently merge or the default wins.

**Fix:** Either (a) document in `--help` that `--config` overlays the default,
or (b) make `--config` fully replace the default path (likely the right UX).
Also note in help text: *"If `$XVN_HOME/autooptimizer.toml` exists, it is always
loaded first."*

---

### U3 — `--budget` termination control silently does nothing with Ollama

**Symptom:** `--budget 0.50` has no effect when using `ollama-local` because Ollama
reports $0 cost per call. No warning is emitted.

**Impact:** Operators writing loop scripts rely on budget to bound execution; the
cycle never terminates on budget. Must use `--experiments-per-cycle` instead,
which is not discoverable from the `--budget` help text.

**Fix:** Emit a warning when `--budget` is set and the resolved provider reports
zero cost: *"Warning: provider 'ollama-local' reports $0/token; --budget will not
terminate this cycle. Use --experiments-per-cycle to bound execution."*

---

### U4 — `xvn optimizer` vs `xvn optimize` — two verbs, different systems, same name shape

**Symptom:** `xvn optimizer` is the old DSPy/pattern system; `xvn optimize` is the
new cycle-based strategy optimizer. Both have `run-cycle`, `inspect`, etc. but they
refer to completely different systems. There's no cross-reference in either `--help`.

**Impact:** Operators run `xvn optimizer run-cycle` expecting the new system behavior.
The old verb silently runs a different code path.

**Fix:** Add a deprecation notice to `xvn optimizer --help`:
*"DEPRECATED: This is the legacy DSPy pattern optimizer. For strategy mutation cycles,
use `xvn optimize run-cycle`."*

Also note that `xvn optimizer unlock` is still the correct command to clear the shared
lock — document this explicitly so operators don't go searching.

---

### U5 — No progress events during parent baseline eval (long silent period)

**Symptom:** After `{"type":"parent_selected",...}` the cycle emits nothing for
10–20 minutes while the parent baseline backtest runs. From the operator's perspective
the process appears hung.

**Impact:** Operators cancel cycles prematurely thinking they've frozen. The previous
session cancelled two cycles at this stage.

**Fix:** Emit periodic progress events from the eval runner back to the cycle output
stream: `{"type":"eval_progress","run_id":"...","decisions":42,"elapsed_s":45}`
at ~30s intervals. Even a heartbeat `{"type":"heartbeat","elapsed_s":60}` would suffice.

---

### U6 — `strategy new --asset` does not accept multiple values, error is not actionable

**Symptom:** `xvn strategy new --asset BTC/USD --asset ETH/USD` fails.
The error doesn't say "use a different flag" or "multi-asset is not supported via CLI."

**Fix:** Either support `--asset` repetition, or return:
*"Error: --asset can only be specified once. Multi-asset strategies must be created via
the dashboard or strategy JSON directly."*

---

### U7 — `strategy new --timeframe 60` fails; "1h" required but not shown in help

**Symptom:** `--timeframe 60` gives a parse error. The help text says `<TIMEFRAME>` with
no format examples.

**Fix:** Add to `--help`: `--timeframe <TIMEFRAME>  Timeframe string, e.g. "1h", "15m", "4h" (not integer minutes)`

---

### U8 — `XVN_PROVIDER_OLLAMA_LOCAL_KEY` not exported — commands fail with opaque error

**Symptom:** The Ollama provider key is stored in `/data/secrets/providers.toml` but NOT
exported to the container environment. Every `xvn` command that touches Ollama requires
`XVN_PROVIDER_OLLAMA_LOCAL_KEY=ollama xvn ...` as a prefix, or returns an error that
doesn't mention the missing env var.

**Impact:** Every Ollama workflow requires a non-obvious env prefix. The error message
("provider key not found" or similar) doesn't say "set XVN_PROVIDER_..._KEY".

**Fix options:**
- (a) Auto-source provider keys from `providers.toml` into the process env at startup
- (b) Emit a clear error: *"Ollama provider key not found in environment. Set
  `XVN_PROVIDER_OLLAMA_LOCAL_KEY=<key>` or add it to the container's env."*
- (c) Document the per-provider env var name pattern in `xvn provider list` output

---

### U9 — `eval list` vs `eval ls` inconsistency

**Symptom:** `xvn eval ls` sometimes fails ("unknown command"); correct verb is
`xvn eval list`. Similarly `xvn provider ls` errors while `xvn provider list` works.

**Fix:** Add `ls` as an alias for `list` on all collection verbs (eval, provider, scenario, strategy), or emit: *"Unknown command 'ls'. Did you mean 'list'?"*

---

### U10 — Filter DSL `asset_scope` must be array; error message doesn't say so

**Symptom:** `xvn strategy set-filter` with `"asset_scope": "BTC/USD"` fails with
`invalid type: string`. The filter-catalog JSON example uses `["BTC/USD"]` but the
error doesn't point to the catalog or the fix.

**Fix:** Add to the error: *"'asset_scope' must be a JSON array, e.g. [\"BTC/USD\"]. See `xvn strategy filter-catalog --json` for a complete example."*

---

### U11 — `wake_when_in_position: on_invalidation_or_target_only` — gotcha not in docs

**Symptom:** Setting this on a filter gates re-fires while a position is open.
During backtests the strategy can open a position and then the filter never re-fires,
causing the eval to run to completion with only 1–2 decisions. No warning is emitted.

**Impact:** Difficult to distinguish from "filter not firing" vs "position blocking re-fire".
The previous session spent significant time debugging this.

**Fix:** 
- Add to filter-catalog docs: *"on_invalidation_or_target_only: the filter will not
  re-fire while a position is open in this asset. Use only if your strategy has a
  reliable exit signal."*
- `eval run` output: emit a filter event like `{"type":"filter_blocked","reason":"position_open"}` 
  when a filter gate is skipped due to open position.

---

### U16 — Bar cache lookup doesn't validate full window coverage before starting an eval

**Symptom (recurring):** The optimizer starts a cycle, emits `parent_selected`, then
silently hangs at 0% CPU for the entire eval. This has happened multiple times across
different agent sessions. Root cause: the requested date window spans multiple bar-cache
entries (e.g. baseline `2025-03-15..2025-05-01` straddles the `Jan-Apr1` and `Apr1-Jun1`
cache boundaries), and the bar fetcher either:
- tries to fetch the gap from Alpaca (empty creds → hangs), OR
- queries each segment separately and fails silently to compose them

**Contributing factor:** `APCA_API_KEY_ID` and `APCA_API_SECRET_KEY` env vars are
empty in the container. The app has a credential/secrets store (the dashboard-managed
secrets in the DB) but the bar fetcher reads **only from ENV**, not from the app's
credential store. Operators configure creds via the dashboard and expect them to work
everywhere, but bar fetches still need the env vars.

**Code structure fix (highest priority):**

1. **Pre-flight bar coverage check** — `xvn optimize run-cycle` and `xvn eval run`
   must verify that all bars for both windows are fully covered by the local cache BEFORE
   spawning the eval, failing with a clear actionable error:
   ```
   Error: bars for BTC/USD 1h 2025-03-15..2025-05-01 are not fully cached.
   Covered: 2025-03-15..2025-04-01 (from cache entry d4c238...), 2025-04-01..2025-06-01 (from 5fd124...).
   Gap: none — but multi-segment assembly is not supported.
   Fix: use `xvn bars fetch --asset BTC/USD --timeframe 1h --start 2025-03-15 --end 2025-05-01`
        or use a window fully contained in a single cache entry.
   ```
   This check must happen before the cycle lock is acquired, so a bad window doesn't
   block the optimizer for the duration of the eval.

2. **Unify credential resolution** — The bar fetcher should resolve Alpaca credentials
   through the same path as the rest of the app (the dashboard-managed secrets store),
   not exclusively from ENV vars. Add a credential resolver that tries:
   (a) `APCA_API_KEY_ID` env var (existing), then
   (b) the app's secret store (`secrets` DB table or providers config), then
   (c) fail-fast with a clear error naming the missing credential and where to set it.
   Removing ENV-only access eliminates the "in the app but not in env" discrepancy.

3. **Bar fetch timeout** — Add a 30s timeout to Alpaca bar fetches. On timeout, emit
   an explicit error rather than hanging indefinitely. This is the last-resort guard if
   pre-flight somehow misses a case.

**Additional UX fix:**
- `xvn bars ls` should show which windows are covered by the union of cached segments,
  not just individual cache entries, so operators can see "coverage gaps" at a glance.

**Operator clarification (2026-06-11):** The Alpaca creds were *already present in the
app's store the whole time* — the operator never needed to enter them. The CLI repeatedly
acted as if it needed creds, apparently because it was trying to fetch bars **manually via
an ENV-only path** instead of (a) checking the cache first and (b) using the creds the app
already holds. This makes the precedence and messaging requirements concrete:
- **Preflight before fetch:** in the common (cached) case the CLI must NOT touch creds at
  all — coverage check first, fetch only on a real miss. This alone removes the spurious
  "needs creds" behavior.
- **Resolve from the app store, silently:** when a fetch is genuinely needed, read creds
  from the app's broker/secret store (where they already live). Do NOT require the operator
  to set `APCA_*` env per-command.
- **Error wording:** if creds are truly absent, point the operator to the **dashboard / app
  broker settings**, NOT to ENV vars. (Contrast U8, where ENV *is* the provider-key
  mechanism — bars/Alpaca creds are app-managed, a different surface.)

---

### U15 — `[[scenario_pool]]` in autooptimizer.toml causes silent exit 2 at runtime (not at parse time)

**Symptom:** `xvn optimize run-cycle --config <file>` exits 0 with `--mock` but exits 2
without `--mock` when the config file contains `[[scenario_pool]]` entries. The error is
"load config: parsing..." followed immediately by exit 2 — no field-level message.

**Root cause theory:** The `--mock` code path skips scenario selection; the real path
tries to deserialize `scenario_pool` entries into a concrete type and fails with an
unhandled error that maps to exit 2.

**Workaround:** Do not use `[[scenario_pool]]` in the TOML. Pass `--day-start`,
`--day-end`, `--baseline-start`, `--baseline-end` as CLI flags and rotate windows
manually in the operator script.

**Fix:** Surface the deserialization error with field path, e.g.:
*"config error: scenario_pool[0].day.start: expected date string, found table"*
Also make the real code path use the same deserialization as `--mock`.

---

### U14 — `lfm2.5:8b` hits `budget_output_tokens_exceeded` even with max_tokens=4096

**Symptom:** `lfm2.5:8b` as trader model causes `budget_output_tokens_exceeded` error after ~93s:
```
step did not complete: status=aborted error=Some("budget_output_tokens_exceeded")
```
The hint says "increase to ≥2048", but max_tokens was already 4096. The model likely
accumulates output tokens across multiple tool-call turns (ohlcv fetch + submit_decision)
and the total exceeds the slot's max_tokens budget.

**Fix options:**
- Document that for agentic multi-turn models, `max_tokens` in the slot acts as a
  cumulative output budget, not a single-call cap. Recommended value for 8B models: 8192–16384.
- Or expose a separate `max_turns_output_budget` config from `max_tokens`.

---

### U13 — `eval cancel` marks run cancelled in DB but does not kill the agentd process

**Symptom:** After `xvn eval cancel --running <id>`, the eval is marked cancelled in
the DB but the node agentd process (`/opt/xvision-agentd/dist/index.js`) continues
running and consuming resources (Ollama GPU memory, CPU).

**Impact:** The next eval run starts while the zombie agentd is still active, competing
for the Ollama inference backend and causing the new eval to appear hung (no decisions
for 10+ minutes).

**Fix:** `eval cancel` should send SIGTERM to the associated agentd process (tracked via
the socket path or a PID file). At minimum, emit a warning: *"Run marked cancelled but
the agent process may still be running. If the next eval is slow, restart the container."*

---

### U12 — Default `max_tokens` in agent slots too small for CoT models

**Symptom:** Strategy created with `strategy new` gets `max_tokens=1024` in the agent
slot. CoT models like `deepseek-r1:8b` emit hundreds of `<think>...</think>` tokens
before the decision JSON, exhausting the budget before any output text is written.

The optimizer cycle exits with code 5 ("truncated at MaxTokens before any text was
emitted") and the eval is marked failed with 0 decisions.

**Impact:** Operators waste a full parent baseline eval (~10 min) before discovering
the slot needs a higher token budget. No pre-run warning is emitted.

**Fix options:**
- (a) Detect CoT-model patterns (deepseek-r*, gemma-*) and set a higher default at
  slot creation time (8192+)
- (b) Emit a pre-launch warning when `max_tokens < 2048` and model name suggests CoT
- (c) `xvn strategy diagnostics` should warn: *"max_tokens=1024 may be insufficient for
  model 'deepseek-r1:8b'; recommended minimum is 4096 for CoT models"*

---

## Docs

### D1 — autooptimizer.toml schema not documented anywhere

**Symptom:** The only source of truth for valid TOML fields is the Rust config struct.
The RUNBOOK had the wrong `[autooptimizer]` wrapper format.

**Fix:** Add `docs/operator/autooptimizer-config.md` with:
- All valid top-level keys (`min_improvement`, `[[scenario_pool]]`, `[mutator]`, etc.)
- Note that `experiments_per_cycle` and `objective` are CLI-only flags, not config fields
- Full example with all optional fields annotated

---

### D2 — Scenario pool vs named scenarios — relationship undocumented

**Symptom:** `xvn scenario ls` shows named scenarios; the `[[scenario_pool]]` in
`autooptimizer.toml` uses raw date ranges. There's no way to reference a named scenario
by ID in the config.

**Impact:** Operators expect to write `scenario_id = "crypto-bull-q1-2025"` and wonder
why it fails.

**Fix:** Document the distinction: "The optimizer's scenario_pool uses date windows
directly, not scenario IDs. Named scenarios (from `xvn scenario ls`) are for eval runs,
not optimizer configuration."

---

### D3 — No CLI verb to list running/completed optimizer cycles (new system)

**Symptom:** `xvn optimizer ls` shows old DSPy runs. There's no `xvn optimize ls`
or equivalent that shows cycle history from `autooptimizer_session_state`.

**Fix:** Add `xvn optimize ls` that queries `autooptimizer_session_state` and shows
cycle_id, strategy, state, kept/dropped counts, started_at.

---

## UI

### UI1 — No optimizer cycle view in dashboard

**Symptom:** The dashboard has no page showing active/historical optimizer cycles,
candidates proposed, or lineage. Operators must use SQLite directly.

**Fix:** Add `/optimizer` dashboard page showing: active cycle progress, recent cycles
table, candidate list per cycle, lineage tree viewer.

---

### UI2 — Filter event details not surfaced in eval run view

**Symptom:** Eval run detail shows decisions but no filter-gate events (when filters
fired, when blocked by position, how many times the gate was checked vs fired).

**Fix:** Add "Filter activity" section to eval run detail: gate check count, fire count,
block count (by reason), with timestamps and indicator values at each fire.

---

### UI3 — Optimizer cycle history list is endless and lacks a strategy column

**Symptom:** The optimizer history list renders every cycle with no pagination/limit,
growing endlessly, and does not show which strategy each cycle ran against.

**Fix:** Paginate / cap the history list (with load-more or a sane default limit), and
add a **Strategy** column identifying the strategy (agent_id / name) each cycle optimized.

---

### UI4 — CLI optimizer runs don't appear in the web UI

**Symptom:** Cycles launched from the CLI never show up on the dashboard optimizer view.

**Root-cause theory:** The verb confusion (U4) — `xvn optimizer` vs `xvn optimize run-cycle`
write cycle state through different code paths / tables, while the dashboard reads only one.
Consolidating to a single `xvn optimize` cycle surface (this session's decision) should make
CLI cycles persist to the same `autooptimizer_session_state` the dashboard reads. Verify the
CLI cycle path writes session state (and emits IPC events) exactly where the dashboard reads.

**Fix:** Ensure the consolidated `xvn optimize` cycle persists to `autooptimizer_session_state`
and surfaces via the same route the `/optimizer` page reads, so CLI-launched cycles appear in
the UI. Add an integration check that a CLI cycle is visible via the dashboard route.

---
