# Printing Press review — `xvn` CLI / skill package

**Date:** 2026-05-11
**Reviewer model:** mvanhorn/printing-press-library + cli-printing-press
**Scope:** the v0.2 `xvn` CLI surface, focused on the new `xvn skill` (Plan 2b)
package and the adjacent `xvn strategy` / `xvn eval` surfaces it composes with.

---

## What Printing Press optimizes for

The Printing Press is opinionated about what makes a CLI valuable to *AI
agents* (not humans). Distilled, the doctrine is:

1. **NOI (Non-Obvious Insight).** Every CLI starts with a one-sentence
   reframe: "X isn't just <obvious thing>. It's <non-obvious thing>. Every
   <data point> is a signal about <hidden truth>." Phase 0 cannot complete
   without one. The NOI determines which compound commands ship.
2. **Local-first SQLite mirror.** High-gravity resources get a domain-specific
   SQLite table (not JSON blobs), FTS5 search, incremental sync. `sync`
   pulls, `search` finds in milliseconds, `sql` lets power users query.
3. **Compound commands.** Once data is local, ship verbs that join across
   resources and analyze history — `stale`, `health`, `bottleneck`,
   `reconcile`, `diff`, `drift`. A stateless wrapper literally cannot do
   this; that is the moat.
4. **Agent-native by default.**
   - Tables in TTY, **auto-JSON when piped** (no `--json` flag).
   - `--compact` drops to high-gravity fields only (60-80% fewer tokens).
   - **Typed exit codes**: `0` ok, `2` usage, `3` auth, `4` not-found,
     `5` upstream, `7` conflict. Agents self-correct without parsing
     error text.
   - `--dry-run` for safe exploration on every mutating command.
   - Stable `--data-source auto|local|live` flag for store-backed CLIs.
5. **Dual interface.** One spec → Cobra CLI (`<api>-pp-cli`) **and** an MCP
   server (`<api>-pp-mcp`). Same client, store, auth. Zero duplication.
6. **Verified, not vibes.** Four mechanical proofs at print time:
   scorecard, dogfood, proof-of-behavior, live API smoke test.
7. **Naming.** `<api>-pp-cli` binary, `pp-<api>` skill, slash-skill is
   `/pp-<api>`. Conventional, predictable, agent-discoverable.
8. **Sources & Inspiration.** Every README credits prior tools studied
   during research.

The press also has a strong house style: short verbs (`ls`, `new`, `tail`,
`search`, `sync`, `sql`, `stale`, `health`), low ceremony, exit codes do
the talking.

---

## Where `xvn` aligns out of the box

- **Dual interface (5).** ✅ `xvn` CLI + `xvn-mcp` server share `xvision_engine::authoring` so the dispatch logic is single-sourced. The MCP advertises the same 7 + 3 verbs the CLI exposes; this is exactly the PP "one spec, two binaries" pattern.
- **Local-first store (2 — partial).** ✅ Strategies persist to `$XVN_HOME/strategies/<id>.json`, decision flight-recorder + eval runs persist to `$XVN_HOME/store.db` (SQLite). The skills surface persists to `$XVN_HOME/skills/<name>.md`. Local data lives in three different shapes (JSON, SQLite, raw markdown), but the *principle* is met.
- **Compound commands exist (3 — partial).** ✅ `xvn strategy run`, `xvn ab-compare`, `xvn eval compare` all join across resources rather than wrap a single endpoint. `xvn skill attach` is a compound mutation (loads bundle + skill, mutates, saves).
- **Short verbs (7).** ✅ `xvn skill {new | ls | attach}` is in the PP house style. Same for `xvn strategy {new | ls | show | run}`.

---

## Where `xvn` diverges — and whether to fix

### Hard misses (worth borrowing)

1. **No auto-JSON when piped, no `--compact`.** `xvn strategy show` always pretty-prints JSON. `xvn skill ls` always prints one-name-per-line. Neither detects stdout TTY vs pipe. An agent calling `xvn skill ls | jq` already gets one-per-line which is jq-friendly, but for tables (e.g. `xvn strategy templates` prints `name display_name`), there's no JSON mode at all. **Recommend:** add a `--json` flag (or auto-detect) on `templates`, `ls --long`, and any future "show me everything" verbs.
2. **No typed exit codes.** Every error currently returns clap's default `2` (usage error) or anyhow's `1`. PP would have us return `4` for "strategy not found", `7` for "skill name collides with existing", `2` only for malformed args. Agents reading the exit code could self-correct (e.g., re-fetch strategy list before retrying attach). **Recommend:** add a small `XvnExit` enum with `From<anyhow::Error>` mapping. Apply incrementally — doesn't need to be a v0.2 blocker.
3. **No `--dry-run` on mutations.** `xvn skill attach` writes to disk on every call. `xvn strategy new` always persists. PP's convention is `--dry-run` returns the would-be result without side effects. **Recommend:** `--dry-run` on `skill attach` and `strategy new` (cheapest wins).
4. **No NOI written down.** `xvn` does have one — *"a strategy isn't just a system prompt + model id; it's a 3-slot agent pipeline whose risk verdicts you can replay against historical bars"* — but it's not in any README header. PP would put this in the first paragraph of `crates/xvision-engine/README.md`. **Recommend:** add an NOI block to the engine + skills READMEs.

### Soft misses (skip in v0.2, revisit if PP becomes the house style)

