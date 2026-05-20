# Intake — 2026-05-20 — User + agent docs: refresh + wire in-app wiki

Operator-driven (Ed, 2026-05-20). With the CLI agent-research-workbench
waves A → D shipped (atomic strategy create, validate, scenario
classify/set-regime/select/inspect, eval batch run/wait/compare,
strategy hypothesis manifest, experiment ledger + orchestrator,
behavior summary, baseline auto-comparison, regime labels, `xvn agent
get`) the in-app docs have drifted hard and most of the long-form
documentation already living in `docs/` is invisible to the dashboard.

Two parallel gaps:

1. **Content drift.** The five baked pages
   (`crates/xvision-dashboard/src/routes/docs/content/`) describe the
   pre-wave-A surface. None of the new verbs, the `--json` posture, the
   experiment ledger, scenario regime metadata, hypothesis manifest, or
   baseline auto-comparison are documented. Operators following the
   in-app docs to drive `xvn` today will hit "verb not found" within the
   first agent loop.
2. **Surface gap.** There is no `docs/xvnwiki/` directory and the `/docs`
   route bakes exactly five `include_str!`'d files into the binary. The
   useful long-form material under `docs/` (MANUAL.md, runbook/,
   strategies/templates/, design/, QA notes) is not surfaced anywhere
   in the SPA. The dashboard cannot answer "how do I do X" beyond the
   five baked pages.

Single sentence framing:

> "Make the in-app docs answer both 'how do I use xvision' (operator)
> and 'how do I drive xvision' (agent), and let curated `docs/` content
> reach the dashboard without rebuilding the binary."

This intake is **content + plumbing**, not a UX overhaul. Tracks 1–4
are pure markdown deltas; track 5 is a small backend change (filesystem
load + index manifest) with conservative defaults; track 6 is the agent-
facing companion most likely to need a spec under
`docs/superpowers/specs/` before contracts open.

## Already-built building blocks (sanity check before decomposition)

Verified on 2026-05-20 in this worktree:

- `/api/docs/index` + `/api/docs/page/:slug` at
  `crates/xvision-dashboard/src/routes/docs/mod.rs` — Axum handlers,
  `PAGES: &[(slug, title, body)]` baked via `include_str!`. Five entries.
- `/docs` route at `frontend/web/src/routes/docs/index.tsx` — two-pane
  layout, client-side fuzzy filter, lazy-loaded. Already production-
  shaped; renders any markdown the backend returns.
- Baked content at `crates/xvision-dashboard/src/routes/docs/content/{quickstart,strategies,scenarios,eval-runs,cli-reference}.md`.
  All five start with an `# h1` and pass the `every_baked_page_is_non_empty`
  guard test.
- Long-form docs that currently live only on disk:
  - `MANUAL.md` (586 lines) — Tier 2/3 operator tasks (Alpaca paper,
    on-chain identity, remote CLI via Tailscale).
  - `docs/cli-non-surfaced.md` — deliberate `xvn` exclusions + reasoning.
  - `docs/runbook/{dashboard-auth.md,observability-otel.md}` — ops
    runbooks.
  - `docs/strategies/templates/` — strategy template references.
  - `docs/superpowers/specs/` — design history (mostly internal, but the
    last 3 months hold real "why we built it this way" context).
  - `docs/QA/2026-05-11-xvision-web-qa.md` — single QA pass record.
- `docs/xvnwiki/` does **not** exist in repo. Treat this intake as the
  proposal to create it as the curated wiki source-of-truth directory.
