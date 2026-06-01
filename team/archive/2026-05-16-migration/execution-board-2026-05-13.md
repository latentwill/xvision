# xvision Execution Board — 2026-05-13

This is the deduped handoff board for the current recovery/rework pass.
Older plans remain useful as source material, but agents should execute
against the tracks below rather than blindly following overlapping plan
documents.

## Superseded wrappers

Use these as reference only:

- `docs/superpowers/plans/2026-05-12-pr91-94-unworked-features.md`
- `docs/superpowers/plans/2026-05-12-qa-pass-4-remediation.md`
- `docs/superpowers/plans/2026-05-12-qa-pass-4-surface-consistency.md`
- `docs/superpowers/plans/2026-05-12-agent-access-and-cli-discoverability.md`
- `docs/superpowers/plans/2026-05-12-remote-cli-over-tailscale.md`

## Global rules

- One track per worktree.
- Board claims and `team/queue/*__claim.md` files are plain git-tracked
  coordination artifacts; they do not use the MCP/subagent message queue.
- If MCP-backed subagents hang at a "booting MCP" or similar status, keep using
  board/status files for track state and diagnose the subagent runtime
  separately instead of changing repo coordination.
- Do not execute the wrapper plans directly.
- If two tracks touch the same source file, they are not parallel-safe unless
  explicitly marked below.
- Cherry-pick narrow useful slices from older branches or PRs where cheaper,
  but do not wholesale merge stale branches.
- Old strategy format compatibility is not required. We are pre-alpha; prefer
  the simpler target shape.

## Track board

