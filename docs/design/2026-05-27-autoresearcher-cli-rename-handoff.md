# CLI rename handoff — autoresearcher plain-language verbs and flags

> For: backend engineer picking up the CLI rename
> Date: 2026-05-27
> Source of truth: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`

## TL;DR

Rename a set of CLI verbs and flags on `xvn autoresearch`,
`xvn memory`, and `xvn flywheel` to use the locked operator-surface
names. Keep every renamed verb and flag as a hidden deprecation alias
for one release so existing operator scripts and the
`autoresearch-ops` skill continue to work while users migrate. Update
the public CLI surface snapshot test.

The CLI is the most invasive of the rename patches because it has
external consumers (operator shell scripts, the published skill,
documented examples in MANUAL.md). The deprecation aliases keep those
consumers working until we cut a release that drops them.

## Files in scope

- `crates/xvision-cli/src/commands/autoresearch.rs` — verb + flag
  renames, help text rewrites
- `crates/xvision-cli/src/commands/flywheel.rs` — help text only (no
  verb/flag renames in scope; section 7 of the lock keeps these names)
- `crates/xvision-cli/src/commands/memory.rs` — `promote` → `activate`
  consolidation, `demote` → `retire` rename, `--tier` → `--kind`,
  `--promotion-state` → `--status`, `--attest-null-window` →
  `--confirm-no-cutoff`, help text rewrites
- `crates/xvision-cli/src/lib.rs` — if any top-level help text
  references renamed verbs
- `crates/xvision-cli/tests/cli_surface_snapshot.json` — the
  authoritative public-CLI snapshot
- `crates/xvision-cli/tests/autoresearch_cli.rs` — integration tests
  that assert on verb/flag names

## Files NOT in scope

- Any file under `crates/xvision-engine/src/autoresearch/` — Rust
  types, internal modules, and engine API stay developer-surface.
- SQLite migrations — schema names stay.
- `crates/xvision-engine/src/api/autoresearch.rs` — the engine API is
  developer-surface; only the CLI translates to operator vocabulary.
- The dashboard SSE wire names — handled by the SSE registry handoff.

## The verb renames

Each renamed verb keeps the old name as a `#[command(alias = "...",
hide = true)]` clap alias for one release. The alias prints a
deprecation note to stderr on use: "Note: `xvn ... <old>` is now
`xvn ... <new>`; the old form still works in this release and will be
removed in the next."

| Subcommand path | Today | Rename to | Deprecation alias |
|---|---|---|---|
| `xvn autoresearch promote` | (current) | `xvn autoresearch activate` | `promote` hidden |
| `xvn autoresearch demote` | (current) | `xvn autoresearch retire` | `demote` hidden |
| `xvn memory promote --ids ... --text ...` | (current — Observation cohort → Pattern) | `xvn memory distill --ids ... --text ...` | `promote` hidden alias forwards to `distill` |
| `xvn memory activate <id>` | (current — single-Pattern form) | `xvn memory activate <id>` (unchanged) | — (kept separate from `distill` per lock amendment) |
| `xvn memory demote` | (current) | `xvn memory retire` | `demote` hidden |

Verbs that DO NOT rename per the lock: `xvn autoresearch run`,
`xvn autoresearch gate`, `xvn autoresearch inspect`,
`xvn autoresearch ls`, `xvn memory ls`, `xvn memory namespaces`,
`xvn memory show`, `xvn memory add-pattern`, `xvn memory rm`,
`xvn memory forget`, `xvn memory undo-forget`, `xvn flywheel status`,
`xvn flywheel velocity`, `xvn flywheel lineage`.

### Memory verb structure (per lock amendment 2026-05-27)

Today there are two separate verbs:
- `xvn memory activate <id>` — flips a single staged Pattern to active
- `xvn memory promote --ids ... --text ...` — distills Observations
  into a new staged-or-active Pattern

The lock was amended 2026-05-27 to keep these as separate verbs:
- `xvn memory activate <id>` stays unchanged (single-Pattern form).
- `xvn memory promote --ids ... --text ...` is renamed to
  `xvn memory distill --ids ... --text ...`.
- `xvn memory promote` ships as a hidden alias for
  `xvn memory distill` for one release.

See "Amendments" section in the terminology lock for the rationale.

## The flag renames

| Subcommand | Old flag | New flag | Alias |
|---|---|---|---|
| `xvn autoresearch gate` | `--gate-epsilon` | `--min-improvement` | old hidden |
| `xvn autoresearch gate` | `--parent-day-score` | `--baseline-today-score` | old hidden |
| `xvn autoresearch gate` | `--child-day-score` | `--candidate-today-score` | old hidden |
| `xvn autoresearch gate` | `--parent-holdout-score` | `--baseline-untouched-score` | old hidden |
| `xvn autoresearch gate` | `--child-holdout-score` | `--candidate-untouched-score` | old hidden |
| `xvn autoresearch gate` | `--min-delta` | `--min-improvement` | old hidden (consolidates with the epsilon rename — both flags meant the same thing on different code paths) |
| `xvn memory ls` | `--tier` | `--kind` | old hidden |
| `xvn memory ls` | `--promotion-state` | `--status` | old hidden |
| `xvn memory add-pattern` | `--attest-null-window` | `--confirm-no-cutoff` | old hidden |

Flags that DO NOT rename per the lock: `--namespace`, `--agent`,
`--scenario`, `--run`, `--limit`, `--offset`, `--json`,
`--pattern-text`, `--active`, `--embedding-json`, `--training-end`,
`--operator-initials`, `--force`, `--metric`, `--baseline-score`,
`--candidate-score`, `--gate-reason`, `--finding-text`,
`--qualitative-finding-json`, `--finding-blinded-metrics`,
`--finding-model`, `--judge-model`, `--judge-token-cost`,
`--promote-if-pass` (the flag inside `gate` — name stays even though
`promote` is being renamed elsewhere, because the semantic is "mark
this run as activated if it passes"; consider renaming to
`--activate-if-pass` for consistency, flag back if you do).

## Help text rewrites

Every command and flag with a `help = "..."` clap attribute needs the
help text re-read for jargon. Specific edits:

### `xvn autoresearch` subcommands (full help text proposals)

`xvn autoresearch run` description:
- Today: "Distill recent Observations into a staged Pattern."
- New: "Distill recent Observations into a candidate Pattern. The
  Pattern enters staged status; use `xvn autoresearch gate` to
  evaluate it, then `xvn autoresearch activate` to put it into use."

`xvn autoresearch gate` description:
- Today: "Record numeric gate and blind Finding for a staged Pattern."
- New: "Record the gate decision (Kept or Dropped) for a candidate
  Pattern, based on its score on today's data and on an untouched
  test period. The qualitative finding is recorded blind to the
  numeric scores."

`xvn autoresearch activate` description (renamed from `promote`):
- New: "Activate a candidate Pattern from an autoresearch run, making
  it available for recall during decisions."

`xvn autoresearch retire` description (renamed from `demote`):
- New: "Retire a Pattern produced by an autoresearch run. Soft-delete
  with a grace window; restore via `xvn memory undo-forget`."

Flag help on `xvn autoresearch gate`:
- `--min-improvement`: "Minimum improvement (Sharpe gain) required on
  both today's score and the untouched-period score for the gate to
  return Kept."
- `--baseline-today-score`: "Baseline strategy's score on today's
  data."
- `--candidate-today-score`: "Candidate strategy's score on today's
  data."
- `--baseline-untouched-score`: "Baseline strategy's score on the
  untouched test period."
- `--candidate-untouched-score`: "Candidate strategy's score on the
  untouched test period."

### `xvn memory` subcommands

`xvn memory ls` flag help:
- `--kind`: "Filter by memory kind: `observation` (auto-captured) or
  `pattern` (operator-attested or distilled)."
- `--status`: "Filter Patterns by status: `active` (in recall),
  `staged` (awaiting gate), `forgotten` (soft-deleted)."

`xvn memory add-pattern` flag help:
- `--confirm-no-cutoff`: "Required when omitting `--training-end`.
  Records explicit operator sign-off that this Pattern has no
  training cutoff and may be recalled in every scenario."

`xvn memory activate` description (unchanged from current):
- "Activate a staged Pattern by id, putting it into recall. To
  produce a Pattern by distilling Observations, use
  `xvn memory distill`."

`xvn memory distill` description (renamed from `promote`):
- "Distill Observation rows into a staged or active Pattern. Each
  contributing Observation must resolve to the same namespace unless
  `--namespace` is set explicitly."

`xvn memory retire` description (renamed from `demote`):
- New: "Retire an active or staged Pattern by id. Soft-delete with a
  grace window."

## Human-readable output strings

The CLI also prints status lines like
`autoresearch run {id} gate_verdict=passed (active)` — these need
operator-surface vocabulary too:

| Today | Replace with |
|---|---|
| `gate_verdict=passed (active)` / `gate_verdict=failed (staged)` | `gate decision: Kept (status: active)` / `gate decision: Dropped (status: staged)` |
| `autoresearch run {id} activated pattern {pattern_id}` | (keep — "activated" is now the verb) |
| `autoresearch run {id} demoted pattern {pattern_id}` | `autoresearch run {id} retired pattern {pattern_id}` |
| `xvn memory ls` table column header `tier` | `kind` |
| `xvn memory ls` table column header for promotion_state field | `status` |
| `xvn flywheel velocity` output `patterns_promoted: N` | `patterns_activated: N` |
| `xvn flywheel velocity` output `patterns_demoted: N` | `patterns_retired: N` |
| `xvn flywheel velocity` output `optimized_child_agents: N` | `new_versions_trained: N` |
| `xvn flywheel velocity` output `average_lineage_depth: N` | `average_generations_deep: N` |
| `xvn flywheel lineage` row `gate verdict={verdict}` | `gate decision: Kept`/`Dropped` |
| `xvn flywheel lineage` row `delta_dev={value} delta_holdout={value}` | `validation improvement: {value} · untouched improvement: {value}` |

Note: JSON output (when `--json` is passed) keeps the developer-surface
field names. The renames above only apply to the human-readable text
output. This is important — operator scripts that pipe to `jq` rely on
the field names being stable. Document this distinction in the
top-level CLI help if it isn't already.

## The CLI surface snapshot test

`crates/xvision-cli/tests/cli_surface_snapshot.json` is the
authoritative test of the public CLI shape. Update it to reflect the
new verbs and flags. The diff should show:

- New verbs added: `autoresearch activate`, `autoresearch retire`,
  `memory activate` (extended), `memory distill`, `memory retire`
- Old verbs marked hidden (still present, but with `hidden: true`):
  `autoresearch promote`, `autoresearch demote`, `memory promote`,
  `memory demote`
- Renamed flags on `autoresearch gate`: hidden aliases for the old
  names, new names visible
- Renamed flags on `memory ls` and `memory add-pattern`: same

If the snapshot test framework supports "this is a deprecation"
metadata, use it. Otherwise add a comment in the JSON (or a sibling
.md file) that lists which entries are aliases.

## Acceptance criteria

1. Every rename in the tables above is implemented in the CLI.
2. Every old verb and flag is preserved as a hidden alias with a
   stderr deprecation note on use.
3. The CLI surface snapshot test is updated and passes.
4. `crates/xvision-cli/tests/autoresearch_cli.rs` is updated to
   assert on the new verb and flag names (the existing assertions
   should be moved to new ones; do not delete the alias tests — add a
   parallel test that exercises the alias path to confirm
   backward-compat).
5. `cargo test -p xvision-cli` passes.
6. `xvn --help`, `xvn autoresearch --help`, `xvn memory --help`,
   `xvn flywheel --help`, and the help of every individual subcommand
   reads cleanly with the new vocabulary — no remaining instances of
   `epsilon`, `holdout` (in user-facing surfaces; "Untouched test
   period" is fine), `promotion`, `demote`, `mutation`, `mutator`.
7. `cargo run -p xvision-cli -- autoresearch promote --help` prints
   the deprecation note and forwards to `activate`'s logic.

## Test paths

- `crates/xvision-cli/tests/cli_surface_snapshot.json` — update
- `crates/xvision-cli/tests/autoresearch_cli.rs` — update + add
  alias-path tests
- Any clap unit tests in `crates/xvision-cli/src/commands/*.rs`
- Manual smoke: run `xvn autoresearch --help` and read it like an
  operator who's never used the tool

## Things to push back on

- `--promote-if-pass` flag inside `xvn autoresearch gate` —
  recommend renaming to `--activate-if-pass` for consistency with
  the verb rename. Flag back if you make this change so we update
  the lock.
- `--min-improvement` consolidates both `--gate-epsilon` and
  `--min-delta` because they meant the same thing on parallel code
  paths. If they actually had different semantics that the audit
  missed, flag back.
- "Validation improvement" for `delta_dev` in human output (line in
  the table above) — `delta_dev` was named for the dev split of the
  train/dev/holdout. The lock renames the split to
  Training/Validation/Untouched test, so `delta_dev` →
  `validation_improvement` follows. If the dev split is called
  something else in the actual rendered output, adjust.

## Rollout sequence

1. Land the rename PR with deprecation aliases.
2. One release cycle later, drop the aliases.
3. The skills-and-docs sweep handoff (separate doc) updates
   `.claude/skills/xvision/autoresearch-ops/SKILL.md` to use the new
   verbs immediately — that skill is the official command-line
   reference and shouldn't show deprecated forms.

## Reference

- Terminology lock: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
- Audit context: `docs/superpowers/notes/2026-05-27-autoresearcher-plain-language-audit.md`
- Project-wide terminology note: `/CLAUDE.md` §Terminology → "Operator-facing names (autoresearcher subsurface)"