- New CLI verbs landed since the baked `cli-reference.md` was written
  (each verified by reading the command module in
  `crates/xvision-cli/src/commands/`):
  - `xvn strategy create --atomic` + `--hypothesis` flags
    (`strategy.rs`, intake tracks #1, #7).
  - `xvn strategy validate <id> --scenario <id> --json` with
    `eval_ready` verdict (`strategy.rs`, intake track #2).
  - `xvn scenario classify`, `set-regime`, `select`, `inspect`
    (`scenario.rs`, intake tracks #5, #6, #12).
  - `xvn eval batch run|status|compare` with `--wait`, `--review-with`,
    `--markdown`, `--json` (`eval/batch.rs`, `eval/compare_format.rs`,
    intake tracks #3, #4).
  - `xvn eval review <run-id> --agent <profile>` (`eval/review.rs`).
  - `xvn experiment` + `xvn experiment run` orchestrator
    (`experiment.rs`, `experiment_run.rs`, intake tracks #8, #9).
  - `xvn agent get <id>` (`agent.rs`, q15 object-json contract).
- Recent renames the docs still spell the old way:
  - `intern` → "default agent" in wizard/rail/settings (in progress —
    keep both terms documented during the cross-over window per
    `project_intern_to_default_agent_rename` memory).
  - `--setups` → `--cycles` on `xvn ab-compare`.
- Agent-facing daemon `xvision-agentd` (TypeScript, UDS server,
  tool-shim, NDJSON event stream) is undocumented in `/docs` despite
  shipping with the binary. Lives at `xvision-agentd/src/`.

## Findings → tracks (decomposition for conductor refinement)

### Content refresh (highest leverage; pure markdown deltas, zero
backend risk)

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 1 | P1 | `docs-cli-reference-refresh` | Rewrite `crates/xvision-dashboard/src/routes/docs/content/cli-reference.md` to cover every verb shipped in waves A–D: `strategy create --atomic/--hypothesis`, `strategy validate --scenario`, `scenario {classify,set-regime,select,inspect}`, `eval batch {run,status,compare}` with `--wait/--review-with/--markdown`, `eval review`, `experiment {create,list,show,run}`, `agent get`. Mark `--setups` as removed alias; mark legacy `intern` role as deprecated nomenclature for "default agent" |
| 2 | P1 | `docs-strategies-refresh` | Update `content/strategies.md` to document hypothesis manifest fields (family / target+avoid regimes / asset+timeframe assumptions / entry+exit+risk logic) and atomic-create flow. Add cross-link to experiments page |
| 3 | P1 | `docs-eval-runs-refresh` | Update `content/eval-runs.md` to cover batch runs, baseline auto-comparison columns (`relative_to_buy_hold`, etc.), behavior summary (flat_rate / trades_opened / avg_bars_held / reentries_after_loss / exits_on_invalidation / primary failure mode), and the `--review-with` auto-review path |
| 4 | P1 | `docs-scenarios-refresh` | Update `content/scenarios.md` to cover regime labels (trend / volatility / liquidity / chop_score / event_type / directional_persistence), classify+set-regime workflow, scenario select (same-decisions, max-decisions, regime filters), inspect card output |

These four cover all surface drift introduced by waves A–D. They are
markdown-only and can ship as a single PR — recommend one wave.

### New baked pages (additive; complete the in-app picture without
unbaking yet)

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 5 | P1 | `docs-experiments-page-new` | New baked page `content/experiments.md` covering hypothesis → strategy → scenario set → `xvn experiment run` → result_json → next iteration. Explain the experiment ledger row schema, conclusion / next_recommendation fields, and how it composes with batch + compare + review. Register it in the `PAGES` array between `eval-runs` and `cli-reference` |
| 6 | P1 | `docs-agents-page-new` | New baked page `content/agents.md` documenting the reusable agent library — what `agent_id` is, how `AgentSlot` (prompt + model + skills + temperature + max_tokens + prompt_version) resolves, how strategies reference agents by id, and how to fetch one with `xvn agent get`. Cross-link from strategies page |
| 7 | P2 | `docs-providers-and-brokers-page-new` | New baked page `content/providers.md` covering provider config (`$XVN_HOME/config/default.toml`), `xvn provider ls/test`, the brokers tab posture (separate from providers per the locked-in design decision), and the auth ladder (env > config > prompt). Cross-link from quickstart |

### Filesystem-loaded wiki (the actual "docs in web UI" plumbing)

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 8 | P1 | `docs-wiki-filesystem-source` | Promote the `/api/docs/*` handler from a hard-coded `PAGES` array to a manifest-driven load from `docs/xvnwiki/`. New layout: `docs/xvnwiki/index.toml` (ordered `[[page]] slug=… title=… path=…` entries) plus `docs/xvnwiki/*.md` files. Bake the manifest + content via `include_str!`/`include_dir!` at build time so the deployed image still has no runtime filesystem dependency. Acceptance: existing five pages continue to render under the same slugs; adding a new file to `docs/xvnwiki/` + a manifest entry surfaces it in `/docs` after rebuild. **No runtime filesystem read** — preserves the V2A onboarding contract that docs are baked |
| 9 | P2 | `docs-wiki-content-migration` | First curated `docs/xvnwiki/` payload beyond the five baked pages: a `manual.md` distilled from `MANUAL.md` (Tier 2 + Tier 3 operator tasks), `runbook/dashboard-auth.md` and `runbook/observability-otel.md` pulled into the wiki tree, `remote-cli.md` consolidated from `MANUAL.md` + the `scripts/xvn-remote.py` posture. Each page rewritten for the in-app reader — no internal-only references, no FOLLOWUPS cross-refs. Depends on #8 |
| 10 | P2 | `docs-wiki-section-headers` | Sidebar grouping in `/docs`: introduce `section` field in `index.toml` (`Quickstart`, `CLI`, `Concepts`, `Operator`, `Agent`) and render the sidebar as collapsed section headers + page list. Search filter stays flat. Depends on #8 |

### Agent-facing documentation (companion track; likely needs a spec)

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 11 | P1 | `docs-agent-driving-guide` | New baked page `content/driving-xvn-as-an-agent.md` (or wiki entry if #8 has landed): the explicit contract for an agent driving `xvn`. Covers: `--json` everywhere, exit-code semantics, atomic verbs vs multi-step, `--wait` polling vs SSE watch, idempotency on retry/rerun, error classes (`provider_unavailable`, `broker_rejected`, …), how to read `eval_ready` + `warnings[]`, the experiment-loop verb chain (`strategy create → validate → scenario select → experiment run → eval compare → eval review`), and the don'ts (no shelling around the CLI to glue state; no MCP without authorization). Source is the agent feedback already captured in `team/intake/2026-05-19-cli-agent-research-workbench.md` |
| 12 | P2 | `docs-agentd-surface-page` | New baked page (or wiki entry) documenting the `xvision-agentd` daemon: UDS server location, NDJSON event schema, tool-shim registry, session lifecycle (`session-start`, `session-step`, `tool-invoke`, `session-store`), runtime health probe. Acceptance: an agent reading only this page can write a client that streams a session end-to-end. May need a spec under `docs/superpowers/specs/2026-05-2x-agentd-surface.md` first; flag for conductor |
| 13 | P3 | `docs-mcp-surface-page` | New baked page documenting the MCP surface — which tools are exposed, what each one wraps, what's deliberately excluded (cross-link to `docs/cli-non-surfaced.md` content). Same "no on-chain side effects, no real-money orders" footgun bar |

### Discoverability + freshness

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 14 | P2 | `docs-route-deep-link` | Honor a `?slug=` query param on `/docs` so the chat rail / wizard / inspector can deep-link into a specific page (e.g. "see Experiments" on an empty experiments list). Acceptance: `/docs?slug=experiments` opens the experiments page; bare `/docs` still defaults to first index entry |
| 15 | P3 | `docs-freshness-staleness-guard` | Add a `last-reviewed: YYYY-MM-DD` frontmatter field to each wiki page and a CI check that fails if any page is older than 90 days OR if `cli-reference.md` was not touched in the same PR as a new top-level verb under `crates/xvision-cli/src/commands/`. Depends on #8 |
| 16 | P3 | `docs-render-codeblock-copy` | Add per-codeblock copy button + language badge to `DocsMarkdown`. Small ergonomic delta; agents reading docs over a streamed terminal don't need it, operators on the dashboard do |

---

## Recommended first wave

If conductor wants to ship one PR's worth: **#1 + #2 + #3 + #4 + #5 +
#11**. That's a pure-markdown content refresh that closes the drift
gap, adds the experiment story, and lays down the agent-driving
contract — no backend changes, no migrations, no infra risk. Bakes
cleanly via the existing `PAGES` array.

Track #8 (filesystem-loaded wiki) is the right second wave — it
unlocks #9, #10, and the entire long-form `docs/` corpus reaching the
dashboard without further code changes. Recommend it as its own track
with the explicit acceptance that no runtime filesystem read is
introduced (preserve `include_dir!`-at-build-time semantics).

Tracks #12 + #13 (agentd / MCP surface pages) are most likely to need
a spec round-trip before contracts open — flag them for spec authoring
even if conductor batches the rest.

## Status reconciliation — 2026-05-21

Most of this intake **already shipped** in commits between
2026-05-20 and 2026-05-21. Wiki source lives at
`crates/xvision-dashboard/wiki/` (not the proposed `docs/xvnwiki/` —
the in-crate location was chosen during implementation), loaded at
build time via `crates/xvision-dashboard/build.rs` + `wiki/index.toml`.
All 13 currently-baked pages carry `last_reviewed = 2026-05-20`.

| Track | Status | Note |
|---|---|---|
| #1 `docs-cli-reference-refresh` | ✅ shipped | `wiki/cli-reference.md` documents all wave A–D verbs |
| #2 `docs-strategies-refresh` | ✅ shipped | `wiki/strategies.md` (169 lines) covers hypothesis manifest |
| #3 `docs-eval-runs-refresh` | ✅ shipped | `wiki/eval-runs.md` (257 lines) covers batch + baseline + behavior summary |
| #4 `docs-scenarios-refresh` | ✅ shipped | `wiki/scenarios.md` (250 lines) covers regime labels + workflow |
| #5 `docs-experiments-page-new` | ✅ shipped | `wiki/experiments.md` (202 lines) |
| #6 `docs-agents-page-new` | ✅ shipped | `wiki/agents.md` (110 lines), intern→default agent rename noted |
| #7 `docs-providers-and-brokers-page-new` | ✅ shipped | `wiki/providers.md` (111 lines) |
| #8 `docs-wiki-filesystem-source` | ✅ shipped | `build.rs` + `index.toml` + `wiki/*.md`; no runtime FS read |
| #9 `docs-wiki-content-migration` | ✅ shipped | `wiki/operator-manual.md` (272), `wiki/runbook.md` (229), `wiki/cli-non-surfaced.md` (107) |
| #10 `docs-wiki-section-headers` | ✅ shipped | `section` field in `index.toml`; Quickstart/Concepts/CLI/Operator/Agent groupings |
| #11 `docs-agent-driving-guide` | ✅ shipped | `wiki/driving-xvn-as-an-agent.md` (271 lines) |
| #12 `docs-agentd-surface-page` | 🆕 ready | Contract opened 2026-05-21 (`team/contracts/docs-agentd-surface-page.md`) — `wiki/agentd.md` not yet written |
| #13 `docs-mcp-surface-page` | ✅ shipped | `wiki/mcp.md` (160 lines) |
| #14 `docs-route-deep-link` | ✅ shipped | `?slug=` supported in `frontend/web/src/routes/docs/index.tsx` |
| #15 `docs-freshness-staleness-guard` | 🆕 ready (partial) | `last_reviewed` field on every page, but no CI check yet. Contract opened 2026-05-21 (`team/contracts/docs-freshness-staleness-guard.md`) |
| #16 `docs-render-codeblock-copy` | ✅ shipped | `navigator.clipboard` already used in `DocsMarkdown.tsx` (copy button present) |

Two tracks remain genuinely open with contracts opened 2026-05-21:
`docs-agentd-surface-page` and `docs-freshness-staleness-guard`.
Everything else from this intake has shipped.

## Out of scope

- Versioned docs (per-release snapshots). The dashboard ships from one
  image at a time; a single live wiki is enough.
- Public-facing marketing docs. `marketing/` stays separate.
- A docs editor in the SPA. Authoring stays in-repo via the normal PR
  flow.
- Translation / i18n.