| Track | Worktree | Main scope | Depends on | Parallel-safe? | Verification |
|---|---|---|---|---|---|
| `remote-cli-orphan-recovery` | `.worktrees/remote-cli-orphan-recovery` | CLI job restart/orphan sweep | none | yes | `cargo test -p xvision-dashboard cli_jobs -- --nocapture` |
| `agent-docs-cli-truth` | `.worktrees/agent-docs-cli-truth` | README, skills docs, CLI help truth | none | yes | `cargo test -p xvision-cli help_cli -- --nocapture` |
| `ghcr-build-optimization` | `.worktrees/ghcr-build-optimization` | GH Actions preflight + staging profile | none | yes | YAML parse + workflow diff check |
| `strategy-agent-backend` | `.worktrees/strategy-agent-backend-core` | Strategy-agent backend refactor | none | no overlap with tracks 9/10 | `cargo test -p xvision-engine && cargo test -p xvision-cli strategy -- --nocapture` |
| `pr94-chart-stabilization` | `.worktrees/pr94-chart-stabilization` | Narrow chart bug salvage from PR 94 | none | yes | `pnpm --dir frontend/web test -- RunChart LiveChart` |
| `runtime-render-optimization` | `.worktrees/runtime-render-optimization` | Rust + frontend rendering-speed optimization pass | after `pr94-chart-stabilization` preferred | no overlap with active chart/frontend tracks | frontend build/test + focused chart perf smoke; Rust checks in CI/non-deploy |
| `qa4-settings-zero-provider` | `.worktrees/qa4-settings-zero-provider` | Zero providers, no default LLM, provider edit, broker replace | none | yes | `cargo test -p xvision-core -p xvision-engine` |
| `qa4-scenarios-4h-bars-ui` | `.worktrees/qa4-scenarios-4h-bars-ui` | 4H scenarios + bars fetch UI | prefer remote CLI sweep first | yes if off wizard/strategy files | `cargo test -p xvision-engine scenario -- --nocapture` |
| `qa4-chat-eval-launcher` | `.worktrees/qa4-chat-eval-launcher` | Chat tools + eval launcher preflight/errors | after track 9 preferred | no overlap with track 9 | dashboard/eval tests |
| `qa4-surface-consistency` | `.worktrees/qa4-surface-consistency` | Wizard/API/list/home/eval consistency | after track 4 preferred | no overlap with tracks 4/8 | dashboard + frontend tests |
| `strategy-agent-inspector` | `.worktrees/strategy-agent-inspector` | Inspector rebuild for agent composition | track 4 | no | frontend typecheck + authoring smoke |
| `strategy-eval-ui-polish` | current workspace | Strategy/eval UI polish after modular agents: strategy list tags/model, Inspector chrome, overflow, eval timer, xvision skill trigger | none | no overlap with active frontend docs/runtime chart tracks | focused frontend tests + typecheck; Rust API compile in CI/non-deploy |
| `mobile-safari-load` | `.worktrees/mobile-safari-load` | Mobile Safari still does not load the dashboard | none | no overlap with active frontend tracks | Safari/mobile load repro + frontend test/build smoke |
| `qa8-template-authoring-flow` | `.worktrees/qa8-template-authoring-flow` | Replace awkward Templates "open form" flow with an empty authoring form and optional template dropdown that autofills fields when selected | none | no overlap with broad authoring/Inspector frontend tracks | setup/strategy-new focused tests + frontend typecheck |
| `qa8-inspector-agent-model-picker` | `.worktrees/qa8-inspector-agent-model-picker` | Fix Strategy Inspector add-agent flow so it loads the same provider/model pick list as the chat rail, and newly added OpenRouter/DeepSeek agents appear in the Inspector agent panel | `strategy-agent-backend`, `strategy-agent-inspector` | no overlap with Inspector or agent API tracks | authoring/agent focused tests + frontend typecheck + dashboard agent route smoke |
| `qa8-shared-chat-rail-context` | `.worktrees/qa8-shared-chat-rail-context` | Audit every dashboard page and collapse separate chat rail contexts/modules so one shared chat rail/session persists across route changes, including Inspector | none | no overlap with shell/chat/eval frontend tracks | ChatRail tests + route navigation persistence smoke + frontend typecheck |
| `qa8-eval-live-decisions` | `.worktrees/qa8-eval-live-decisions` | Stream decisions into the UI while an eval is running and show a clear running indicator for active eval slots/runs | eval store/event bus stable | no overlap with eval launcher/chart tracks | eval progress/SSE tests + eval-runs focused frontend tests |
| `qa8-unbounded-slot-tool-use` | `.worktrees/qa8-unbounded-slot-tool-use` | Fix `execute_slot exceeded 8 tool-use iterations` by removing the hard eight tool-call cap for agent slots; agents should not fail solely because they need more tool calls | none | no overlap with eval runtime/agent execution tracks | tool-use regression test + eval slot execution test |
| `qa8-strategy-table-density` | `.worktrees/qa8-strategy-table-density` | Make long strategy tags readable without making table rows/columns excessively tall; remove backend ID from Strategies table columns and show ID only in Inspector | none | no overlap with strategy list/Inspector UI tracks | strategies focused frontend tests + visual smoke |
| `qa8-cli-runtime-blockers` | `.worktrees/qa8-cli-runtime-blockers` | Fix remote CLI blockers: `strategy_bundle_hash` DB schema error, inconsistent `XVN_HOME` handling, hidden fallback DBs, and unsupported 6h scenario granularity; make all supported time frames available through CLI/API | none | no overlap with CLI config/schema/scenario tracks | CLI integration tests with temp `XVN_HOME` + migration/schema regression + scenario granularity tests |
| `qa8-cli-noninteractive-core-flows` | `.worktrees/qa8-cli-noninteractive-core-flows` | Make core CLI flows fully non-interactive for strategy create, scenario create, eval run, eval list, and eval get so the task can be completed without the UI | `qa8-cli-runtime-blockers` preferred | no overlap with CLI command refactors | CLI golden tests for create/list/get/run with no prompts |
| `qa8-cli-json-contracts` | `.worktrees/qa8-cli-json-contracts` | Add stable machine-readable `--json` or `--format json` output for list/get/create/run commands with fields agents can chain | `qa8-cli-noninteractive-core-flows` preferred | no overlap with CLI output-format tracks | JSON schema/golden tests for strategy/scenario/eval commands |
| `qa8-cli-full-object-create-validate` | `.worktrees/qa8-cli-full-object-create-validate` | Allow full object creation in one command for strategies and scenarios, and add explicit `strategy validate`, `scenario validate`, and `eval validate` dry-run paths | `qa8-cli-runtime-blockers` preferred | no overlap with strategy/scenario CLI shape tracks | validation tests + full-create CLI integration tests |
| `qa8-eval-cli-workflow` | `.worktrees/qa8-eval-cli-workflow` | Make eval runs a first-class CLI workflow: `eval run`, `eval watch`, `eval results`, `eval compare`, clean final metrics, and failure reasons | `qa8-cli-runtime-blockers` preferred | no overlap with eval runtime/event-bus tracks | eval CLI integration tests + metrics output golden tests |
| `qa8-cli-doctor-help-examples` | `.worktrees/qa8-cli-doctor-help-examples` | Add real CLI examples for create strategy, create scenario, and run eval; expose `doctor` and effective config output so agents can inspect homes/config/DB targets | `qa8-cli-runtime-blockers` preferred | no overlap with docs/help CLI tracks | help snapshot tests + doctor/effective-config tests |
| `qa8-agent-ux-cli-templates` | `.worktrees/qa8-agent-ux-cli-templates` | Improve agent UX: deterministic strategy scaffolds for simple creation, UI copy-pastable CLI commands, and template registry/version parity between deployed image and local repo | `qa8-template-authoring-flow` preferred | no overlap with template authoring or shell UI tracks | frontend tests for CLI command rendering + template registry API tests |
| `qa8-scenario-display-name-contract` | `.worktrees/qa8-scenario-display-name-contract` | Fix scenario creation/tooling so custom scenarios always carry a required display name and missing-name validation is actionable | none | no overlap with scenario create/API/CLI tracks | scenario API/CLI validation tests + create-scenario focused frontend/tool tests |
| `qa8-eval-provider-preflight` | `.worktrees/qa8-eval-provider-preflight` | Prevent Web UI eval and wizard flows from launching with unconfigured `openai`/`anthropic` defaults; require configured provider/model selection or a clear zero-provider setup action | `qa4-settings-zero-provider` preferred | no overlap with eval launcher, chat rail, or provider picker tracks | eval launch/provider preflight tests + chat/wizard zero-provider regression test |
| `qa10-eval-openrouter-slot-resolution` | `.worktrees/qa10-eval-openrouter-slot-resolution` | Fix eval provider/model resolution so the one-month smoke strategy dispatches through `openrouter` instead of falling through to Anthropic; align the strategy slot/provider fields and add a regression for the run path | `qa8-eval-provider-preflight` preferred | no overlap with eval executor/parser tracks | focused eval provider-selection regression + strategy bundle smoke |
| `qa10-flash-crash-fixture-alignment` | `.worktrees/qa10-flash-crash-fixture-alignment` | Ensure `flash-crash-2024-08` resolves to a real 30-day parquet fixture at the runtime path eval expects, and keep the scenario registry/load path wired to that asset | none | no overlap with CLI/scenario data tracks | scenario fixture smoke + eval run smoke |
| `qa10-backtest-short-window-replay` | current workspace | Remove the hard 200-bar backtest warmup gate so literal 30-day/day-1 eval windows can run from available bars; keep only the next-bar fill requirement | none | no overlap unless another worker edits `crates/xvision-engine/src/eval/executor/backtest.rs` | focused backtest executor regression for 30 daily bars |
| `qa9-delete-edit-flow-verification` | `.worktrees/qa9-delete-edit-flow-verification` | Verify scenario clone-to-edit, archive, and delete failure flows after live QA stopped before delete/edit coverage | none | no overlap with wizard/strategy-agent tracks; frontend test-only coverage | scenario detail focused frontend tests + typecheck |
| `qa9-strategy-wizard-persistence` | `.worktrees/qa9-strategy-wizard-persistence` | Fix live QA bug where setup wizard/chat claims asset/cadence/risk edits but Inspector manifest still shows original draft values | none | no overlap with eval/agent attachment tracks; owns wizard authoring manifest persistence | authoring/API/wizard regression tests in CI/non-deploy + frontend typecheck |
| `qa9-readonly-editability-contract` | `.worktrees/qa9-readonly-editability-contract` | Clarify the setup/Inspector contract so read-only manifest/mechanical fields are not presented as directly editable without a successful setup tool save | none | no overlap with backend persistence; owns copy/tests for read-only contract | setup + authoring focused frontend tests + typecheck |
| `qa9-strategy-agent-attachment-flow` | `.worktrees/qa9-strategy-agent-attachment-flow` | Validate attaching an existing AgentRef from the Inspector and make attached rows show agent/provider/model metadata before eval | none | no overlap with setup wizard persistence or read-only copy tracks | authoring focused frontend tests + typecheck |
| `qa10-chat-strategy-agent-authoring-recovery` | current workspace | Recover live chat authoring failure: malformed strategy tool args, no auto-created trader AgentRef, and eval preflight missing provider/model after chat-created strategies | `qa9-strategy-wizard-persistence`, `qa9-strategy-agent-attachment-flow`, `qa8-eval-provider-preflight` | no overlap while editing chat/wizard strategy tooling | `pnpm --dir frontend/web typecheck`; `pnpm --dir frontend/web test -- ChatRail`; Rust cargo checks deferred on deploy hosts |
| `qa10-eval-trader-empty-output-resilience` | `.worktrees/qa10-eval-trader-empty-output-resilience` | Reproduce and harden eval trader decisions that fail after several ticks with empty/truncated model output (`EOF while parsing a value at line 1 column 0`), preserving raw diagnostics and preventing orders on invalid decisions | `qa9-alpaca-eval-full-run-burndown`, `qa9-json-schema-enforcement` preferred | no overlap with eval executor/parser tracks | eval executor regression for empty output + run failure reason test + paper/backtest parser tests |
| `qa10-stop-eval-run-control` | `.worktrees/qa10-stop-eval-run-control` | Add an explicit Stop Eval Run control and harden cancellation semantics so queued/running runs stop promptly, failed/cancelled terminal runs cannot keep executing in the background, streams close cleanly, and paper/live executors never place orders after stop/failure | `qa10-eval-trader-empty-output-resilience` preferred | no overlap with eval executor/progress/run-detail tracks | cancellation API tests + executor stop regression + run-detail Stop button test + SSE terminal close test |
| `qa10-eval-chat-scrollbars-controls` | `.worktrees/qa10-eval-chat-scrollbars-controls` | Implement the `docs/design/XVN_Scrollbars.zip` prototype: always-visible themed scrollbars, eval decision list scrollbar/filter, clear BUY/SELL/HOLD/CLOSE signal boxes with reasoning text, chat rail scrollbar, and square Play/Stop composer control with no highlighted border | `color-themes-light-dark` preferred if theme variables are moving | no overlap with broad shell/chat/eval frontend tracks | `pnpm --dir frontend/web test -- ChatRail eval-runs-detail`; frontend typecheck; visual smoke against the design zip |
| `qa10-chat-scenario-dsml-recovery` | `.worktrees/qa10-chat-scenario-dsml-recovery` | Fix chat-rail scenario creation from prompts like “try a paper eval on solana in q1 2026”: required `description` must survive tool argument parsing, malformed DSML/partial JSON must fail locally with a short repair path, and tool traces must not stream corrupt markup indefinitely | `qa10-chat-strategy-agent-authoring-recovery`, `qa8-scenario-display-name-contract` | no overlap with chat wizard/tool parser tracks | WizardLoop/chat route regression for SOL Q1 2026 scenario + malformed DSML argument test + ChatRail rendering test |
| `eval-review-data-model` | `.worktrees/eval-review-data-model` | Add the Eval Review Agent persistence foundation from `docs/superpowers/specs/2026-05-15-eval-review-agent.md`: `eval_reviews`, seeded review agent profiles, and review-linked first-class findings while preserving existing `eval_findings` callers | none | no overlap with eval store/schema migrations | migration tests + RunStore review/findings roundtrip tests |
| `eval-review-agent-engine` | `.worktrees/eval-review-agent-engine` | Build the review payload collector, strict JSON prompt/response contract, agent-profile dispatch, hallucination/evidence validation, and normalized review persistence for completed eval runs | `eval-review-data-model`, `qa10-eval-trader-empty-output-resilience` preferred | no overlap with eval executor/parser tracks | review contract unit tests + sparse-payload inconclusive test + evidence-reference validation test |
| `eval-review-api-cli` | `.worktrees/eval-review-api-cli` | Add `POST /api/eval/runs/:id/review`, `GET /api/eval/runs/:id/reviews`, `GET /api/eval/reviews/:id`, plus `xvn eval review <run_id> --agent ... [--force] [--output review.json]` | `eval-review-agent-engine`, `qa8-cli-json-contracts` preferred | no overlap with broad eval CLI/API command refactors | dashboard route tests + CLI golden tests for human and JSON output |
| `eval-review-run-detail-ui` | `.worktrees/eval-review-run-detail-ui` | Add the `/eval-runs/:id` Review panel with agent picker, status, verdict badge, confidence, regenerate action, summary, findings, risks, next tests, open questions, and evidence links | `eval-review-api-cli`, `qa10-eval-chat-scrollbars-controls` preferred | no overlap with eval run-detail styling tracks | eval-runs-detail focused tests + frontend typecheck + visual smoke |
| `color-themes-light-dark` | `.worktrees/color-themes-light-dark` | Execute `docs/superpowers/plans/2026-05-14-color-themes-light-dark-mode.md`: color-only dashboard themes, General settings, sidebar sun/moon toggle, chart palette integration | none | no overlap with broad shell/settings/chart frontend tracks | `corepack pnpm --dir frontend/web test && corepack pnpm --dir frontend/web typecheck && corepack pnpm --dir frontend/web build` |

