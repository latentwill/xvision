---
name: xvision
description: Orient an agent USING xvision as a product — running `xvn` CLI verbs, the embedded dashboard, MCP tools, strategy/eval workflows, the live Tailscale nodes, and the remote CLI job surface. USAGE ONLY. If the task is editing crates/**, frontend/web/**, migrations, contracts, or CI/deploy scripts, use `xvision-dev` instead; do not load this skill just because the cwd is the xvision repo.
---

# xvision (operator / usage skill)

> **Building xvision rather than using it?** Use the `xvision-dev` skill.
> This skill is for end-user operation: running `xvn`, driving the dashboard,
> submitting remote-CLI jobs, interpreting pipeline output.

A multistrategy trading-agent backtest harness. Single CLI binary `xvn` + a baked-in axum + Vite SPA dashboard.

Use this skill when the task is about *using* xvision: running or explaining
`xvn`, managing strategies/agents/scenarios/eval runs, operating the dashboard
nodes, interpreting xvision pipeline vocabulary, or following xvision operator
runbooks.

## How to invoke xvn

`xvn` ships as a single binary. Two contexts:

**Local binary** (built via `cargo build --workspace`, binary at `target/debug/xvn` or `target/release/xvn`, or installed via `cargo install --path crates/xvision-cli`):
```bash
xvn <args>
```

**Inside Docker** (the runtime image):
```bash
# exec into the running container — preferred for one-off commands:
docker exec xvn-app xvn <args>          # dev node
docker exec xvnej-app xvn <args>        # QA node

# one-shot container via compose:
docker compose run --rm xvn <args>
```

`XVN_HOME` controls where state (strategies, scenarios, the SQLite ledger, config) lives. When running inside Docker it **must match the container's volume mount** — the default in the shipped image is `/data` (set via `XVN_DATA_DIR`; production stacks may override this). If you automate commands against a running node over Tailscale, prefer the remote-CLI job API (`POST /api/cli/jobs`) or `scripts/xvn-remote.py exec` over `docker exec` — both handle reconnects and output streaming.

## Where to look first (operator docs)

- `MANUAL.md` — operator-side prerequisites (Alpaca creds, Orderly onboarding, Mantle minting). Tier 2 = forward-paper, Tier 3 = one-time setup.
- `xvn <verb> --help` — source of truth for any CLI flag.
- `docs/operator/filters.md` — current strategy-level filter workflow and filter-QA checks.
- `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md` — the shipped remote CLI contract for driving live nodes.

For engineering docs (specs/plans/ADRs/FOLLOWUPS, architecture, team board),
switch to `xvision-dev`.

## CLI quick map

`xvn --help` is the source of truth, but the high-traffic verbs:

- `strategy` — author / validate / list / inspect saved `Strategy` artifacts (`$XVN_HOME/strategies/<id>.json`). Atomic mode (`strategy new --prompt`) creates a Strategy + Agent + provider/model binding in one call. Prefer explicit provider/model and asset/timeframe; no workspace default model is assumed for eval launch. `--family / --hypothesis / --target-regime / --avoid-regime` attach a `Hypothesis` to the strategy.
- `strategy set-filter` / `strategy filter-catalog` — install and inspect the deterministic inline Filter DSL. Always consult `xvn strategy filter-catalog --json` before authoring filters from chat rail; it is the canonical machine-readable catalog.
- `scenario` — author scenarios. Includes `select` (read-only comparable set query), `inspect --card` (plain-text card), `classify` (auto-derive regime labels from bars), `set-regime` (operator-authored labels).
- `eval` — `run`, `list`, `show`, `results`, `watch`, `compare` (with `--markdown` table), `batch` (multi-scenario), `attest`, `export` (canonical `EvalRunExport` JSON, q15 §3), `review`, `validate`. `xvn eval run --auto-fire-review --max-review-annotations 8` opts the run into completion-time review annotations; `xvn eval show` prints the stored auto-review state.
- `experiment` — ledger that groups a research question + strategy + scenarios. `experiment run` orchestrates pick → batch → bind → `result_json` in one shot; pair with `--wait --compare --markdown` for a publishable summary.
- `metrics` / `gate` — pre-committed metrics + anti-overfit verdict (treatment vs baseline).
- `dashboard serve` — axum server with the SPA baked in via `rust-embed`. Default bind `0.0.0.0:8788`.
- `provider` — manage registered LLM providers in `$XVN_HOME/config/default.toml`. `refresh-models` hits `/v1/models`; `models` reads the cached catalog (no network).
- `agent get <id>` — fetch one agent record from the workspace agent library (shape matches the `agents[]` slot in `EvalRunExport`).
- `agent inspect <id> --diagnostics` — per-capability readiness for one agent (prompt / model / tools / runtime / optimizable). State-only; exits 0 for a resolved agent.
- `strategy diagnostics <id>` — whole-strategy launch readiness; exits **14** (`OptValidation`) when not launchable, listing each unmet required capability with a typed reason.
- `optimize` — **canonical Optimizer CLI** (codename autooptimizer): offline strategy-mutation flywheel. Sub-commands: `run --strategy <id> [--cycles N] [--mock]` (full optimizer cycle against a strategy), `run-cycle [--strategy <id>] [--mock] [--session-id ...] [--budget N] [--provider P] [--model M] [--day-start DATE] [--day-end DATE] [--baseline-start DATE] [--baseline-end DATE] [--objective METRIC] [--experiments-per-cycle N]`, `mutate-once --parent-bundle-hash <hex> [--config PATH] [--cycle-id ID] [--dry-run] [--db PATH] [--blob-dir PATH] [--mock]`, `demo [--fixture PATH] [-v]`, `inspect <run-id> [--json]`. Offline-only, never on the eval/live path. **Distinct from the deprecated `optimizer`** (see disambiguation below). Operate it via the `xvision/autooptimizer-ops` skill.
- `optimizer` — **DEPRECATED.** All sub-commands (`run-cycle`, `mutate-once`, `demo`, `run`, `ls`, `inspect`, `gate`, `activate`, `retire`, `lineage`) print deprecation notices and delegate to `xvn optimize`. Use `xvn optimize` instead. Kept for backwards compatibility only.
- `flywheel` — observability over memory + Optimizer activity (velocity / health cards).
- `obs retention` / `obs janitor` — agent-run retention policy + TTL/max-bytes sweep.
- `run inspect <run_id>` — materialize `xvn_run.json` + `xvn_report.md` for a finished agent run from the SQLite ledger.
- `intern` / `trader` / `risk` — preview prompts or run one pipeline stage in isolation.
- `store` — SQLite flight-recorder (`xvn.db`) migrate / stats.
- `eod` — end-of-day operator report (markdown to stdout).
- `doctor` — print effective `$XVN_HOME` / config / db / provider / template targets.
- `bars` — SQLite-cached historical bars: fetch / ls / rm / gc.
- `migrate` — apply pending migrations + seed (or `--dry-run` to report state).
- `example` — seed curated example strategies, scenarios, and tutorial artifacts.

## High-value examples

```bash
xvn strategy ls
xvn strategy show <id>
xvn strategy new --prompt prompts/trader.md \
  --name funding-fader --provider openrouter --model kimi-k2 \
  --role trader --asset ETH/USD --timeframe 1h \
  --family compression-breakout \
  --hypothesis "Post-compression range breakouts persist for 4–8 bars" \
  --target-regime "post-compression trend" --avoid-regime chop --json

xvn scenario classify --all                              # auto-label regimes
xvn scenario set-regime <id> --regime expansion --volatility high --direction up
xvn scenario select --asset ETH/USD --timeframe 60 --count 4

xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval compare <run_id_a> <run_id_b> --markdown --sort sharpe
xvn eval batch --strategy <id> --scenarios sc_a,sc_b,sc_c --wait

xvn strategy filter-catalog --json
xvn strategy set-filter <strategy_id> --from-json filter.json

xvn experiment run \
  --name reg-breakout-eth-q1 \
  --question "Does the breakout edge survive across 1h ETH regimes?" \
  --strategy <id> --assets ETH/USD --timeframe 60 --count 4 \
  --wait --compare --markdown --output reports/exp-2026-05-20.md

xvn provider list
xvn provider refresh-models --name openrouter
xvn dashboard serve --bind 127.0.0.1:8788
```

## Safe eval launch path for agents

When an agent is asked to launch, debug, or explain an eval, use this order.
It separates provider/config problems from strategy wiring problems and avoids
turning a launch into a blind first-decision failure:

```bash
# 1. Provider readiness: config exists, secret env is reachable, model catalog is sane.
xvn doctor --json
xvn provider list
xvn provider check --name <provider>
xvn provider models --name <provider>

# 2. Strategy diagnostics: launchability of required capabilities.
xvn strategy diagnostics <strategy-id> --json

# 3. Eval validate: scenario/mode/preflight without enqueueing a run.
xvn eval validate --strategy <strategy-id> --scenario <scenario-id> --mode backtest

# 4. Eval run only after the prior gates are clean.
xvn eval run --strategy <strategy-id> --scenario <scenario-id> --mode backtest
```

`strategy diagnostics` answers "is this strategy launchable?" with a full
`required_unmet[]` breakdown. `eval validate` answers "can this specific
strategy/scenario/mode run be enqueued?" — **it now also runs
`capability_diagnostics` + `assert_launchable` internally and exits 14
(`OptValidation`) if the strategy is not launchable**, so dangling agent refs
and missing capabilities are caught before a run is enqueued. Run both steps
before `eval run` unless the user explicitly asks to skip preflight.

## Execution modes: agent vs mechanical

Use these labels when explaining strategy behavior:

- **Default: Filter-gated agent** — a saved deterministic filter artifact gates
  whether the agent/model is called. The strategy still needs its required
  agent capability and provider/model binding. This is the normal production
  path for filtered LLM strategies.
- **Advanced: Rules-only mechanical** — deterministic rules make the decision
  without a model call. Treat this as an intentional mechanical strategy mode,
  not as "the agent is missing". It should be documented in the strategy
  manifest/hypothesis and checked with `eval validate` like any other mode.
- **Legacy/discouraged: Agent-direct** — the model is called without a saved
  filter gate. Use only for legacy comparisons or explicit baseline work; do
  not present prompt text containing the word "filter" as equivalent to a
  saved filter artifact.

If a strategy is supposed to be **Filter-gated agent** or **Agent-direct** and
diagnostics reports `missing_prompt`, `missing_model_binding`, `missing_tool`,
or `unsupported`, that is a broken/missing-agent state. If the strategy is
intentionally **Rules-only mechanical**, absence of a model call is expected;
the bug would be ambiguous labeling or validation that treats it as an
unconfigured agent.

## Execution venues (Alpaca / Orderly / Byreal)

The direct-execution CLI verbs (`fire-trade`, `portfolio`, `close-position`)
take a `--venue` flag selecting which live executor to build from env:

```bash
# Byreal perps (executes on Hyperliquid via npx @byreal-io/byreal-perps-cli).
# Env: BYREAL_PRIVATE_KEY (required), BYREAL_NETWORK (default mainnet), BYREAL_ACCOUNT (optional).
xvn fire-trade --venue byreal --asset BTC --side buy --size-bps 100 --sl 2 --tp 4
xvn portfolio --venue byreal
xvn close-position --venue byreal BTC

# Other venues:
xvn fire-trade --venue alpaca ...     # Alpaca paper (APCA_API_KEY_ID/...)
xvn fire-trade --venue orderly ...    # Orderly (ORDERLY_KEY/SECRET/ACCOUNT_ID/BASE_URL)
```

`xvn doctor` reports Byreal env readiness under `byreal_venue` (presence only —
never the key). `[runtime] executor = "byreal"` parses but is currently
config-only / not yet dispatched (same as `orderly` today) — venue selection
for the CLI verbs comes from the `--venue` flag, not config.

**Live-eval:** the engine path (`eval run --mode live`, `broker_creds_ref`)
supports `alpaca`, `orderly_testnet`, and `byreal`. Byreal live-eval is
**testnet-only** — set `BYREAL_NETWORK=testnet` (mainnet is rejected with an
actionable error; use the `--venue byreal` CLI verbs for mainnet). Alpaca creds
are still required for the live market-data bar stream regardless of venue.

## Strategy inspector and filters

- Canonical dashboard inspector route: `/strategies/:id`.
- `/authoring/:id` still works only as a compatibility alias for older links.
- The inspector Manifest card edits display name, description, asset universe, and cadence. The stable strategy ID is read-only.
- Eval readiness validation is manual: click **Check eval readiness** or run `xvn strategy validate <id>` / `xvn eval validate`.
- Mechanical params are not an operator tuning surface in the current inspector.
- A real XVN filter is a saved strategy filter artifact, not prompt wording. Prompt text saying "filter" does not prove the filter subsystem ran.
- For filter QA, confirm the strategy shows a filter artifact and completed eval details include filter summaries/events when expected.
- Eval details separate direct model decisions from synthesized rows such as `noop_skip` and early-stop inheritance. Treat high synthesized counts as a QA caveat.

## Pipeline vocabulary (locked 2026-05-10, terminology rename Option B)

| Concept | Name |
|---|---|
| Per-decision-cycle id | `cycle_id` (NOT `setup_id`) |
| Local strategy id (pre-NFT-mint) | `agent_id` (ULID; becomes NFT token id post-mint) |
| Pipeline-config artifact | `Strategy` |
| Decision producer (eval baseline) | `Algorithm` (NOT `Strategy`) |
| One A/B arm | `arm` / `Box<dyn Algorithm>` |
| Trader output | `TraderDecision` |
| Risk gate verdict | `RiskDecision` (Approved / Modified / Vetoed) |
| Cycles DB table | `cycles` (formerly `setups`) |

**Pipeline roles** (intern → trader → risk → executor) are valid
conventions, not hardcoded slot names. The current shipped CLI initializes
state with `xvn init` (back-compat alias: `xvn migrate`); interactive
setup/onboarding is handled through the dashboard wizard and operator runbooks.

## Experiment vs strategy vs eval (mental model)

- **Strategy** — the immutable pipeline configuration (one `Strategy.json`,
  one or more `AgentRef`s). Authored via `xvn strategy new`.
- **Eval run** — one execution of a strategy against one scenario.
  Launched via `xvn eval run` (or `eval batch` for N scenarios in one call).
- **Experiment** — a ledger row that groups a research question + a strategy
  + the set of scenarios that question demands. `xvn experiment run`
  orchestrates the whole loop: it picks scenarios (`--scenarios` explicit,
  or `--assets / --timeframe / --regimes / --target-decisions / --same-decisions`
  selector mode), launches an eval batch, binds the run ids back to the
  experiment row, and (with `--wait --compare --markdown`) emits a
  publishable comparison table. `--decision-budget` is metadata only — it
  records operator intent, it does not cap eval execution.

Use `experiment` when the operator's question is the unit of work
("does this strategy survive across these regimes?"); use a bare `eval batch`
when you just need N runs and don't need the ledger row.

## Disambiguation: verbs agents routinely confuse

Match the **intent**, not the verb's spelling.

### "compare" — `eval compare` vs `experiment run`

To A/B two real strategies: launch two eval runs (`eval run` ×2, or `eval batch`)
and then `eval compare` their ids — or use `experiment run` to do
pick → batch → compare in one call.

| You want to… | Use |
|---|---|
| Compare 2+ **completed eval runs** (by `run_id`) — metrics, equity, findings | `xvn eval compare <run_a> <run_b> [--markdown --sort sharpe]` |
| Compare every run in a finished batch | `xvn eval compare --batch <batch_id> --markdown` |

### "optimize" — `xvn optimize` (canonical) vs `xvn optimizer` (deprecated)

| Verb | What it is | Operates on | Skill |
|---|---|---|---|
| `xvn optimize` | **Canonical Optimizer CLI** (codename autooptimizer): offline flywheel that mutates strategies via `run-cycle` / `mutate-once` / `demo` / `inspect` | Patterns / strategies across the flywheel | `xvision/autooptimizer-ops`; health via `xvision/flywheel-ops` |
| `xvn optimizer` | **DEPRECATED** — all sub-commands delegate to `xvn optimize` with a deprecation warning. Use `xvn optimize` instead | same as above (via delegation) | same, but use `xvn optimize` |

Both are **offline-only** and never run on the live or eval path. `xvn optimizer`
continues to work (it delegates), but new scripts and docs should use `xvn optimize`.

## Inline deterministic Filter DSL

For pure indicator gates, prefer `xvn strategy set-filter <strategy_id>
--from-json <path>` over an LLM Filter agent. The authoritative catalog
is `xvn strategy filter-catalog --json`; the human docs are
`docs/operator/filter-dsl-catalog.md` and `/docs?slug=filter-dsl-catalog`.

Current high-value tokens include ADX/DI (`adx_14`, `di_plus_14`,
`di_minus_14`), recent/persistent operators (`above_for_<bars>`,
`crossed_above_<bars>`, `crossed_below_<bars>`), opening range
(`opening_range_high_30`, `opening_range_low_30`), time-of-day volume
(`rvol_tod_20`), and `volume_zscore_20`.

Filters may include optional `fire` metadata:

```json
{
  "fire": {
    "reason": "trend_breakout",
    "priority": 0.85,
    "tags": ["trend", "breakout", "volume"],
    "context": ["close", "opening_range_high_30", "adx_14", "rvol_tod_20"]
  }
}
```

`fire` does not change the boolean gate. It adds a compact trigger
reason/context bundle to traces and trader briefings when the gate is
active.

Compare surfaces:

- `/eval-runs/compare?ids=<run-a>,<run-b>` is run-centric: metrics, findings,
  behavior, and the full run table.
- `/charts/compare?ids=<run-a>,<run-b>` is Charts v2 strategy-centric:
  equity overlay, roster pills, and cards. Roster changes update the URL.
- CLI and JSON keep ids as the addressing primitive, but labels prefer
  `strategy_name` from the strategy manifest when available.

## Optimizer CLI (`xvn optimize`)

> `xvn optimizer` is **deprecated** and delegates here. Use `xvn optimize` directly.
> See the `xvision/autooptimizer-ops` skill for the full operator workflow.

Run the strategy-mutation flywheel **offline** — never on the eval or live path.

Sub-commands:

- `run --strategy <id> [--cycles N] [--mock]` — run the full optimizer cycle against a strategy (strategy ID is required)
- `run-cycle [--strategy <id>] [--mock] [--session-id ...] [--budget N] [--provider P] [--model M] [--day-start DATE] [--day-end DATE] [--baseline-start DATE] [--baseline-end DATE] [--objective METRIC] [--experiments-per-cycle N]` — full optimizer cycle (strategy optional for backwards compat)
- `mutate-once --parent-bundle-hash <hex> [--config PATH] [--cycle-id ID] [--dry-run] [--db PATH] [--blob-dir PATH] [--mock]` — single mutation proposal
- `demo [--fixture PATH] [-v]` — replay a saved cycle from a fixture file
- `inspect <run-id> [--json]` — inspect a persisted optimization run

```bash
# Run the full optimizer cycle against a strategy
xvn optimize run --strategy <strategy_id> --cycles 3

# Run a single optimizer cycle (fine-grained control)
xvn optimize run-cycle --strategy <strategy_id> \
  --budget 100 --provider openrouter --model kimi-k2 \
  --objective sharpe --experiments-per-cycle 5

# Propose one mutation without running the full cycle
xvn optimize mutate-once --parent-bundle-hash <hex> --dry-run

# Replay a saved fixture for debugging / demos
xvn optimize demo --fixture fixtures/optimizer-cycle.json -v

# Inspect a persisted optimization run
xvn optimize inspect <run-id> --json
```

Note: `memory-demos`, `memory-demos-gate`, `accept-as-child-agent`,
`revert-accepted`, `export-demos`, `import-demos`, and `explain-missing-data`
have been **removed** from `xvn optimize`. Those sub-commands no longer exist.
Full surface in [`references/cli.md`](references/cli.md).

## Capability diagnostics (launch readiness)

`xvn strategy diagnostics <id>` is the launch gate — every required capability
needs a prompt, model binding, required tools, and a supporting runtime. It
exits **14** when not launchable, listing each unmet capability with a typed
reason (`missing_tool` / `missing_prompt` / `missing_model_binding` /
`unsupported`). `xvn agent inspect <id> --diagnostics` is the per-agent,
strategy-independent view (state-only, exits 0 for a resolved agent).

```bash
xvn strategy diagnostics <strategy-id> --json   # launchable + required_unmet[]
xvn agent inspect <agent-id> --diagnostics --json
```

Both `xvn strategy validate` and `xvn eval validate` now run the same
`assert_launchable` gate internally and exit 14 (`OptValidation`) on a
blocker — so either serves as a safe shell gate. `strategy diagnostics`
additionally emits the structured `required_unmet[]` JSON breakdown, which
is useful when you need to know exactly *which* capability is missing.
For agent-facing launch work, prefer the full sequence:
provider readiness → `strategy diagnostics` → `eval validate` → `eval run`.

## Chat rail (conversational driving surface)

The chat rail is the dashboard's persistent conversational session — distinct
from the headless `xvn` automation loop. It has a unified, replayable event
stream and **server-enforced** safety. Drive it via its HTTP endpoints (these
are stable, unlike the general dashboard CRUD API):

- `GET /api/chat-rail/sessions/:id/stream?after_seq=<n>` — replay past the
  cursor → `replay_complete{last_seq}` → live tail. Reconnect with the last
  `seq` you saw; events have stable `event_id` + monotonic `seq`. Order/dedupe
  on `(session_id, seq)` — do **not** regex the stream.
- `POST /api/chat-rail/sessions/:id/mode` `{ "mode": "research"|"act" }` —
  research denies write tools **before** they run (server reads the persisted
  mode column; the client can't spoof it); act allows them subject to policy.
- `GET/PUT /api/chat-rail/tool-policy` — three-state `(enabled, auto_approve)`:
  Auto / Ask / Disabled. Absent tool ⇒ class default (read=Auto, **write=Auto**,
  dangerous=Disabled): a Write tool **auto-runs in Act mode** (no approval
  round-trip) and is **denied in Research mode**. To force an approval prompt for
  a write tool, set its policy to `auto_approve:false` (the **Ask** state).
  `run_eval` is a Write tool, so in Act mode it launches a backtest with no
  confirmation by default. Unknown tool fails safe to write.
- `GET/PUT /api/chat-rail/focus` — per-scope `focus.md` at
  `$XVN_HOME/scopes/<kind>/<id>/focus.md`, re-injected each turn; path-safe.
- `GET /api/chat-rail/sessions/:id/checkpoints` +
  `POST /api/chat-rail/checkpoints/:cid/restore` — list newest-first; restore
  rewinds a strategy byte-identically and is non-destructive on failure.

Typed-error event kinds (`error_missing_capability`, `error_missing_tool`,
`error_invalid_schema`, `error_provider_unavailable`, `error_policy_denied`,
`error_persistence_failed`) never short-circuit silently. Full taxonomy + shapes
in `/docs?slug=driving-xvn-as-an-agent`.

## MCP tool peers for new CLI verbs

The CLI workbench wave landed six new MCP tools that mirror the new verbs,
so the chat-rail / MCP-driving agent doesn't shell out to `xvn`:

| MCP tool | CLI equivalent |
|---|---|
| `xvn_strategy_create_atomic` | `xvn strategy new --prompt …` (atomic mode) |
| `xvn_strategy_validate_preflight` | `xvn strategy validate <id>` (returns `eval_ready` + warnings/errors) |
| `xvn_eval_batch_run` | `xvn eval batch --strategy <id> --scenarios …` |
| `xvn_eval_compare_report` | `xvn eval compare …` decorated with behavior summary per row |
| `xvn_scenario_inspect_card` | `xvn scenario inspect <id> --card` |
| `xvn_eval_behavior` | on-demand `BehaviorSummary` for a finished run |

These wrap the same engine APIs the CLI calls — no `xvn`-binary shell-out.
Prefer them over `POST /api/cli/jobs` when an MCP client is already attached.

## Live nodes (operator surface)

Two live `xvn` instances behind Tailscale on `extndly-dev`:

- `https://xvn.tail2bb69.ts.net` — personal node
- `https://xvnej.tail2bb69.ts.net` — QA node

Live-node control means the Tailscale-served dashboard + remote-CLI surface,
not generic SSH orchestration. Assume one of those hostnames unless told
otherwise. Deployment mechanics (image build, compose recreate, cert
issuance) live in `xvision-dev` — operator tasks should call the dashboard /
remote CLI, not run `docker compose` directly.

## Remote CLI over Tailscale

This is the Tailscale-served remote CLI surface for typed argv jobs.
For command-style live-node work, prefer the typed remote CLI job API instead of ad hoc SSH or shell assumptions.

- Use `scripts/xvn-remote.py exec ...` for a shell-free helper that submits typed argv.
- Use `POST /api/cli/jobs` with `argv` for direct API access.
- Use `GET /api/cli/jobs/:id` and `GET /api/cli/jobs/:id/output` to reconnect after disconnects.
- Use `GET /api/cli/jobs/:id/events` for SSE progress.
- `xvn-mcp` is separate stdio MCP, not the HTTP remote-control surface.

## Don'ts (operator-facing)

- Don't bind the dashboard wider than loopback outside Tailscale until **F35** (dashboard auth) lands.
- Don't drive the live nodes through ad-hoc SSH or shell text — use the typed remote-CLI job API or `scripts/xvn-remote.py`.
- Don't bypass the `xvn provider` / `xvn strategy` / `xvn eval` surfaces by editing `$XVN_HOME` files directly — the CLI knows the right invariants and audit hooks.
- Don't call a run a filter-functionality test unless a real filter artifact was attached and the eval output shows filter participation.
- Don't infer strategy behavior from synthesized eval rows without calling out `noop_skip`, graph-gated, or early-stop provenance.

## Deeper references

- [`references/cli.md`](references/cli.md) — full CLI subcommand surface with examples.
- [`references/architecture.md`](references/architecture.md) — pipeline stages and how `xvn` output maps to the engine internals (operator-relevant only).

Engineering-side deployment + crate-level architecture moved to the
`xvision-dev` skill.

## Filter DSL — wake_when_in_position

Controls whether the trader is re-invoked while a position is open in the
filter's asset (per-bar polling cost during a hold; it does not change entry
firing). Set in filter JSON to one of three tokens:

| Value | Behavior | When to use |
|---|---|---|
| `"never"` | No mid-position calls; exits via `risk.stop_loss_atr_multiple` | Mean-reversion strategies (hold to target) — fewest decisions |
| `"on_invalidation_or_target_only"` | Wake only on a fresh trip (the bar the condition tree first becomes true again); sustained-true bars suppressed | Strategies needing early exit on signal reversal — caution on oscillating indicators |
| `"always"` | Wake on every true bar while in position (expensive — one trader-LLM call per in-position bar) | Almost never correct |

Default: `"on_invalidation_or_target_only"` (the cost-safe default; the old
`"always"` per-bar default was removed). Start with `"never"` unless you need
early exits. With the default the filter will NOT re-fire while a position is
open, so pair it with a distinct exit signal or `risk.stop_loss_atr_multiple` —
otherwise an entry condition that stays true never re-wakes the trader to
close, and a run can finish with only 1–2 decisions (look for `filter_blocked`
events with `reason = position_open` in the run detail).

**Session data:** A 21-trade SORB strategy produced 153 decisions with `on_invalidation_or_target_only` vs 14 decisions with `never`.

## Eval chain — process group isolation

Eval chains launched as background shell scripts share PIDs with their parent shell. Kill with:
```bash
# Launch: store the process group ID
setsid bash /tmp/eval_chain.sh &
PGID=$!

# Kill the entire process group (kills child processes too)
kill -- -$PGID
```

Or add a lockfile guard at the top of eval_chain.sh:
```bash
LOCKFILE=/tmp/eval_chain.lock
exec 200>"$LOCKFILE"
flock -n 200 || { echo "chain already running"; exit 1; }
```

## Critical workflow: agent tools attachment

After `xvn strategy new`, ALWAYS run:
```bash
xvn agent set-tools <AGENT_ID> --slot main --tools submit_decision
```
Atomic strategy creation does NOT attach the submit_decision tool. The strategy will not function until this is done.

---

*Skills owner: whichever track ships a new `xvn` verb, Filter DSL
surface, or operator-visible strategy/eval workflow is responsible for
updating this file in the same PR. Last refresh: 2026-06-10 (optimizer CLI migration: xvn optimize is canonical, xvn optimizer is deprecated).*