5. **Single binary, not `xvn-pp-cli` / `xvn-pp-mcp`.** The PP convention is two binaries per API; we have `xvn` (CLI superset) and `xvn-mcp` (MCP only). Renaming would be a breaking change for no functional gain, and `xvn` is closer to the project's identity than `xvision-pp-cli`. **Recommend:** keep current names; document the convention deviation.
6. **Skills are markdown files, not SQLite rows.** PP would store skills in a `skills` table in `store.db` and ship `xvn skill search`, `xvn skill stale`. We chose markdown for byte-exact roundtrip + git-friendliness + author-readability. **Recommend:** keep markdown; add a thin sqlite index (`name, display_name, version, content_hash, attached_to[]`) once skill count crosses ~50, gated on real demand.
7. **No "verified, not vibes" pipeline at publish time.** PP runs scorecard / dogfood / proof-of-behavior / live smoke as a print step. We have `cargo test --workspace`. Different problem (we're not generating CLIs, we're hand-writing them) — adopting the four checks doesn't transfer cleanly. **Recommend:** skip; our equivalent is `cargo clippy -D warnings + cargo test + smoke`.
8. **Sources & Inspiration section.** Our READMEs don't credit prior art (OSShip skill format, `xvn`'s prior decisions). **Recommend:** add a 5-line "Inspired by" footer to `xvision-engine/README.md` and `xvision-skills/README.md` — cheap, polite, helpful.

### Active conflicts (PP would be wrong here)

9. **Strategy `id` is a ULID, not a slug.** PP convention is human-readable slugs (`linear-pp-cli`, `espn-pp-cli`). We ship ULIDs because strategies are *tokens* (mintable as NFTs in Plan 5), so the id has to be globally unique pre-mint. The CLI's `--name` flag is the human-readable display name; the ULID is the addressable id. **Don't change.**
10. **No `--data-source auto|local|live`.** PP store-backed CLIs flip between local SQLite mirror and live API. Strategies + skills are *intrinsically* local artifacts — there's no upstream to mirror. The flag would have nothing to mean. **Don't add.**

---

## Specific findings on the `xvn skill` surface (Plan 2b, this PR)

Read against PP house style:

| Item | PP would say | Plan 2b ships | Verdict |
|---|---|---|---|
| Verb names (`new`, `ls`, `attach`) | ✅ short, imperative | ✅ matches | OK |
| `--from-file` argument | PP prefers stdin for piping (`cat my-trader.md \| xvn skill new -`) so agents can compose | We require a file path | **Worth adding stdin support** — 3 lines of code, big agent ergonomics win |
| Error format (`anyhow` text) | PP wants typed exit codes + JSON-shaped errors on `--json` | We use anyhow Display | **Defer to whole-CLI exit-code rollout** |
| `xvn skill ls` output | PP would auto-JSON when piped; tabular when TTY | We always print one-per-line | **Acceptable** — one-per-line is the common-denominator format that `\| xargs -I{} ...` handles cleanly. Add `--long` if we ever surface display_name/version. |
| `xvn skill attach` mutates without confirmation | PP requires `--dry-run` for safe exploration | We don't have one | **Worth adding** in a follow-up; not a v0.2 blocker. |
| `xvn skill cat <name>` to print body | PP would ship this for free | Not in v0.2 | **Add when needed** — trivial, but skill files live at predictable paths so users can `cat $XVN_HOME/skills/<name>.md` directly. |
| `xvn skill rm <name>` | PP would ship | Not in v0.2 | **Defer** — operationally fine to `rm $XVN_HOME/skills/<name>.md` for now; revisit if/when we add markdown post-hooks. |

---

## Recommendations (ranked by leverage)

1. **Add an NOI line to the engine + skills READMEs.** Free, makes the value prop crisp for any agent or human reading the repo cold. Suggested:
   - `xvision-engine`: *"A strategy isn't just a system prompt — it's a 3-slot agent pipeline whose risk verdicts you can replay against any historical fixture and compare arm-for-arm."*
   - `xvision-skills`: *"A skill isn't just a prompt template — it's a portable, content-addressed override that promotes a slot's prompt + model + tool allowlist atomically, so an authored 'crypto trader' skill drops into any strategy without re-templating."*
2. **Add stdin support to `xvn skill new`.** `--from-file -` reads stdin. 3 lines, ~5 minutes, lets agents pipe `xvn` + an LLM together without a tmpfile dance.
3. **Add a `--dry-run` to `xvn skill attach` and `xvn strategy new`.** Returns the would-be JSON result, no disk write. Standard PP convention. Small.
4. **Plan a `XvnExit` enum follow-up across the whole CLI.** Bigger surface change, separate plan. Track as a follow-up issue.
5. **Skip the rest** for v0.2. The remaining PP conventions either don't apply to our domain (data-source flag) or aren't worth the breaking change (binary renaming).

---

## TL;DR

Plan 2b's `xvn skill` surface already matches PP's *style* — short verbs,
predictable persistence, dual MCP/CLI surface. It misses three small but
worthwhile PP idioms (stdin in, `--dry-run`, NOI in README) and one larger
cross-CLI investment (typed exit codes) that should land as a follow-up
plan, not part of this PR. None of the gaps justify churning the v0.2
release; all are additive.

The genuine cross-cutting opportunity is the **typed exit codes** —
agents reading `4 → not found` instead of grepping anyhow output is the
single highest-leverage Printing Press idea for the broader `xvn` CLI.

---

## Sources

- https://github.com/mvanhorn/printing-press-library — catalog + readme
- https://github.com/mvanhorn/cli-printing-press — the printer + spec
- https://printingpress.dev — live catalog