## Recommended order

1. `remote-cli-orphan-recovery`
2. `agent-docs-cli-truth`
3. `ghcr-build-optimization`
4. `strategy-agent-backend`
5. `pr94-chart-stabilization`
6. `runtime-render-optimization`
7. `qa4-settings-zero-provider`
8. `qa4-scenarios-4h-bars-ui`
9. `qa4-surface-consistency`
10. `qa4-chat-eval-launcher`
11. `strategy-agent-inspector`
12. `mobile-safari-load`
13. `strategy-eval-ui-polish`
14. `qa8-template-authoring-flow`
15. `qa8-strategy-table-density`
16. `qa8-inspector-agent-model-picker`
17. `qa8-shared-chat-rail-context`
18. `qa8-unbounded-slot-tool-use`
19. `qa8-eval-live-decisions`
20. `qa8-cli-runtime-blockers`
21. `qa8-cli-noninteractive-core-flows`
22. `qa8-cli-json-contracts`
23. `qa8-cli-full-object-create-validate`
24. `qa8-eval-cli-workflow`
25. `qa8-cli-doctor-help-examples`
26. `qa8-agent-ux-cli-templates`
27. `qa8-scenario-display-name-contract`
28. `qa8-eval-provider-preflight`
29. `qa10-eval-openrouter-slot-resolution`
30. `qa10-flash-crash-fixture-alignment`
31. `qa10-backtest-short-window-replay`
32. `color-themes-light-dark`
33. `qa10-eval-trader-empty-output-resilience`
34. `qa10-stop-eval-run-control`
35. `qa10-chat-scenario-dsml-recovery`
36. `qa10-eval-chat-scrollbars-controls`
37. `eval-review-data-model`
38. `eval-review-agent-engine`
39. `eval-review-api-cli`
40. `eval-review-run-detail-ui`

