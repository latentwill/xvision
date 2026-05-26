# Agent CLI — cli-printing-press Audit & Punch List

**Date:** 2026-05-25
**Surface:** `crates/xvision-cli/` (binary `xvn`), `docs/cli-non-surfaced.md`,
`README.md`, `MANUAL.md`, `.claude/skills/xvision-cli/`, `scripts/xvn_*.py`.
**Status:** Draft for user review
**Related:**
- External: [mvanhorn/cli-printing-press](https://github.com/mvanhorn/cli-printing-press) — the agent-native CLI generator whose primitives this spec audits against.
- External: [Trevin Chow — 10 Principles for Agent Native CLIs](https://trevinsays.com/p/10-principles-for-agent-native-clis) — the press's primary intellectual source.
- `crates/xvision-cli/src/exit.rs` — typed exit codes ("following the Printing Press convention").
- `crates/xvision-cli/src/io.rs` — stdout/stderr channel discipline.
- `docs/cli-non-surfaced.md` — existing anti-surface manifest.
- `team/contracts/cli-json-stdout-contract.md` — the JSON stdout binary contract.
- `team/intake/2026-05-16-eval-review-and-v2a.md` — current wave intake (this spec sits behind it).

## Goal

Take the design ideas that the cli-printing-press project ships as conventions
for agent-native CLIs, score xvision's existing CLI surface against them, and
publish a punch list of contract-sized tracks the conductor can hand out one
at a time. The aim is **not** to copy the press wholesale — xvision is not a
generated wrapper and several of the press's mechanisms (NOI gate, Ecosystem
Absorb manifest, browser-sniff path, proof-of-behavior gates) are generator-
side concerns that don't apply. The aim is to close the specific gaps where
xvision still leaks human-CLI habits, and to name the Rung-5 insight verbs
that the SQLite flight recorder makes possible but `xvn` doesn't yet expose.

This document is **phase 0** of the `agent-cli-press` track. It resolves the
seven decisions below and decomposes the follow-on work into nine tracks with
explicit acceptance criteria.

## Scope

This spec covers:

- Resolution of the seven decisions listed in the §Decisions section.
- The universal flag set that should land on every applicable `xvn` verb.
- The cookbook and `AGENTS.md` content shape.
- The list of Rung-5 insight verbs to spike (with one chosen as the first).
- The disposition of every `scripts/xvn_*.py` helper (promote into `xvn` or
  declare in `cli-non-surfaced.md`).
- The decomposition into nine follow-on tracks, with dependencies.

This spec does **not** cover:

- Reworking the `xvn-mcp` server. Its deprecation note in
  `docs/cli-non-surfaced.md` already covers disposition; this spec leaves it
  alone except to assert "no verb duplication between `xvn` and `xvn-mcp`."
- Replacing `clap` or the existing subcommand routing in `lib.rs`.
- Frontend or dashboard work — the dashboard's `xvn-remote.py` overlap is in
  scope only insofar as it informs §Decision 6.
- The on-chain identity surface — `cli-non-surfaced.md` already locks this.

## Context: what xvision already has

The press's bar is lower than it appears for xvision specifically, because
several of its primitives are already shipped:

- **Typed exit codes** with the same scheme (0/2/3/4/5/7). `exit.rs` literally
  documents itself as "following the Printing Press convention."
- **stdout/stderr channel discipline** with the binary "stdout is JSON-only
  when `--json` is passed" contract in `io.rs` and the `cli-json-stdout-contract.md`.
- **A doctor verb** — `xvn doctor` reports xvn_home / db / config /
  effective providers, with `--json`.
- **An explicit anti-surface manifest** — `docs/cli-non-surfaced.md` is more
  disciplined than the press's own equivalent. Keep it; this spec extends it.
- **A skills layer** under `.claude/skills/xvision-cli/` already routed from
  the README's "For Agents" section.
- **Tracing routed to stderr by default** in `main.rs`.

The gaps are concentrated in: universal flag coverage, auto-JSON-on-pipe,
agent-discoverable documentation (no `AGENTS.md`, no cookbook), env-var
diagnostics (`doctor` is structural only), narrowing hints on list verbs,
verb-name regularity, the Python script overlap, and Rung-5 insight verbs.

## Decisions

### Decision 1 — Universal flag set

Every `xvn` verb that emits structured output adopts the following flag
contract, implemented as a shared `AgentFlags` clap group in
`crates/xvision-cli/src/io.rs` (new module: `flags.rs`):

```text
--json            Force JSON output to stdout (existing behavior).
--compact         Project only high-gravity fields. Implies --json.
                  Per-verb projection table lives in commands/*.rs.
--select <expr>   jq-style field projection over the --json payload.
                  Implies --json. Example: --select '.cycle_id,.action'.
--csv             Emit CSV instead of JSON. Mutually exclusive with --json.
--quiet           Suppress all stderr human! / progress! lines.
--no-color        Disable ANSI in human-mode output. Auto-disabled when
                  stdout is not a TTY.
--dry-run         Per-verb: previews the side effect, exits 0 without
                  mutating. Required on all mutating verbs.
--stdin           Read the primary input from stdin instead of a path arg.
                  Verbs that accept a `--snapshot path.json` style arg gain
                  `--stdin` as an alternative.
--yes             Suppress confirmation prompts. xvn has none today; this
                  reserves the flag for future destructive verbs.
--no-input        Fail-fast instead of prompting for missing input. Same
                  reservation as --yes.
```

Two flags from the press are **explicitly not adopted**:

- `--no-cache` — collides with `--data-source live` (see Decision 3); the
  data-source selector is the cache policy.
- `--data-source live|local|auto` is preferred over a bare `--no-cache`.

Auto-JSON-when-piped (`!isatty(stdout)` → compact JSON) lands as a separate
mechanism, not a flag. See Decision 2.

Verbs not currently `--json`-aware (`migrate`, `fire-trade`, `portfolio`,
`close-position`, `report`) get audited; if their output is structured at all,
they gain `--json`. If they're pure side-effect (`fire-trade`), they emit a
small JSON receipt under `--json`.

### Decision 2 — Auto-JSON when piped

`io.rs` gains an `output_mode()` helper:

```rust
pub enum OutputMode { Json, JsonCompact, Csv, Human }

pub fn output_mode(flags: &AgentFlags) -> OutputMode {
    if flags.csv { return OutputMode::Csv; }
    if flags.compact || flags.select.is_some() { return OutputMode::JsonCompact; }
    if flags.json { return OutputMode::Json; }
    if !std::io::stdout().is_terminal() { return OutputMode::JsonCompact; }
    OutputMode::Human
}
```

Every verb calls `output_mode()` once at the top and branches on the result.
The "stdout is JSON-only when `--json`" invariant from
`cli-json-stdout-contract.md` extends to "stdout is JSON-only when
`output_mode()` is `Json` or `JsonCompact`" — same channel discipline, wider
trigger. Banner/progress text continues to route through `human!` /
`progress!` to stderr regardless.

The contract file is updated to reflect the broader trigger. The test in
`tests/json_stdout_contract.rs` is extended with a piped-stdout case
(spawn `xvn eval list` with a closed-pty stdout, assert JSON).

### Decision 3 — `--data-source` selector replaces ad-hoc cache flags

`xvn ab-compare` currently expresses cache policy as a mutual exclusion
between `--bars <path>` (file) and `--from`/`--to` (cache + Alpaca on miss).
This collapses into:

```text
--data-source auto|local|live
    auto:  use cache, fall through to live provider on miss (default)
    local: use cache only; exit 4 (NotFound) if any bar in window is missing
    live:  bypass cache, refetch every bar from the provider
--bars <path>     overrides --data-source; treats the file as an external feed
```

The same selector is exported to `xvn bars ls`, `xvn bars fetch`, and any
future verb that touches a cache layer (`xvn provider list --effective`
already reads cache state; gains `--data-source` for symmetry). The selector
gives an agent one knob to reason about across the surface, rather than
verb-specific flag combinations.

### Decision 4 — `AGENTS.md` at root, cookbook split out

Two new top-level docs:

- `AGENTS.md` — single canonical agent-operating guide. Contents hoisted
  from README's "For Agents" section + "Hard deployment rules for agents".
  README replaces both sections with a one-line pointer:
  `> For agents driving this repo, start at [AGENTS.md](AGENTS.md).`
- `docs/cookbook.md` — concrete invocation recipes for the 10–12 highest-
  frequency agent tasks. Linked from `AGENTS.md`.

Cookbook v1 covers, at minimum:
1. Backtest one strategy on one scenario (local).
2. Backtest the same scenario remote via `scripts/xvn-remote.py`.
3. Promote a strategy variant from the autoresearcher (variant → lineage seal).
4. Re-run a single cycle from the flight recorder.
5. Diff two eval runs (treatment vs baseline).
6. Fire a paper trade end-to-end against Alpaca paper.
7. Inspect a stuck/failing run (`xvn run`, `xvn obs`, `xvn doctor auth`).
8. Add or edit an inline strategy filter (`xvn strategy set-filter`).
9. Refresh the bar cache for a window.
10. Open the dashboard locally and on the live node.

Each recipe is a verb-by-verb shell snippet with one-line rationale. No prose
expansions; an agent should be able to grep one recipe and copy.

### Decision 5 — `xvn doctor auth` enumerates the credential surface

A new `xvn doctor auth` subcommand (sibling of the existing structural
`doctor`) enumerates every env var the CLI cares about, with set/unset/
suspicious/shadowed status and 4-char fingerprints. Exits 0 always (it is
diagnostic, not gating — same convention the press uses). The credential
surface to enumerate:

- LLM providers: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, any
  `OPENAI_BASE_URL` overrides.
- Brokers: `APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`, `APCA_API_BASE_URL`,
  `ORDERLY_KEY`, `ORDERLY_SECRET`, `ORDERLY_ACCOUNT_ID`, `ORDERLY_BASE_URL`.
- Remote control: `XVN_BASE_URL`, `XVN_REMOTE_URL`, `XVN_HOME`.
- Observability: `XVN_MEMORY_DB`.

The list lives in `crates/xvision-cli/src/commands/doctor.rs` as a single
const array, with a one-line `purpose` string per var so the JSON output is
self-documenting. The existing `xvn doctor` keeps its current shape; `xvn
doctor auth` adds the env-var dimension.

### Decision 6 — Disposition of `scripts/xvn_*.py`

Each Python helper is either **promoted** (its verbs land in `xvn`, gated on
local-vs-remote at runtime) or **demoted** (declared in
`cli-non-surfaced.md` with a rationale). Default disposition by helper:

| Helper | Default | Rationale |
|---|---|---|
| `xvn_api.py` | keep | infrastructure library for the others; not an agent-facing verb |
| `xvn-remote.py` | keep | the remote-control entrypoint; document in `AGENTS.md` |
| `xvn_eval_harness.py` | promote | overlap with `xvn eval show/export`; merge into `xvn eval export --remote` |
| `xvn_filter_lab.py` | promote | overlap with `xvn strategy set-filter`; merge with `--validate` mode |
| `xvn_author_strategy.py` | demote | mutates via dashboard API only because remote CLI denies it; record in cli-non-surfaced.md as "remote-write helper" |
| `xvn_scenario_builder.py` | demote | same shape as the strategy author |
| `xvn_investigate.py` | promote | becomes `xvn doctor investigate --strategy <id>` / `--run <id>` |
| `xvn_memory_report.py` | promote | becomes `xvn memory propose --run <id>` (read-only; writes stay manual) |

The promote/demote calls in this table are the **proposed** dispositions; the
`xvn-script-overlap-resolve` track (see §Tracks) executes them after a final
review pass per helper.

### Decision 7 — Verb-name regularity

The current surface mixes top-level hyphenated verbs (`show-decision`,
`show-briefing`, `show-metrics`, `fire-trade`, `close-position`,
`run-setup`, `ab-compare`) with resource-grouped subverbs (`xvn agent ls`,
`xvn eval show`, `xvn scenario show`). An agent has to remember which
pattern each resource uses. Resolution:

- **Add resource-grouped aliases** for every top-level hyphenated verb.
  `xvn show-decision` keeps working; `xvn decision show <id>` is the new
  canonical form, with `xvn show-decision` documented as a deprecated alias.
- Same treatment for `show-briefing` → `xvn briefing show`,
  `show-metrics` → `xvn metrics show`, `run-setup` → `xvn cycle run`,
  `fire-trade` → `xvn trade fire`, `close-position` → `xvn trade close`,
  `ab-compare` → `xvn eval ab-compare`.
- `xvn portfolio` stays — it reads as a noun, not a verb. (Optionally lift
  to `xvn venue portfolio` for symmetry with the `venue.rs` module, but the
  current shape is fine.)
- The deprecation window is six months. After that, the aliases are
  removed and `AGENTS.md` carries a one-time migration note. Tracked in
  `FOLLOWUPS.md`.

This is a non-breaking change today; the breakage is purely the eventual
removal of the aliases.

## Rung-5 insight verbs (new surface)

The press's "Creativity Ladder" puts wrap-the-endpoint at Rung 1 and
behavioral / compound insights at Rung 5. xvision is already at Rung 3–4
(SQLite flight recorder; `xvn eval`, `xvn metrics`, `xvn gate`). Rung 5
candidates that join across `cycles`, `briefings`, `decisions`,
`risk_outcomes`, `executions`, `traces`:

1. **`xvn cycle stale --since <duration>`** — cycles where the briefing
   landed but no decision / execution did. Joins `cycles` ⋈ `decisions` ⋈
   `executions`. Output: list of `cycle_id`, age, stage where it dropped.
2. **`xvn strategy health <strategy_id>`** — composite over equity
   drawdown, decision rate, risk-veto rate, error rate, average tokens
   per cycle. Joins strategy → cycles → decisions → risk_outcomes →
   executions → traces. Output: a single JSON object with the rollup.
3. **`xvn strategy similar <strategy_id> [--k N]`** — nearest neighbors in
   the agent library / lineage graph. Useful for "this lost money, what
   else from this lineage did the autoresearcher promote."
4. **`xvn provider load --since <duration>`** — call counts × cost per
   provider over a window. Reads from the observability tables.
5. **`xvn run bottleneck <run-id>`** — per-stage latency / token-cost
   rollup across briefings, decisions, risk for one run.
6. **`xvn risk patterns --since <duration>`** — most common veto reasons,
   ranked. Reads from `risk_outcomes`.
7. **`xvn obs friction --since <duration>`** — heuristic surface over
   agent-run traces: flags called with bad args before succeeding,
   repeated `MANUAL.md` lookups, repeated failed `xvn doctor` invocations.
   The press's `/printing-press-amend` analogue. Speculative; spike before
   committing.

**First spike: `xvn strategy health`.** Highest signal-to-effort ratio:
the join is straightforward, the output is a single rollup, and "is this
strategy doing OK?" is a question agents and operators both ask multiple
times a day. If the spike earns its keep, the remaining six get their own
tracks. If it doesn't, the Rung-5 program goes back on the shelf.

## Anti-pattern audit (one-time pass)

The press names five anti-patterns worth a one-time grep across `xvn`:

1. **Dead flags** — clap-declared flags with no code path. Audit by
   walking every `commands/*.rs` and matching declared flags against
   usage. Tracked as the `cli-dead-flag-audit` track.
2. **Ghost SQLite tables** — write path without read path or vice versa.
   Audit by enumerating every `INSERT` and `SELECT` in
   `crates/xvision-data/src/` and pairing them. Worth adding as a
   `scripts/board-lint.sh` check post-audit.
3. **Hallucinated paths** — URLs and file paths in help text that don't
   resolve. CI check via `cargo run -p xvision-cli -- --help-all | grep
   -oE 'https?://[^ )]+' | xargs -I{} curl -sIo /dev/null -w "%{http_code} {}\n" {}` plus a path-existence pass.
4. **Generic Upsert** — single-method `Upsert()` across heterogeneous
   tables. xvision's `xvision-data` uses per-domain methods already;
   audit confirms.
5. **Verb-name irregularity** — covered by Decision 7.

The audit lands as a single report (`docs/superpowers/audits/2026-05-XX-cli-anti-pattern-sweep.md`), not as inline fixes. Fixes are tracked separately.

## Tracks

This spec decomposes into nine follow-on tracks. Dependencies are between
brackets; `→` means "blocks."

1. **`agents-md-cookbook`** — Write `AGENTS.md` and `docs/cookbook.md`;
   hoist sections out of README. No code changes. (independent)
2. **`cli-universal-flags`** — Implement `AgentFlags` clap group in
   `crates/xvision-cli/src/flags.rs`, wire into every existing verb that
   emits structured output. Includes the auto-JSON-on-pipe trigger from
   Decision 2. Touches every file in `crates/xvision-cli/src/commands/`.
   → `cli-compact-projection-tables`
3. **`cli-compact-projection-tables`** — Define the `--compact` field
   projection per verb. One table per command in `commands/*.rs`.
   Acceptance: `xvn eval show --compact` returns ≤25% of the byte count
   of `xvn eval show --json`.
   Blocked by: `cli-universal-flags`.
4. **`cli-data-source-selector`** — Implement Decision 3; collapse
   `ab-compare`'s `--bars` mutual exclusion into the unified selector.
   Touches `commands/ab_compare.rs`, `commands/bars.rs`,
   `commands/provider.rs`.
5. **`cli-doctor-auth`** — Implement Decision 5; new `xvn doctor auth`
   subcommand with the credential-surface enumeration.
6. **`cli-list-narrowing-hints`** — Add the "Showing N. To narrow: add
   --limit / --json --select / filter flags" stderr line to every list
   verb: `eval list`, `bars ls`, `scenario ls`, `strategy ls`,
   `agent ls`, `experiment ls`, `memory list`, `provider list`.
   Blocked by: `cli-universal-flags` (so `--select` exists when the
   hint references it).
7. **`cli-verb-aliases`** — Implement Decision 7; add resource-grouped
   aliases for every hyphenated top-level verb, mark originals
   deprecated.
8. **`xvn-script-overlap-resolve`** — Execute the promote/demote table
   from Decision 6, one helper at a time. Each demotion lands as a
   `cli-non-surfaced.md` entry; each promotion lands as a CLI verb.
9. **`cli-rung5-strategy-health-spike`** — Implement `xvn strategy
   health <id>` per the Rung-5 section. Spike-scoped: ship the verb,
   re-evaluate the remaining six Rung-5 candidates after two weeks of
   real use.

Parallel: **`cli-anti-pattern-sweep`** — one-shot audit per the
anti-pattern audit section. Output is a single audit doc; no inline
fixes in this track.

## Acceptance

This spec is accepted when:

- The seven decisions in §Decisions are signed off, or the user has
  marked specific items for rework.
- The nine tracks are filed into `team/board.md` with owning agents and
  the dependency graph respected.
- `AGENTS.md` and `docs/cookbook.md` exist (track 1 lands first; it
  unblocks downstream tracks by being the canonical place to document
  the new flags and verbs as they ship).

Each track has its own contract under `team/contracts/<track>.md` per
the standard process (`team/CONDUCTOR.md`). The conductor decomposes
one track at a time per the wave-cadence rules; no freelancing.

## Out of scope (for the record)

The following press primitives are **deliberately not adopted**, with
rationale:

- **Non-Obvious-Insight gate, Ecosystem Absorb manifest, browser-sniff
  path, two-tier scorecard, proof-of-behavior gates** — all
  generator-side. xvision isn't generating a CLI; it owns one.
- **`--no-cache` as a standalone flag** — replaced by Decision 3's
  `--data-source` selector.
- **`/printing-press-amend` slash command** — the friction-mining idea
  is captured as the `xvn obs friction` Rung-5 candidate; the slash
  command form is not a fit for xvision's skill layout.
- **Public catalog (printingpress.dev) analogue** — xvision is a single
  product, not a generator producing many CLIs; nothing to catalog.

## Adoption note

The CLI patterns in this spec are not new house style — they extend the
two existing house artifacts (`exit.rs`'s exit-code convention and
`io.rs`'s channel discipline) into a fuller agent-CLI contract. Where
this spec conflicts with a future house decision, the house decision
wins; this spec is a starting point, not a constitution.
