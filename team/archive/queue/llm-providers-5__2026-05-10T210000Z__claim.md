---
from: llm-providers-5
to: all
topic: claim
created_at: 2026-05-10T21:00:00Z
ack_required: false
---

# `llm-providers-5` track claimed (Phase 4 finish — Tasks 14, 15, 16)

Session 3 (continuing the Plan #7 thread; Phases 1, 2, 3, and Phase 4 T13
already merged via PRs #14/#16/#20/#22/#27) takes the three remaining
followups that close Phase 4. Worktree `.worktrees/llm-providers-5`,
branch `feature/llm-providers-phase-4-finish`.

## Scope

- **T14** — `xvn provider add` / `remove` with in-place TOML mutation via
  `toml_edit` (preserves comments + formatting). Refuses duplicates,
  invalid kinds, names starting with `_` (synthetic prefix), and removal
  of any provider whose triple matches the workspace `[intern]` block.
  5 new unit tests.
- **T15** — `xvn provider check` with env-presence + TCP-connect smoke
  and an opt-in `--probe` that GETs `<base_url>/models` with the Bearer
  key. Tiny in-file `url_parse_minimal` covers the two URL shapes we use
  (http/https, optional explicit port). 3 new unit tests.
- **T16** — `cache_diverges_on_intern_model_change` regression test in
  `xvision-eval::baselines::trader_arm` that locks the spec §3.5 cache
  divergence semantics. No production change.

Three commits, one PR — closes Plan #7 Phase 4.

## Files this track touches (no overlap with other active sessions)

- `crates/xvision-cli/src/commands/provider.rs` — fills in `add`, `remove`,
  `check` (Phase 4 T13 merged the `list`/`show` scaffold + stubs).
- `crates/xvision-cli/Cargo.toml` — adds `toml_edit = "0.22"` and
  `reqwest = { workspace = true }`.
- `crates/xvision-eval/src/baselines/trader_arm.rs` — appends one
  `#[tokio::test]` to the existing `mod tests`.
- `Cargo.lock` — toml_edit + reqwest pull-ins.

Zero overlap with active sessions:
- `frontend-2-home-and-health` (PR #13)
- `frontend-2-settings` (PR #18)
- `frontend-2-eval-runs` / `frontend-2-run-detail` (PRs #21 / #24)

## Out of scope (still deferred)

- **Phase 5** — UI design lock + migration note (4 doc tasks; T17–T20).
- **Plan 2a** — MCP server + verbs + tool dispatch + polish.
- **Plan 2d** — Dashboard + Wizard.

## v1 QA value

Closes the operator surface for the providers registry. With T14–T15 an
operator can register and reachability-check providers from the CLI
without touching `config/default.toml` by hand:

```
$ xvn provider add --name groq --kind openai-compat \
    --base-url https://api.groq.com/openai/v1 --api-key-env GROQ_API_KEY
$ xvn provider check --name groq
● env GROQ_API_KEY set
● tcp api.groq.com:443 reachable
$ xvn provider remove --name groq
```

T16 closes the spec §3.5 lock-down for the per-arm Intern model story.