## Immediate start set

Safe to start now:

- `remote-cli-orphan-recovery`
- `agent-docs-cli-truth`
- `ghcr-build-optimization`
- `pr94-chart-stabilization`
- `runtime-render-optimization` once `pr94-chart-stabilization` is merged or no
  longer active
- `qa4-settings-zero-provider`
- `qa4-scenarios-4h-bars-ui`
- `mobile-safari-load`
- `qa8-template-authoring-flow`
- `qa8-strategy-table-density`
- `qa8-unbounded-slot-tool-use`
- `qa8-cli-runtime-blockers`
- `qa8-scenario-display-name-contract`
- `qa10-eval-openrouter-slot-resolution`
- `qa10-flash-crash-fixture-alignment`
- `qa10-backtest-short-window-replay`
- `color-themes-light-dark`
- `qa10-eval-trader-empty-output-resilience`
- `qa10-stop-eval-run-control`
- `eval-review-data-model`

Wait for `strategy-agent-backend`:

- `qa4-surface-consistency`
- `strategy-agent-inspector`
- `qa8-inspector-agent-model-picker`

Wait for shared eval/chat ownership:

- `qa8-shared-chat-rail-context`
- `qa8-eval-live-decisions`

Wait for CLI runtime blockers:

- `qa8-cli-noninteractive-core-flows`
- `qa8-cli-json-contracts`
- `qa8-cli-full-object-create-validate`
- `qa8-eval-cli-workflow`
- `qa8-cli-doctor-help-examples`
- `qa8-agent-ux-cli-templates`

Wait for provider settings stabilization:

- `qa8-eval-provider-preflight`

Wait for chat/scenario parser stabilization:

- `qa10-chat-scenario-dsml-recovery`

Wait for theme stabilization:

- `qa10-eval-chat-scrollbars-controls`
- `eval-review-run-detail-ui`

Wait for eval review foundation:

- `eval-review-agent-engine`
- `eval-review-api-cli`

Do not overlap:

- `strategy-agent-backend` with `qa4-surface-consistency`
- `strategy-agent-backend` with `strategy-agent-inspector`
- `qa4-surface-consistency` with `qa4-chat-eval-launcher`
- `mobile-safari-load` with broad frontend route refactors
- `qa8-inspector-agent-model-picker` with `strategy-agent-inspector`
- `qa8-shared-chat-rail-context` with broad shell/chat-rail/frontend route refactors
- `qa8-eval-live-decisions` with eval launcher or eval event-bus refactors
- `qa8-strategy-table-density` with `strategy-eval-ui-polish` if that track is still editing Strategies table/Inspector UI files
- `qa8-cli-runtime-blockers` with broad CLI config/store/schema refactors
- `qa8-cli-noninteractive-core-flows` with `qa8-cli-full-object-create-validate`
- `qa8-eval-cli-workflow` with `qa8-eval-live-decisions` if both touch eval event/progress contracts
- `qa8-agent-ux-cli-templates` with `qa8-template-authoring-flow`
- `qa8-scenario-display-name-contract` with `qa8-cli-full-object-create-validate`
  if both are reshaping scenario create payloads.
- `qa8-eval-provider-preflight` with `qa4-chat-eval-launcher`,
  `qa8-shared-chat-rail-context`, or `qa8-inspector-agent-model-picker` if
  those tracks are still editing provider/model picker wiring.
- `color-themes-light-dark` with broad shell/settings/chart frontend tracks,
  especially `qa8-shared-chat-rail-context`, `pr94-chart-stabilization`,
  `runtime-render-optimization`, or any active Settings layout refactor.
- `qa10-eval-trader-empty-output-resilience` with `qa9-alpaca-eval-full-run-burndown`,
  `qa9-json-schema-enforcement`, or any active eval executor/parser refactor.
- `qa10-stop-eval-run-control` with `qa10-eval-trader-empty-output-resilience`,
  `qa8-eval-live-decisions`, or any active eval executor/progress/run-detail
  cancellation refactor.
- `qa10-chat-scenario-dsml-recovery` with `qa10-chat-strategy-agent-authoring-recovery`,
  `qa8-scenario-display-name-contract`, or any active WizardLoop/chat tool parser refactor.
- `qa10-eval-chat-scrollbars-controls` with `color-themes-light-dark`,
  `qa9-chat-rail-inflight-controls`, `qa8-eval-live-decisions`,
  `qa10-stop-eval-run-control`, or broad ChatRail/eval route styling refactors.
- `eval-review-data-model` with broad eval store/schema migrations.
- `eval-review-agent-engine` with `qa10-eval-trader-empty-output-resilience`
  or active eval executor/parser refactors if both touch decision payload
  diagnostics, persisted events, or postprocess hooks.
- `eval-review-api-cli` with broad eval CLI/API refactors, especially
  `qa8-eval-cli-workflow`, `qa8-cli-json-contracts`, or eval route reshaping.
- `eval-review-run-detail-ui` with `qa10-eval-chat-scrollbars-controls`,
  `qa8-eval-live-decisions`, or any broad `/eval-runs/:id` run-detail rewrite.

## Cherry-pick policy

Allowed:

- Chart-only fixes from PR 94 into `pr94-chart-stabilization`
- Narrow bars-fetch UI slices from `.worktrees/bars-fetch-ui`
- Missing remote CLI job pieces only if confirmed absent

Avoid:

- Merging `origin/codex/qa-pass-4` wholesale
- Reviving `2026-05-12-pr91-94-unworked-features.md` as an execution branch
- Carrying forward old strategy-shape compatibility unless a live caller forces it

## Track notes

### `strategy-agent-backend`

- This is the central backend unblocker.
- Old slot-shape compatibility can be cut aggressively.
- Default authoring model: inline agents first, promote later.
- Use `.worktrees/strategy-agent-backend-core`. The earlier
  `.worktrees/strategy-agent-backend` branch contains out-of-scope frontend
  work and is source-only unless deliberately cherry-picked.
- Current checkpoint: `strategy-agent-backend-core` commit `2ae9828` exposes
  existing agent-ref/pipeline API through `xvn strategy add-agent`,
  `remove-agent`, and `set-pipeline`.
- Additional checkpoints: `f2786a3` adds `xvn strategy migrate-agents`, and
  `fd1fc0e` executes resolved single/sequential AgentRef pipelines through
  eval and `xvn strategy run`.
- Latest checkpoint: `b9c39f1` makes `xvn strategy new` seed template drafts
  directly as AgentRefs + pipeline, and updates `xvn strategy run` token
  estimates to derive from resolved agent slots for AgentRef strategies.
- Inspector can build against AgentRefs; graph pipeline execution is still
  intentionally not implemented in runtime.

### `pr94-chart-stabilization`

- Target chart stabilization first.
- Safe scenario/new-screen fixes may be absorbed only if clearly scoped and
  do not drag in unrelated PR 94 state.
- Completed checkpoint: `pr94-chart-stabilization` commit `da8b3a6`
  stabilizes RunChart time-scale synchronization and passes
  `corepack pnpm --dir frontend/web test -- RunChart LiveChart`.

### `runtime-render-optimization`

- Claimed in `.worktrees/runtime-render-optimization`.
- Implemented checkpoint:
  - `RunChart` now keeps chart instances stable across data-only payload
    changes and updates existing series instead of rebuilding every pane.
  - Dashboard router applies HTTP compression.
  - Embedded static assets get cache headers (`index.html` no-cache, hashed
    assets immutable).
  - Frontend production source maps are no longer embedded.
  - Font imports are latin-only.
  - Route modules and the chat rail are lazily split from the initial bundle.
- Verification:
  - `corepack pnpm --dir frontend/web test -- RunChart LiveChart`
  - `corepack pnpm --dir frontend/web typecheck`
  - `corepack pnpm --dir frontend/web test`
  - `corepack pnpm --dir frontend/web exec vite build --outDir /tmp/xvision-runtime-render-build --emptyOutDir true`
  - `git diff --check`
- Build artifact check: no `.map` files emitted, font asset count is 18
  instead of the previous 100, and the old single ~805 KB JS bundle is split
  into route/chart/chat chunks.
- Rust checks remain CI/non-deploy follow-up because local Cargo is forbidden
  on this deploy host.

### `ghcr-build-optimization`

- Completed checkpoint: `ghcr-build-optimization` commit `ef76621`
  installs pnpm before `actions/setup-node` enables pnpm cache in the GHCR
  workflow preflight.
- Verified with `git diff --check`, workflow diff check, and Python YAML parse.

### `agent-docs-cli-truth`

- Completed checkpoint: `agent-docs-cli-truth` commit `b617c09` aligns
  README/MANUAL/Claude-skill/frontend CLI guidance with the shipped command
  surface.
- Verified with `bash scripts/check_agent_docs.sh` and `git diff --check`.
- Rust help test remains CI/non-deploy follow-up because cargo is forbidden on
  this deploy host.

### `remote-cli-orphan-recovery`

- Completed checkpoint: `remote-cli-orphan-recovery` commit `d1d41b3`
  wires CLI job orphan recovery/restart handling.
- Verified with `git diff --check main...HEAD`, scoped `rg` audit, and clean
  branch status. Cargo verification remains CI/non-deploy follow-up.

### `qa4-settings-zero-provider`

- Workspace startup with zero providers and no default LLM is required.
- Completed checkpoint: `qa4-settings-zero-provider` commit `1ed7c45`
  allows zero-provider startup/no default LLM and updates provider/broker UI
  behavior.
- Verified with `git diff --check`, static conflict scan, focused provider
  frontend test, full frontend test suite, and frontend typecheck. Rust
  core/engine tests remain CI/non-deploy follow-up.

### `qa4-scenarios-4h-bars-ui`

- Completed checkpoint: `qa4-scenarios-4h-bars-ui` commit `7f00196`
  implements 4H scenarios + bars-fetch UI across API/CLI/frontend surfaces.
- Verified with frontend install, full frontend test suite, frontend typecheck,
  `git diff --check`, and clean branch status. Rust scenario tests remain
  CI/non-deploy follow-up.

### `strategy-agent-inspector`

- Completed checkpoint: `strategy-agent-inspector` commit `cd5687d` adapts the
  authoring/Inspector UI for strategy AgentRefs and pipeline state.
- Verified with `corepack pnpm --dir frontend/web test -- authoring-risk`,
  frontend typecheck, and `git diff --check`.

### `qa4-surface-consistency`

- Completed checkpoint: `qa4-surface-consistency` commit `350d462` repairs
  command-palette/frontend route consistency and updates frontend docs.
- Verified with full frontend test suite, frontend typecheck, frontend build,
  and `git diff --check`. Rust dashboard verification remains CI/non-deploy
  follow-up.

### `qa4-chat-eval-launcher`

- Completed checkpoint: `qa4-chat-eval-launcher` commit `18ab4c0` adds eval
  launcher preflight, uses scenario-registry scenarios, defaults the web
  launcher to backtest mode, and keeps launch/preflight errors inline.
- Final branch head is `4fbabaa`, which removes a branch-local board edit so
  the track status stays scoped to `team/status/qa4-chat-eval-launcher.md`.
- Verified with focused eval-runs test, frontend typecheck, full frontend test
  suite, and `git diff --check`. Rust checks remain CI/non-deploy follow-up.

### `mobile-safari-load`

- New regression report: mobile still does not load on Safari.
- Repeat Q8 operator report on 2026-05-13: mobile Safari still does not load
  the page after the latest QA pass.
- Claimed in `.worktrees/mobile-safari-load`; focus on root-cause isolation
  before changing production code.
- Initial target surface: Vite/React dashboard startup path and any browser
  compatibility assumptions that can break iOS Safari before the app renders.

### Board Closeout

- All previous execution-board tracks now have branch/status checkpoints and clean
  worktrees.
- `mobile-safari-load` and `strategy-eval-ui-polish` were the final open work
  on this board during the 2026-05-13 merge/deploy pass.

### `strategy-eval-ui-polish`

Claimed 2026-05-13T08:46:17Z in the current workspace.

Scope:

- Fix the Strategies model column for the modular AgentRef structure.
- Surface strategy tags on the Strategies page.
- Remove the Inspector validation box.
- De-emphasize the Inspector strategy id.
- Prevent long IDs/errors from overflowing UI boxes.
- Add elapsed/duration timing to eval runs.
- Confirm conversation persistence status; SQLite-backed chat sessions/messages
  already exist, so no new work is needed unless product wants a richer export.
- Narrow the xvision Claude skill trigger so it is for agents using the `xvn`
  CLI/dashboard, not generic coding in this repo.

Initial claim order:

1. Strategy list model/tags.
2. Inspector cleanup and overflow.
3. Eval timer.
4. Skill trigger wording.

### Q8 QA intake

New operator QA report on 2026-05-13. Split into implementation-sized tracks
above; do not treat this as a single broad cleanup branch.

Raw items mapped to board tracks:

- `mobile-safari-load`: Mobile Safari still does not load the dashboard page.
  Treat as a current Q8 blocker, not an already-closed historical note.
- `qa8-template-authoring-flow`: Templates wording is awkward. The strategy
  authoring page should open as an empty form; templates should be an optional
  dropdown that autofills the form when selected.
- `qa8-inspector-agent-model-picker`: Strategy Inspector still cannot add an
  agent because its provider/model pick list is empty. It must use the same
  provider/model list as the chat rail. Agents created via chat rail, including
  OpenRouter / DeepSeek V4 Flash, must show in the Inspector agent panel.
- `qa8-shared-chat-rail-context`: Inspector appears to have a separate chat
  rail context that disappears when leaving the Inspector page. Review all
  pages and remove/rework page-specific chat rail modules so there is one
  shared chat rail across the dashboard.
- `qa8-eval-live-decisions`: Decisions should stream into the UI while an eval
  is running, with a visible running indicator.
- `qa8-unbounded-slot-tool-use`: Eval failed with
  `execute_slot exceeded 8 tool-use iterations — the model is stuck calling tools without producing a final decision`.
  Remove or rework the hard eight tool-use iteration limit so agents are not
  blocked from taking the tool calls they need.
- `qa8-strategy-table-density`: Strategy tags are too squished and make table
  columns/rows very tall. Backend ID is not useful in the Strategies table;
  remove it from table columns and show ID only in the Inspector.
- `qa8-cli-runtime-blockers`: Agent attempting CLI had to use Python as a
  thin HTTP client because remote `xvn` CLI subcommands failed with
  `no such column: strategy_bundle_hash`; audit migrations/schema and any
  skill/docs references that may be pointing agents at broken CLI paths. Fix
  `XVN_HOME` so every subcommand honors it consistently, with no hidden fallback
  to a baked-in default DB. Add all scenario time frames through API/CLI;
  6h granularity was specifically rejected by the scenario API.
- `qa8-cli-noninteractive-core-flows`: Core workflows must be fully
  non-interactive: `xvn strategy create`, `xvn scenario create`,
  `xvn eval run`, `xvn eval list`, and `xvn eval get`. Everything needed for
  the task should be possible without the UI.
- `qa8-cli-json-contracts`: Add a machine-readable mode, either `--json` or
  `--format json`, for list/get/create/run commands. Outputs need stable
  fields that are easy for agents to parse and chain.
- `qa8-cli-full-object-create-validate`: Full object creation should be
  possible in one command. Strategy creation needs template, display name,
  provider/model, risk settings, mechanical params, and tags. Scenario creation
  needs asset, date range, granularity, fees, slippage, and latency. Add
  explicit dry-run validation for strategy, scenario, and eval so schema
  mistakes fail before long runs.
- `qa8-eval-cli-workflow`: Eval runs need first-class commands:
  `xvn eval run --strategy ... --scenario ...`, `xvn eval watch <run_id>`,
  `xvn eval results <run_id>`, and `xvn eval compare ...`. Final output should
  cleanly include total return, Sharpe, max drawdown, win rate, trade/decision
  count, and failure reason when applicable.
- `qa8-cli-doctor-help-examples`: Help output should include real examples for
  creating a strategy, creating a scenario, and running an eval. Add `doctor`
  and/or `config show --effective` so agents can see the effective config,
  `XVN_HOME`, DB path, providers, templates, and remote/local target.
- `qa8-agent-ux-cli-templates`: Reduce tool loops in strategy drafting by
  preferring deterministic scaffolds for simple strategy creation over
  open-ended wizard behavior. UI forms should print copy-pastable CLI commands
  for the strategy/eval/scenario action they represent. Keep templates aligned
  between deployed image and local repo with a clear template registry/version
  endpoint; note product concern that templates can constrain agents, so this
  should support broader registries rather than force a narrow template-only
  path.
- `qa8-scenario-display-name-contract`: Latest build intake found scenario
  creation/tooling could omit the scenario display name, leaving the workflow
  to fail later with unclear validation. Make `display_name` a first-class
  required field across the Web UI, wizard tool schema, dashboard API, and CLI
  create/validate paths. If the name is omitted, fail before creating the
  scenario with a field-specific message that tells the caller to provide a
  scenario display name.
- `qa8-eval-provider-preflight`: Latest Web UI eval launch failed with
  `validation: request: provider 'openai' is not configured. Pick a configured provider/model for the strategy agent before running eval.`
  and wizard recovery then tried unavailable providers (`anthropic`, then
  `openai`) until it hit
  `wizard tool-use loop exceeded 12 iterations`. The UI and wizard should read
  configured providers/models first, block eval launch when none are usable,
  and show a setup action instead of retrying guessed providers. Strategies
  with missing or stale agent provider/model assignments should be flagged in
  preflight before queueing an eval.

### Q10 QA intake

New operator QA report on 2026-05-15. Split into implementation-sized tracks
above; do not merge these into the completed QA8/QA9 status files unless a
worker is explicitly rebasing unfinished work.

Raw items mapped to board tracks:

- `qa10-eval-trader-empty-output-resilience`: Eval run
  `01KRMKWZ1KJ2BGRNWGP518ZQ3Q` failed after about four decisions with
  `run 01KRMKWZ1KJ2BGRNWGP518ZQ3Q decision 4: trader output is invalid JSON: EOF while parsing a value at line 1 column 0`.
  Treat this as a different failure mode from the earlier missing-`action`
  schema bugs: the model/tool layer produced an empty or truncated final trader
  payload after the run had already made progress. Required action: reproduce
  from the run row/logs where available, persist enough raw provider
  diagnostics to distinguish empty text, stream abort, timeout, and parser
  failure, and ensure paper/live executors never submit an order for an invalid
  or missing trader decision. A bounded retry is acceptable only if it is
  explicitly idempotent and recorded in run events.
- `qa10-eval-openrouter-slot-resolution`: The one-month smoke strategy still
  resolves its executable LLM slot to `anthropic` at runtime, which forces an
  Anthropic API-key check and 401s even though the strategy policy is
  OpenRouter DeepSeek. Required action: align the strategy slot/provider fields
  so eval dispatches through `openrouter` only, and add a regression that proves
  `xvn eval run --strategy 01KRMYS1N4QT5B9EM32VNXJJ9V --scenario
  flash-crash-2024-08 --mode paper` selects the OpenRouter path before queueing
  the run.
- `qa10-flash-crash-fixture-alignment`: `flash-crash-2024-08` still depends on a
  real 30-day parquet fixture at the runtime path eval expects. Required
  action: verify the scenario registry points at a materialized asset, package
  or symlink it into the deployed data path, and add a smoke check so the eval
  cannot launch against a missing fixture.
- `qa10-backtest-short-window-replay`: QA report
  `/home/agents/qa/xvision-eval-2026-05-15/report.md` found the provider bug is
  fixed and local-candle eval works, but a literal 30-day Day1 eval cannot
  complete because `BacktestExecutor` hard-skips a 200-bar warmup. Required
  action: remove that artificial warmup restriction from the executor and add a
  regression proving a 30 daily-bar fixture starts decisions from day 1. Keep
  only constraints that are mechanically required, such as needing a following
  bar when fills execute at next open.
- `qa10-stop-eval-run-control`: Operators need a clear Stop Eval Run control
  because failed runs appear to keep going after the UI/run row reports failure.
  The code already has a `/api/eval/runs/:id/cancel` route and a small Cancel
  affordance, but this track must verify the full lifecycle rather than only
  restyling the button. Required action: rename/upgrade the run-detail affordance
  to an explicit Stop control for queued/running runs; ensure stop requests are
  visible while pending; make cancellation idempotent for repeated clicks; ensure
  background tasks check terminal status before every model call, decision
  persist, fill/order placement, metric update, and postprocess step; make a
  failed run terminal immediately so no later executor loop can overwrite it or
  continue streaming; close SSE with a terminal status event; and add an audit
  trail/event that distinguishes user stop from model/parser failure. If the
  executor can be active after the DB row is already `failed`, add a safe
  internal stop signal or terminal-state guard rather than allowing a "failed
  but still trading" state.
- `qa10-eval-chat-scrollbars-controls`: Use the attached prototype archive at
  `docs/design/XVN_Scrollbars.zip` (contains `eval-focus-bundle/*`) as the
  source for visible scrollbar styling and the eval focus/filter treatment.
  Add a scrollbar and filter to the decisions section under eval run detail.
  Preserve the prototype's clear action signal boxes for BUY, SELL, HOLD, and
  CLOSE, including distinct color/background/border treatment and compact
  counts in the filter row. Add the decision `reasoning` field to the displayed
  ledger; if the current persisted/API field is still named `justification`,
  the UI/API contract should either alias it to `reasoning` or migrate the wire
  shape without losing existing decision rows. The chat rail scrollbar must be
  visible at all times and match the same theme treatment. The chat rail stop
  button should become a square icon-only control with no highlighted border;
  when no chat/task is active it should render as Play and submit the composer
  exactly like Enter, and while active it should render as Stop and abort the
  in-flight request.
- `qa10-chat-scenario-dsml-recovery`: Chat prompt
  `try a paper eval on solana in q1 2026` updated the manifest to
  `asset_universe=SOL/USD` and `decision_cadence_minutes=240m`, then attempted
  to create `SOL Q1 2026` but failed with `create_scenario failed: missing field
  \`description\``. The streamed trace showed a `description` parameter followed
  by malformed/partial DSML around the `asset` payload
  (`"venue_symbol": "SOL/USD":<|DSML|tool_calls> ...`) and then continued for a
  long time. Required action: make chat scenario creation validate the decoded
  tool argument object before invoking the tool, keep required fields
  (`display_name`, `description`, asset/date/granularity) intact through DSML
  parsing, surface a compact repair prompt or local validation error on
  malformed tool markup, and add a regression for SOL Q1 2026 paper-eval
  scenario creation.

### Eval Review Agent intake

New product spec captured in
`docs/superpowers/specs/2026-05-15-eval-review-agent.md`. This is not a simple
metrics summary. It adds an analytical review layer over completed eval runs,
using selected review agents and storing review findings as first-class objects
for future autooptimizer, lineage, and reputation work.

Raw spec mapped to board tracks:

- `eval-review-data-model`: Add `eval_reviews`, seed review agent profiles,
  and preserve/extend `eval_findings` so review findings can link to a review
  while existing lightweight extractor callers keep working. The repo already
  has `eval_findings`; do not create a text-only report table that hides
  findings inside `raw_output_json`.
- `eval-review-agent-engine`: Build the bounded review payload from current
  persisted artifacts: `eval_runs.metrics_json`, `eval_decisions`,
  `eval_equity_samples`, existing findings/events/errors where available, and
  scenario/agent metadata. The prompt must acknowledge missing artifacts rather
  than invent orders, positions, market metadata, or logs that are not persisted
  yet. Enforce strict JSON, evidence references, low-temperature deterministic
  output, and sparse-payload `inconclusive` behavior.
- `eval-review-api-cli`: Expose review generation and reads through dashboard
  API routes and `xvn eval review`. The route shape must match current code:
  frontend path is `/eval-runs/:id`; API path is `/api/eval/runs/:id/review`,
  plus review list/detail endpoints.
- `eval-review-run-detail-ui`: Add the Review panel to existing
  `frontend/web/src/routes/eval-runs-detail.tsx` instead of creating a parallel
  page. Required display sections are executive summary, verdict, key findings,
  risks, evidence, recommended next tests, and open questions. Regeneration with
  another agent is part of MVP.

MVP acceptance:

- A completed eval run can be reviewed by Fast Trader, Reasoning, Risk, or
  Research agent profile.
- Review and normalized findings persist.
- Run-detail displays summary, verdict, confidence, findings, risks, next
  tests, and questions.
- CLI can generate and export the review.
- The agent does not hallucinate metrics or trades absent from the eval payload.
- Deferred: autooptimizer loop, mutation, blockchain identity, marketplace
  publishing, live trading decisions, settlement, multi-agent debate, and memory
  graph.

## Next board intake

The active roadmap for followups, specs, and todo notes is now
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.

Seed the next execution board from these first tickets:

| Order | Ticket | Phase | Effort | Source |
|---|---|---|---|---|
| 1 | Add Driver.js first-run and restart-tour infrastructure | V2A | M | F36 |
| 2 | Add in-app docs/help route and docs index | V2A | M | onboarding/settings, dashboard docs |
| 3 | Create resettable example strategies/scenarios/tutorial artifacts | V2A | M | frontend docs, eval docs |
| 4 | Add dashboard mutating-route auth boundary | V2B | L | F35 |
| 5 | Add remote CLI orphan recovery and audit trail | V2B | M | F37, remote CLI specs |
| 6 | Add broker/wallet/testnet kill switch and limits | V2B | M | security + blockchain plans |
| 7 | Deploy/refactor Mantle Sepolia identity/reputation addresses | V2C | M | SLF2, ADR 0008 |
| 8 | Implement strategy NFT mint/readback flow | V2C | L | SLF3 |
| 9 | Implement testnet marketplace list/buy/sell/delegate flow | V2C | L | marketplace spec |
| 10 | Implement reputation and validation receipt write/readback | V2C | L | SLF4, SLF5 |
| 11 | Build autooptimizer mutation/eval/judge loop | V3 | L | autooptimizer plans |
| 12 | Build autooptimizer dashboard and lineage review | V3 | L | autooptimizer dashboard plan |
| 13 | Run final UI/UX pass across dashboard surfaces | V3 | L | design docs, chart plans |
| 14 | Prepare contract audit, launch flags, and mainnet runbook | V4 | L | ADR 0008, contract specs |
