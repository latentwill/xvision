# WIP branch triage — 2026-10-22

Compared all 146 `origin/wip/2026-10-22-*` refs after `git fetch --all` with pinned `origin/main` [`9bd6215d`](https://github.com/latentwill/xvision/commit/9bd6215dd5446b7fc019791f21412db26b92a13f). The refs contain 134 distinct tips. Work took place only in this clone and GitHub. No server access, branch deletion, force-push, or main update occurred.

## Summary

| Class | Branches | Recommendation |
| --- | ---: | --- |
| MERGED | 62 | Delete later, after this inventory is reviewed. |
| SUPERSEDED | 74 | Delete later; main contains a squash, replacement, or later rewrite. |
| UNIQUE | 10 | Keep 8 archives; review 2 branches for selective salvage. |

All requested T1–T8 fixes and EJ QA polish behavior already landed in main. T4, T5, and T8 are individually patch-equivalent. T1, T2, T3, and T7 landed in revised commits. The QA branch as a whole is patch-equivalent to the combined main commit. No runtime code is added by this salvage branch; its only final change is this report. Conflicting patches were aborted, and empty patches were skipped.

`salvage` below means a candidate for a separate, reviewed follow-up, not a change included in this PR. `delete-later` is a recommendation, not an action. Preserve the unique snapshots until their useful files have been separately retained.

## How the classes were determined

- **MERGED:** `git cherry MAIN REF` has no `+` entries. This includes reachable commits and individual patch equivalents. Git cherry ignores merge commits; this is the requested commit/patch classification, not a claim of identical trees.
- **SUPERSEDED:** positive cherry entries exist, but an aggregate stable patch ID matches a main commit, or patch and current-code inspection identifies the replacement. Each row records that evidence. A later modification alone was not treated as proof of equivalence.
- **UNIQUE:** a substantive residual remains, including unmerged research, operational scripts, and prototype tests. A mixed branch stays UNIQUE even when some of its changes landed.
- 77 refs have no common Git ancestor with current main. Their old history has mostly patch-equivalent counterparts. Raw ahead counts would overstate new work, so the table shows both positive cherry commits and all unreachable commits.
- For related UNIQUE branches, file/line totals are the net diff from the merge base. For the unrelated logging-design branch, the baseline is the parent of its sole positive cherry commit (the tip). Binary files are counted as files but excluded from line totals. Tests on archived snapshots were not run; current-code results are distinguished below.
- Positive-line matches against main history helped locate squash/rewrite evidence; they were not used alone to assert behavioral equivalence. Identical snapshot tips share the same verdict.

Reproduce the inventory and per-branch checks with these commands (the main SHA is pinned):

```bash
main=9bd6215dd5446b7fc019791f21412db26b92a13f
git for-each-ref --format="%(refname:short) %(objectname)" \
  "refs/remotes/origin/wip/2026-10-22-*"
ref=origin/wip/2026-10-22-dev-xvn-wt-t1
git cherry "$main" "$ref"
git rev-list --count "$main..$ref"
git merge-base "$main" "$ref"
git diff --numstat "$(git merge-base "$main" "$ref")" "$ref"
git log --no-merges -p --format="commit %H" "$main" | git patch-id --stable
```

For unrelated histories, a recent first-parent ancestor marked `-` by `git cherry` provides a squash-comparison baseline. Merge commits can broaden that delta. The UNIQUE logging branch has only one positive commit, at its tip, so its reported file/line delta uses that commit’s parent instead. This avoids counting inherited merge changes as unique work.

## Saved dev deploy checkout

The exact saved deploy source is `origin/wip/2026-10-22-dev-deploy-xvision`, [`8d18f537`](https://github.com/latentwill/xvision/commit/8d18f537460eff62739aee445f7b7fdb49a816d2). Its commit message records the checkout branch as `main`. Its parent is [`ed66bdf3`](https://github.com/latentwill/xvision/commit/ed66bdf36cfaa5db84c4386128f085875db6e846), the QA UI2 wave (#969), which is reachable from current main. Current main has 435 commits after that parent. No other saved WIP ref has the same tip.

The snapshot adds 12 files worth of local changes (+324/−2): provider configuration, a pre-configuration backup, Gemini prompt/filter experiments, a CLAUDE.md note, and a decision timeline patch. Keep it as deployment provenance. The timeline patch removes the declaration of `priorSideForRow` but still references it twice. That saved source cannot establish that a fresh build succeeds. The snapshot records source state, not the running process, container image, database, or runtime configuration; no live runtime verification was performed.

## Complete branch table

Every branch name below has the prefix `origin/wip/2026-10-22-`. `+/ahead` is positive cherry commits / all commits unreachable from pinned main. UNIQUE details and full changed-file lists follow.

| Branch suffix | Tip | Class | +/ahead | Recommendation | Description and evidence |
| --- | --- | --- | ---: | --- | --- |
| `dev-agent-chat-runtime` | [`94f13a3b`](https://github.com/latentwill/xvision/commit/94f13a3b88bbab061045bf8e32bcd88c17bebda1) | MERGED | 0/391 | delete-later | feat(dashboard): unify agent chat runtime; no positive cherry entries. |
| `dev-alpaca-1-asset-whitelist-bars` | [`8ed6f712`](https://github.com/latentwill/xvision/commit/8ed6f712a388caa983b4a6b024bf8cac24b26831) | SUPERSEDED | 4/408 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`9450cae4`](https://github.com/latentwill/xvision/commit/9450cae4); individual git cherry entries differ because of squash/history rewriting. |
| `dev-alpaca-2-bars-cache-cli` | [`946c9718`](https://github.com/latentwill/xvision/commit/946c97189710189883e6af76b104be5235388c41) | MERGED | 0/406 | delete-later | test(engine): lock bars cache contract; no positive cherry entries. |
| `dev-alpaca-3-scenario-registry-runner` | [`91218341`](https://github.com/latentwill/xvision/commit/9121834174eea0abacb6d405d34cf16a5572de05) | MERGED | 0/407 | delete-later | feat(engine): validate scenario v1 domain shape; no positive cherry entries. |
| `dev-alpaca-4-dashboard-scenario-authoring` | [`8a5ab17a`](https://github.com/latentwill/xvision/commit/8a5ab17a86a87d9ec59bbb771d1bf284a2f5339c) | MERGED | 0/408 | delete-later | feat(web): harden scenario authoring form; no positive cherry entries. |
| `dev-alpaca-tradingview-qa-performance` | [`224ba78b`](https://github.com/latentwill/xvision/commit/224ba78b55b3c9339415f85c78935142c97f6987) | MERGED | 0/413 | delete-later | docs: document scenario and chart workflows; no positive cherry entries. |
| `dev-app-health-sweep-2026-05-14` | [`daae0812`](https://github.com/latentwill/xvision/commit/daae08127c93438df03ba0b71a86a3e43532868b) | MERGED | 0/444 | delete-later | Lazy-load chat rail from app shells; no positive cherry entries. |
| `dev-backend-audit-20260513` | [`0037e3a6`](https://github.com/latentwill/xvision/commit/0037e3a6bf82eed8067f7e365a9a496a86737033) | SUPERSEDED | 2/392 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`28858471`](https://github.com/latentwill/xvision/commit/28858471); individual git cherry entries differ because of squash/history rewriting. |
| `dev-backup-local-main-2026-05-19` | [`5ab44c6f`](https://github.com/latentwill/xvision/commit/5ab44c6f44b7e2c790e1513dc3636ccbaa0061f1) | MERGED | 0/611 | delete-later | harness: real SHA-256 prompt_hash + response_hash on model_call spans (#277); no positive cherry entries. |
| `dev-chart-rework-klinecharts-uplot` | [`56264107`](https://github.com/latentwill/xvision/commit/5626410753f84b3ab7cc5d0177c4094137c96dce) | SUPERSEDED | 2/2 | delete-later | [`d38e1acc`](https://github.com/latentwill/xvision/commit/d38e1acc): chart-v2 foundation and six surfaces landed; later chart adapters and UI replace this initial version. |
| `dev-chore-cli-list-ls-aliases` | [`95ad4f41`](https://github.com/latentwill/xvision/commit/95ad4f419af4deaf6630afe87fe45cfa29a8f6ee) | MERGED | 0/1 | delete-later | chore(cli): add ls alias to provider/eval list; fix scenario-discovery help; no positive cherry entries. |
| `dev-chore-harness-qa-findings` | [`80f52269`](https://github.com/latentwill/xvision/commit/80f5226935c579a3ea36c136dfe543bfdbd78cb1) | MERGED | 0/1 | delete-later | docs(qa): eval backtest harness + position-communication findings; no positive cherry entries. |
| `dev-chore-help-text-reconcile` | [`c61b2397`](https://github.com/latentwill/xvision/commit/c61b23971008336de3943c78af545d2dfa61da18) | MERGED | 0/1 | delete-later | chore(cli): reconcile eval-run/model-bakeoff help with shipped behavior; no positive cherry entries. |
| `dev-codex-deploy-compile-fix` | [`669686bf`](https://github.com/latentwill/xvision/commit/669686bf01b4054d144932b8ecea21aca0fd2828) | MERGED | 0/388 | delete-later | Fix deploy compile blockers; no positive cherry entries. |
| `dev-codex-multi-asset-tool-asset-guard` | [`b1063d3b`](https://github.com/latentwill/xvision/commit/b1063d3be5584471e6a431722661162134fbaf8f) | MERGED | 0/1 | delete-later | Guard market data tools by decision asset; no positive cherry entries. |
| `dev-codex-predeploy-eval-scenario-guardrails` | [`040622db`](https://github.com/latentwill/xvision/commit/040622db5a5e25224cc91f647568a335ed46375c) | SUPERSEDED | 2/448 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`0a12e755`](https://github.com/latentwill/xvision/commit/0a12e755); individual git cherry entries differ because of squash/history rewriting. |
| `dev-codex-q8-board-closeout` | [`9843f50e`](https://github.com/latentwill/xvision/commit/9843f50e1204dc546b17697e7527a89436a1afaa) | SUPERSEDED | 14/411 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68) and [`31395cb9`](https://github.com/latentwill/xvision/commit/31395cb9): Q8 closeout plus Safari fixes integrated; remaining differences are old CLI signatures and team records. |
| `dev-codex-q8-board-docs` | [`75614196`](https://github.com/latentwill/xvision/commit/75614196558983fd5bd875d4f02f10eb9c8fe0f7) | SUPERSEDED | 4/399 | delete-later | [`e2264a3e`](https://github.com/latentwill/xvision/commit/e2264a3e) and [`50e1d694`](https://github.com/latentwill/xvision/commit/50e1d694): provider/model pairing and scenario/eval guards integrated; theme documents also landed in [`6905633c`](https://github.com/latentwill/xvision/commit/6905633c). |
| `dev-codex-q8-board-docs-clean` | [`62f82b98`](https://github.com/latentwill/xvision/commit/62f82b98868982b0467ee23f1d65bddc1869c7e5) | MERGED | 0/448 | delete-later | enforce provider/model pairs for eval preflight and chat runtime; no positive cherry entries. |
| `dev-codex-wizard-loop-compile-fix` | [`a22155b1`](https://github.com/latentwill/xvision/commit/a22155b1275b68fd4ff672dfbae4086ba01a085f) | MERGED | 0/393 | delete-later | Fix wizard loop compile after chat merge; no positive cherry entries. |
| `dev-color-themes-light-dark` | [`e73f3e6e`](https://github.com/latentwill/xvision/commit/e73f3e6eddf34331db035a0307592480748cd96a) | SUPERSEDED | 11/414 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`6905633c`](https://github.com/latentwill/xvision/commit/6905633c); individual git cherry entries differ because of squash/history rewriting. |
| `dev-conductor-2026-05-18-sweep-2` | [`937a1074`](https://github.com/latentwill/xvision/commit/937a1074221263790a36abff589a1bc4aea514e5) | MERGED | 0/612 | delete-later | conductor: 2026-05-18 sweep #2 — archive QA round 2/3 tail, V2A, harness F-1; no positive cherry entries. |
| `dev-deploy-xvision` | [`8d18f537`](https://github.com/latentwill/xvision/commit/8d18f537460eff62739aee445f7b7fdb49a816d2) | UNIQUE | 1/1 | keep | Deploy checkout: local provider config, Gemini prompt/filter experiments, and management-action display patch. |
| `dev-deploy-xvision-wt-episodic-memory` | [`002f7266`](https://github.com/latentwill/xvision/commit/002f72665db422632b64cb117cf8d185f62d1e4d) | SUPERSEDED | 2/2 | delete-later | [`f8c8a9d7`](https://github.com/latentwill/xvision/commit/f8c8a9d7): episodic memory and seed-context integration includes the branch additions. |
| `dev-deploy-xvision-wt-optimizer-enabled-models` | [`ef4d5f44`](https://github.com/latentwill/xvision/commit/ef4d5f445ab65c4f8fb0c7e8ea1e6162007fd3a0) | SUPERSEDED | 1/1 | delete-later | [`9d9cf676`](https://github.com/latentwill/xvision/commit/9d9cf676): enabled-model allowlist check landed; current optimizer dispatch evolved further. |
| `dev-deploy-xvision-wt-provider-models-filter` | [`d207f6a0`](https://github.com/latentwill/xvision/commit/d207f6a0b4b6a3e48f5eb41d8dfd438a6a06740a) | SUPERSEDED | 1/1 | delete-later | [`dc9330a1`](https://github.com/latentwill/xvision/commit/dc9330a1): provider model filtering and enable/disable/all CLI landed with revised error handling. |
| `dev-deploy-xvision-wt-tool-based-agent-capabilities` | [`af19f1d5`](https://github.com/latentwill/xvision/commit/af19f1d5049d116c038fa8c8e4c1259fd3b150ab) | SUPERSEDED | 3/3 | delete-later | [`aa27bd3a`](https://github.com/latentwill/xvision/commit/aa27bd3a): tool grants replace capabilities across the app; migration is 056 rather than this prototype 055. |
| `dev-docs-agent-training-pipeline-research` | [`2195905b`](https://github.com/latentwill/xvision/commit/2195905bd9442a15922df188390b034a923eb831) | UNIQUE | 1/1 | keep | Unmerged agent-training pipeline research and a broad docs ignore-rule change. |
| `dev-feat-episodic-memory-context` | [`002f7266`](https://github.com/latentwill/xvision/commit/002f72665db422632b64cb117cf8d185f62d1e4d) | SUPERSEDED | 2/2 | delete-later | [`f8c8a9d7`](https://github.com/latentwill/xvision/commit/f8c8a9d7): episodic memory and seed-context integration includes the branch additions. |
| `dev-feat-multi-asset-alpaca-unlock` | [`601c467a`](https://github.com/latentwill/xvision/commit/601c467aee473b9b6597b5f1a7eb72318c63608c) | MERGED | 0/1 | delete-later | feat: multi-asset Alpaca unlock — drop validate_v1 / executor walls; no positive cherry entries. |
| `dev-feat-optimizer-provider-config` | [`4b286c53`](https://github.com/latentwill/xvision/commit/4b286c5315c86770c656f9e5e7c8c14b02d2a571) | SUPERSEDED | 2/2 | delete-later | [`eb82b67d`](https://github.com/latentwill/xvision/commit/eb82b67d) and [`93c1725e`](https://github.com/latentwill/xvision/commit/93c1725e): T1 paths and T2 provider/model gate and seeded config landed; now in commands/optimize.rs. |
| `dev-feat-tool-based-agent-capabilities` | [`af19f1d5`](https://github.com/latentwill/xvision/commit/af19f1d5049d116c038fa8c8e4c1259fd3b150ab) | SUPERSEDED | 3/3 | delete-later | [`aa27bd3a`](https://github.com/latentwill/xvision/commit/aa27bd3a): tool grants replace capabilities across the app; migration is 056 rather than this prototype 055. |
| `dev-feature-chat-rail-inline-charting` | [`1bed65e8`](https://github.com/latentwill/xvision/commit/1bed65e87a7a32e013e74fdcf51220f82f35c914) | SUPERSEDED | 4/432 | delete-later | [`8d875f4e`](https://github.com/latentwill/xvision/commit/8d875f4e), [`4d7320c5`](https://github.com/latentwill/xvision/commit/4d7320c5), [`3b04811f`](https://github.com/latentwill/xvision/commit/3b04811f), [`7529c369`](https://github.com/latentwill/xvision/commit/7529c369), [`9d995a03`](https://github.com/latentwill/xvision/commit/9d995a03): rich chat blocks, cards, accessibility, and import-meta checks landed in revised form. |
| `dev-feature-logging-event-log` | [`c65d715a`](https://github.com/latentwill/xvision/commit/c65d715aecc3c9389e59c32b674b8c3fa9edab8c) | UNIQUE | 1/375 | keep | Unmerged logging and durable-event design proposal. |
| `dev-filter-fix` | [`31b063b3`](https://github.com/latentwill/xvision/commit/31b063b330a077ba05722b1416f875ebb41c80a7) | SUPERSEDED | 2/2 | delete-later | [`ce6fef18`](https://github.com/latentwill/xvision/commit/ce6fef18) and [`ff6613d6`](https://github.com/latentwill/xvision/commit/ff6613d6): wizard filter hardening and source-aware filter endpoint landed; current routes deserialize through newer APIs. |
| `dev-fix-agent-slot-prompt-version-cli` | [`9635572f`](https://github.com/latentwill/xvision/commit/9635572fba45cd673153acfb8182977a73515594) | MERGED | 0/1 | delete-later | fix(xvision-cli): add prompt_version to AgentSlot in strategy.rs; no positive cherry entries. |
| `dev-fix-autooptimizer-cycle-contract` | [`8eaf1f93`](https://github.com/latentwill/xvision/commit/8eaf1f93f7bcc93d48ee183a65af420afcc034ed) | MERGED | 0/1 | delete-later | fix(autooptimizer): require parent strategy for evening cycle; no positive cherry entries. |
| `dev-fix-chat-thread-jsdom-scroll` | [`76254d9e`](https://github.com/latentwill/xvision/commit/76254d9ee7a2cd17cb91bc86d88eff74ab9ab5a0) | MERGED | 0/435 | delete-later | fix(web): guard chat rail scroll fallback; no positive cherry entries. |
| `dev-fix-cli-agent-experience` | [`b102b1c9`](https://github.com/latentwill/xvision/commit/b102b1c90f3ceaa9f22249e79bd3beca1f82fe98) | MERGED | 0/1 | delete-later | Improve CLI validation and agent strategy guidance; no positive cherry entries. |
| `dev-fix-empty-prompt-guard` | [`5db391ad`](https://github.com/latentwill/xvision/commit/5db391ad9344909e91d8502c7e4f68c34a96d3e5) | MERGED | 0/2 | delete-later | Fix QA28 chat rail order, dedupe tool entries, clickable checkpoints; no positive cherry entries. |
| `dev-fix-eval-harness-exit-enforcement` | [`e5e9a1be`](https://github.com/latentwill/xvision/commit/e5e9a1be5cf84a1c4a44f7e22aecffd36a779173) | SUPERSEDED | 4/4 | delete-later | [`3851cfa2`](https://github.com/latentwill/xvision/commit/3851cfa2): protective exits, wake policy, and shared eval test harness replace the branch implementation. |
| `dev-fix-eval-in-position-wake-cost` | [`6836d50f`](https://github.com/latentwill/xvision/commit/6836d50f493902fca6f50c1978672152799dc738) | SUPERSEDED | 1/1 | delete-later | [`0505dd1e`](https://github.com/latentwill/xvision/commit/0505dd1e) and [`3851cfa2`](https://github.com/latentwill/xvision/commit/3851cfa2): Trip-only gating and explicit wake policy replace unconditional in-position suppression. |
| `dev-fix-example-trend-follower-agent` | [`542d8548`](https://github.com/latentwill/xvision/commit/542d85480ea6cc447e93a2d1819872219abb33a7) | SUPERSEDED | 1/1 | delete-later | [`14cd26a3`](https://github.com/latentwill/xvision/commit/14cd26a3) and [`e664515b`](https://github.com/latentwill/xvision/commit/e664515b): example trader attachment landed, then provider defaults were corrected. |
| `dev-fix-openai-compat-v1-base-url` | [`1b14bf03`](https://github.com/latentwill/xvision/commit/1b14bf0364fa4ec3b81141541f3d40215b1c7427) | SUPERSEDED | 1/1 | delete-later | [`debe2e8b`](https://github.com/latentwill/xvision/commit/debe2e8b): OpenAI-compatible base URL normalization landed through shared helpers. |
| `dev-fix-optimizer-enabled-models-check` | [`ef4d5f44`](https://github.com/latentwill/xvision/commit/ef4d5f445ab65c4f8fb0c7e8ea1e6162007fd3a0) | SUPERSEDED | 1/1 | delete-later | [`9d9cf676`](https://github.com/latentwill/xvision/commit/9d9cf676): enabled-model allowlist check landed; current optimizer dispatch evolved further. |
| `dev-fix-optimizer-xvn-home-paths` | [`b9580627`](https://github.com/latentwill/xvision/commit/b958062730b748ee2a27c83f3d9f78a6daf63e5d) | SUPERSEDED | 1/1 | delete-later | [`eb82b67d`](https://github.com/latentwill/xvision/commit/eb82b67d): XVN_HOME path fix landed; optimizer CLI subsequently moved to commands/optimize.rs and retains path_defaults_resolve_under_xvn_home. |
| `dev-fix-paper-cash-aware-gate-and-reset` | [`ba7b5cc6`](https://github.com/latentwill/xvision/commit/ba7b5cc640c13239aaf7784be45c541d29f161a6) | UNIQUE | 2/2 | keep | Cash-aware paper gate, paper-account reset script, multi-asset plan, and deploy-image pruning. |
| `dev-fix-provider-key-secrets-fallback` | [`99917bf1`](https://github.com/latentwill/xvision/commit/99917bf1d8bb18cc1710dffb6539e16634bbbfdb) | SUPERSEDED | 1/1 | delete-later | [`25a958f3`](https://github.com/latentwill/xvision/commit/25a958f3): secrets-file key fallback landed; provider resolution and optimizer dispatch were subsequently refactored. |
| `dev-fix-provider-models-allowlist-filter` | [`d207f6a0`](https://github.com/latentwill/xvision/commit/d207f6a0b4b6a3e48f5eb41d8dfd438a6a06740a) | SUPERSEDED | 1/1 | delete-later | [`dc9330a1`](https://github.com/latentwill/xvision/commit/dc9330a1): provider model filtering and enable/disable/all CLI landed with revised error handling. |
| `dev-fix-risk-show-config-paths` | [`56642dcb`](https://github.com/latentwill/xvision/commit/56642dcb8ef837def9793619900469a5be7eadd6) | MERGED | 0/1 | delete-later | fix(risk): resolve show-config defaults under \$XVN_HOME; no positive cherry entries. |
| `dev-fix-sqlite-busy-wal-busy-timeout` | [`f944afda`](https://github.com/latentwill/xvision/commit/f944afda2ea6957cab4e1dc1ba5ae038980eefc9) | MERGED | 0/1 | delete-later | fix(api): xvn.db pool — WAL + 5s busy_timeout + bounded connections; no positive cherry entries. |
| `dev-followup-gate-duplicate-opens` | [`a17a8fd4`](https://github.com/latentwill/xvision/commit/a17a8fd46902327aed982f13978d5872a34a7d81) | MERGED | 0/608 | delete-later | paper: gate duplicate long_open / short_open when position already open; no positive cherry entries. |
| `dev-help-cli-model-bakeoff-doc-fix` | [`b95599ac`](https://github.com/latentwill/xvision/commit/b95599ac48a6f0b130a2932e4056dffc5f9236bf) | SUPERSEDED | 2/2 | delete-later | [`c1be003d`](https://github.com/latentwill/xvision/commit/c1be003d): model bakeoff landed; current commands/model.rs and wiki/cli-reference.md replace the old command/help contract. |
| `dev-knip-unused-pass-2026-05-14` | [`8f58526e`](https://github.com/latentwill/xvision/commit/8f58526e29a7305d38320b27dcc2e2588ef1530d) | MERGED | 0/446 | delete-later | Remove unused frontend and Rust code; no positive cherry entries. |
| `dev-mobile-safari-load-main` | [`27230d13`](https://github.com/latentwill/xvision/commit/27230d13a595c1292e8e90a90274d97e0ae23dd6) | MERGED | 0/415 | delete-later | fix(web): guard chat rail storage for mobile safari; no positive cherry entries. |
| `dev-pr-94` | [`1b204caa`](https://github.com/latentwill/xvision/commit/1b204caa83ce3cd3024f46a02ecaca5760d3116c) | SUPERSEDED | 87/386 | delete-later | [`16d1156f`](https://github.com/latentwill/xvision/commit/16d1156f) and [`21c64d4e`](https://github.com/latentwill/xvision/commit/21c64d4e): scenario/chart and QA integration carried the bulk of this old-root branch; [`7196e31e`](https://github.com/latentwill/xvision/commit/7196e31e) and later chart-v2 replace its remaining UI/API forms. |
| `dev-qa-2026-06-14-ej-polish-fixes` | [`de0c15e6`](https://github.com/latentwill/xvision/commit/de0c15e664e6322488e4f99b7ecef8b74fae6d21) | SUPERSEDED | 5/5 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`dd747f33`](https://github.com/latentwill/xvision/commit/dd747f33); individual git cherry entries differ because of squash/history rewriting. |
| `dev-qa-pass-4-remediation` | [`c2635832`](https://github.com/latentwill/xvision/commit/c2635832c451233e076f840bc2d5af6e4235ce27) | SUPERSEDED | 1/313 | delete-later | [`f86c7052`](https://github.com/latentwill/xvision/commit/f86c7052): wizard eval-flow tests landed; no new substantive added lines remain outside main history. |
| `dev-qa30-followups` | [`a958f71e`](https://github.com/latentwill/xvision/commit/a958f71e7a1a73959d5d61a9ef610c942d909697) | MERGED | 0/1 | delete-later | QA30 follow-on: persist max_wall_ms per agent slot; no positive cherry entries. |
| `dev-qa8-agent-ux-cli-templates` | [`9e8214f3`](https://github.com/latentwill/xvision/commit/9e8214f35447278bb534d8c86a29219966574e3b) | SUPERSEDED | 7/392 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-cli-doctor-help-examples` | [`0e9a4e40`](https://github.com/latentwill/xvision/commit/0e9a4e408838810256308d4b913c04247724df10) | SUPERSEDED | 6/391 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-cli-full-object-create-validate` | [`77bb7153`](https://github.com/latentwill/xvision/commit/77bb71531a026c8667e9e25c9538d6611f50fb97) | SUPERSEDED | 4/389 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-cli-json-contracts` | [`5e341780`](https://github.com/latentwill/xvision/commit/5e34178034b73e18855cfbe678b499f0c2424e3b) | SUPERSEDED | 3/388 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-cli-noninteractive-core-flows` | [`7248d0be`](https://github.com/latentwill/xvision/commit/7248d0be3aa27d33189ff16c68b7097b77cc8495) | SUPERSEDED | 2/387 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-cli-runtime-blockers` | [`faa7e977`](https://github.com/latentwill/xvision/commit/faa7e977e02e809809af20ce5daf335dd4edca2e) | SUPERSEDED | 1/386 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-eval-cli-workflow` | [`53b3acad`](https://github.com/latentwill/xvision/commit/53b3acad7093448b04ebbcda41a25fb8bafe7b76) | SUPERSEDED | 5/390 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-eval-live-decisions` | [`c85606bc`](https://github.com/latentwill/xvision/commit/c85606bc4e8c136221af1a78e2ff392640847c0d) | MERGED | 0/386 | delete-later | Stream eval decisions to run detail; no positive cherry entries. |
| `dev-qa8-eval-provider-preflight` | [`089fecee`](https://github.com/latentwill/xvision/commit/089fecee0cd88b6bfa377470ceceba96647b7e9b) | SUPERSEDED | 2/397 | delete-later | [`1f71226e`](https://github.com/latentwill/xvision/commit/1f71226e) and [`e2264a3e`](https://github.com/latentwill/xvision/commit/e2264a3e): provider/model preflight and launch gating replace this initial UI gate. |
| `dev-qa8-inspector-agent-model-picker` | [`b8d67df9`](https://github.com/latentwill/xvision/commit/b8d67df9ce1b406c7689458a1801c4074fc5babf) | SUPERSEDED | 1/386 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-scenario-display-name-contract` | [`c8c76c73`](https://github.com/latentwill/xvision/commit/c8c76c7315f9c494b4397b7aa99d2faf203a2e01) | SUPERSEDED | 1/396 | delete-later | [`50e1d694`](https://github.com/latentwill/xvision/commit/50e1d694) and [`0a12e755`](https://github.com/latentwill/xvision/commit/0a12e755): scenario display-name and predeploy guards landed. |
| `dev-qa8-shared-chat-rail-context` | [`5915fe0a`](https://github.com/latentwill/xvision/commit/5915fe0adc2f3f88327a5c61b32f86a621516168) | SUPERSEDED | 1/386 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-strategy-table-density` | [`02652427`](https://github.com/latentwill/xvision/commit/02652427b91ae84cd8b071f81dbed9eb54d0d35b) | SUPERSEDED | 1/386 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-template-authoring-flow` | [`c27852c4`](https://github.com/latentwill/xvision/commit/c27852c4015292d558500145c1153d60215b0fe5) | SUPERSEDED | 1/386 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa8-unbounded-slot-tool-use` | [`47a2f122`](https://github.com/latentwill/xvision/commit/47a2f122593e71e1872242824fb0b8f238564644) | SUPERSEDED | 1/386 | delete-later | [`27809e68`](https://github.com/latentwill/xvision/commit/27809e68): Q8 CLI and authoring integration includes this work; current commands and UI retain the behavior with later signatures and fixtures. |
| `dev-qa9-alpaca-eval-full-run-burndown` | [`b5209b74`](https://github.com/latentwill/xvision/commit/b5209b74027662e89519ef96fe340d9d79518012) | MERGED | 0/445 | delete-later | fix(eval): harden trader output diagnostics; no positive cherry entries. |
| `dev-qa9-alpaca-eval-parser-consolidation` | [`e7f8f903`](https://github.com/latentwill/xvision/commit/e7f8f903cbe76f7e0eab6a14deac982697a574aa) | MERGED | 0/444 | delete-later | fix(eval): harden trader output diagnostics; no positive cherry entries. |
| `dev-qa9-chat-rail-inflight-controls` | [`676e2cb4`](https://github.com/latentwill/xvision/commit/676e2cb4060b72019a17343ca96a90125a357d60) | MERGED | 0/436 | delete-later | fix(web): keep chat rail editable while streaming; no positive cherry entries. |
| `dev-qa9-cli-eval-model-resolution` | [`51f4a782`](https://github.com/latentwill/xvision/commit/51f4a782dd40e2b1c95ca7cc7a14acc0375ca3b9) | MERGED | 0/399 | delete-later | fix(eval): preflight queued run model resolution; no positive cherry entries. |
| `dev-qa9-delete-edit-flow-verification` | [`4f96b48d`](https://github.com/latentwill/xvision/commit/4f96b48df5d7a9d4fa822ee65302e7b0ffdd4691) | MERGED | 0/400 | delete-later | test(web): cover scenario delete edit flows; no positive cherry entries. |
| `dev-qa9-eval-agent-prereq-surfacing` | [`b68390c1`](https://github.com/latentwill/xvision/commit/b68390c1ae3d309183adb647c8b6b341e698effa) | MERGED | 0/414 | delete-later | fix(web): surface eval agent prerequisites; no positive cherry entries. |
| `dev-qa9-eval-agent-prereq-surfacing-rebased` | [`c0da89af`](https://github.com/latentwill/xvision/commit/c0da89afb9a5164eb8afb925c795b98963850c8d) | MERGED | 0/576 | delete-later | fix scenario chart timeframe controls (#247); no positive cherry entries. |
| `dev-qa9-json-schema-enforcement` | [`dbd9538f`](https://github.com/latentwill/xvision/commit/dbd9538f6b2820951bac978be8f2dd02bcecaff0) | MERGED | 0/442 | delete-later | enforce eval JSON schemas; no positive cherry entries. |
| `dev-qa9-readonly-editability-contract` | [`f25a9b11`](https://github.com/latentwill/xvision/commit/f25a9b1125cc29b0e636c4d8053e9f5ec56f4acb) | MERGED | 0/443 | delete-later | Merge remote-tracking branch 'origin/main' into qa9-readonly-editability-contract; no positive cherry entries. |
| `dev-qa9-scenario-dedupe-api` | [`c0b62467`](https://github.com/latentwill/xvision/commit/c0b624677b94aed3687802360d67086082f1424e) | MERGED | 0/396 | delete-later | fix(scenarios): reject duplicate active scenario names; no positive cherry entries. |
| `dev-qa9-strategy-agent-attachment-flow` | [`50e3eaa2`](https://github.com/latentwill/xvision/commit/50e3eaa2c2bdcae9db5a75a2e188ab5176c48ff9) | SUPERSEDED | 2/404 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`85c9fae4`](https://github.com/latentwill/xvision/commit/85c9fae4); individual git cherry entries differ because of squash/history rewriting. |
| `dev-qa9-strategy-create-agent-guardrail` | [`87ccb164`](https://github.com/latentwill/xvision/commit/87ccb1649551a8412a78d95f23c28f9d283d460e) | MERGED | 0/441 | delete-later | Merge remote-tracking branch 'origin/main' into qa9-strategy-create-agent-guardrail; no positive cherry entries. |
| `dev-qa9-strategy-delete-endpoint` | [`a83c1a23`](https://github.com/latentwill/xvision/commit/a83c1a238ef67c6d430726755e123cc56ae7a76b) | MERGED | 0/397 | delete-later | feat(strategies): add strategy delete endpoint; no positive cherry entries. |
| `dev-qa9-strategy-validation-drift` | [`ad64dbac`](https://github.com/latentwill/xvision/commit/ad64dbac89e551f80e08dbf85efb43d34ad57d3a) | MERGED | 0/398 | delete-later | fix(strategy): report prompt manifest validation drift; no positive cherry entries. |
| `dev-qa9-strategy-wizard-persistence` | [`93324502`](https://github.com/latentwill/xvision/commit/933245020d674847bf1d1d4487dd07f89d7d83e8) | MERGED | 0/441 | delete-later | Merge remote-tracking branch 'origin/main' into qa9-strategy-wizard-persistence; no positive cherry entries. |
| `dev-rust-unused-deps-actions-2026-05-14` | [`8f8422cd`](https://github.com/latentwill/xvision/commit/8f8422cdd81304268499ba330e95c5686144786c) | MERGED | 0/436 | delete-later | Add Rust unused dependency audit workflow; no positive cherry entries. |
| `dev-task-agent-error-feedback-self-healing` | [`09e99eb7`](https://github.com/latentwill/xvision/commit/09e99eb7b3956629c4ed96dc0091d0c7dc76527b) | SUPERSEDED | 2/605 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`1019fe50`](https://github.com/latentwill/xvision/commit/1019fe50); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-autooptimizer-ui-align` | [`8658b168`](https://github.com/latentwill/xvision/commit/8658b1686052d21413f7779ab0c74deffb09739c) | SUPERSEDED | 1/1 | delete-later | [`14577ec1`](https://github.com/latentwill/xvision/commit/14577ec1) and [`d01a549f`](https://github.com/latentwill/xvision/commit/d01a549f): optimizer lineage UI followed by the three-screen optimizer redesign supersede this alternate dashboard implementation. |
| `dev-task-chart-vertical-snap-to-fit` | [`929822d5`](https://github.com/latentwill/xvision/commit/929822d5e58edb12d09d57ed59cac24b852a3215) | MERGED | 0/1 | delete-later | chart-vertical-snap-to-fit: re-fit price axis on visible-range change (F-5); no positive cherry entries. |
| `dev-task-chat-history-auto-title` | [`577293a1`](https://github.com/latentwill/xvision/commit/577293a1df7fbd39180fa79d96ce32fecd99b030) | SUPERSEDED | 2/599 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`6b523f39`](https://github.com/latentwill/xvision/commit/6b523f39); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-chat-rail-strategy-list-refresh` | [`d2fd08af`](https://github.com/latentwill/xvision/commit/d2fd08afdc70bfb3dc07651e64c7c97196f257c9) | MERGED | 0/597 | delete-later | fix(chat-rail): truthy-error gate + agent invalidation on create_strategy; no positive cherry entries. |
| `dev-task-cli-strategy-clone-model-override` | [`58ac4827`](https://github.com/latentwill/xvision/commit/58ac4827c22f4c0af14d7d8822ba15eff017500a) | SUPERSEDED | 2/5 | delete-later | [`ff6613d6`](https://github.com/latentwill/xvision/commit/ff6613d6): source-aware filter endpoint landed. CLI cloning now delegates to api::strategy::clone_strategy_full, including cleanup_created_clone_agents; old CLI-local cleanup and fixtures are superseded. |
| `dev-task-container-config-path-papercut` | [`bec654db`](https://github.com/latentwill/xvision/commit/bec654db79b0d5c219f6a9dc9ffb21e30b2185c7) | MERGED | 0/1 | delete-later | Track: container-config-path-papercut — drop XVN_CONFIG_PATH from image ENV so docker-exec sessions see writable config; no positive cherry entries. |
| `dev-task-docs-harness-audit-prompt` | [`5ea1ffda`](https://github.com/latentwill/xvision/commit/5ea1ffda99d22687693a0d08be234edf9557f251) | MERGED | 0/595 | delete-later | docs: add harness audit prompt v3; no positive cherry entries. |
| `dev-task-eval-guardrail-log-collapse` | [`4fdbd14a`](https://github.com/latentwill/xvision/commit/4fdbd14a10fb4763a576b0b6cb8ea8c0e817d605) | SUPERSEDED | 1/1 | delete-later | [`61058260`](https://github.com/latentwill/xvision/commit/61058260): guardrail log collapsing landed with revised finding metadata and test fixtures. |
| `dev-task-eval-honesty-smell-tests` | [`6f82de8c`](https://github.com/latentwill/xvision/commit/6f82de8c95a4662ba907840329e8365dd9be2d2a) | SUPERSEDED | 1/1 | delete-later | [`fc967e36`](https://github.com/latentwill/xvision/commit/fc967e36): uniformity smell checks landed with revised fixtures. |
| `dev-task-eval-provider-attestation` | [`1d3b86d2`](https://github.com/latentwill/xvision/commit/1d3b86d25f79a7f6cbe823fff272635392270810) | SUPERSEDED | 1/1 | delete-later | [`a8fd1fb4`](https://github.com/latentwill/xvision/commit/a8fd1fb4): provider/model attestation landed with revised export format. |
| `dev-task-eval-provider-preflight` | [`3821fefe`](https://github.com/latentwill/xvision/commit/3821fefeb16fba2423cfee13087709cea1af8472) | SUPERSEDED | 1/1 | delete-later | [`ebc2d7a6`](https://github.com/latentwill/xvision/commit/ebc2d7a6): provider reachability preflight landed with revised route fixtures. |
| `dev-task-eval-review-profile-editable` | [`15afe3cb`](https://github.com/latentwill/xvision/commit/15afe3cb1d8783fd4fdf4a456f0c65bb6b2c47ef) | SUPERSEDED | 2/2 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`ab497986`](https://github.com/latentwill/xvision/commit/ab497986); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-fix-scrollbar-stable` | [`d3579b8a`](https://github.com/latentwill/xvision/commit/d3579b8a31cf54fa7379f625eb3a66e6dcb269d9) | MERGED | 0/591 | delete-later | fix(scenario): bars estimate reacts to Context bars input (#269); no positive cherry entries. |
| `dev-task-fix-streaming-legacy-fallback` | [`a6c9c4f9`](https://github.com/latentwill/xvision/commit/a6c9c4f9432a82b24ca0885665566742eedf45f7) | MERGED | 0/588 | delete-later | fix(span-inspector): preserve legacy span.streaming streaming indicator; no positive cherry entries. |
| `dev-task-harness-prompt-version-field` | [`8dbb62c4`](https://github.com/latentwill/xvision/commit/8dbb62c4d8c06b1a513b9b68f074bb5e3c757095) | MERGED | 0/1 | delete-later | harness-prompt-version-field: add agent_slots.prompt_version (F-3); no positive cherry entries. |
| `dev-task-harness-recovery-state-machine` | [`3c723869`](https://github.com/latentwill/xvision/commit/3c7238698b4a41a7cf73e821997d1eb213467122) | SUPERSEDED | 2/6 | delete-later | [`177f0e3c`](https://github.com/latentwill/xvision/commit/177f0e3c) and [`258732b0`](https://github.com/latentwill/xvision/commit/258732b0): typed recovery dispatcher and context-overflow recovery replace the executor-local prototype. |
| `dev-task-harness-span-attrs-populate` | [`14a2e79b`](https://github.com/latentwill/xvision/commit/14a2e79bbcba6e695165f30708ebb4f8475b4f30) | MERGED | 0/613 | delete-later | harness-span-attrs-populate: populate SpanStartedEvent.attributes_json; no positive cherry entries. |
| `dev-task-harness-span-attrs-populate-v2` | [`ee1fe09e`](https://github.com/latentwill/xvision/commit/ee1fe09e0f9af28bca2b23030271c3105d15b12e) | MERGED | 0/1 | delete-later | harness-span-attrs-populate: populate SpanStartedEvent.attributes_json; no positive cherry entries. |
| `dev-task-qa-eval-action-lifecycle` | [`0615f2c6`](https://github.com/latentwill/xvision/commit/0615f2c6e0f92475a4201a551ee29f961991daf3) | SUPERSEDED | 2/581 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`5bfa8ccb`](https://github.com/latentwill/xvision/commit/5bfa8ccb); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-qa-retention-prompt-storage-bug` | [`9b5c74c6`](https://github.com/latentwill/xvision/commit/9b5c74c61558d62916a19faf309ec890cd3efee6) | MERGED | 0/600 | delete-later | chat-history-auto-title: auto-summarize chat history titles (ChatGPT-style) (#280); no positive cherry entries. |
| `dev-task-qa-trace-broker-spans` | [`35e0690d`](https://github.com/latentwill/xvision/commit/35e0690dec094734ca8fb81b0502547d68280faf) | SUPERSEDED | 2/602 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`c34b5bbb`](https://github.com/latentwill/xvision/commit/c34b5bbb); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-qa-trace-dock-resizable` | [`e2165e1a`](https://github.com/latentwill/xvision/commit/e2165e1a9697e506fd9e32b14faf89d7b508718a) | SUPERSEDED | 1/582 | delete-later | [`e9a67061`](https://github.com/latentwill/xvision/commit/e9a67061): resizable trace dock landed with revised handle and store tests. |
| `dev-task-qa-ui-polish-round2` | [`d1626bf4`](https://github.com/latentwill/xvision/commit/d1626bf4d69de3100db61a52949b0ae1a3b14218) | SUPERSEDED | 3/586 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`ab58e67b`](https://github.com/latentwill/xvision/commit/ab58e67b); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-risk-sees-conviction` | [`59a43284`](https://github.com/latentwill/xvision/commit/59a432846d66c38a72498936960ea6717da7044d) | SUPERSEDED | 1/1 | delete-later | [`7b6596c9`](https://github.com/latentwill/xvision/commit/7b6596c9): conviction risk context landed; legacy xvision-risk crate subsequently removed. |
| `dev-task-round4-selective-reset` | [`a546f3ef`](https://github.com/latentwill/xvision/commit/a546f3ef74544ad01709e109076f8a6dba179625) | SUPERSEDED | 2/2 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`a0ef8c99`](https://github.com/latentwill/xvision/commit/a0ef8c99); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-round4-wizard-broker-followups` | [`59dea15b`](https://github.com/latentwill/xvision/commit/59dea15b79813ec19decb77464842cadb4dd47c2) | MERGED | 0/1 | delete-later | round4-wizard-broker-followups: scenario variant guardrails + Alpaca min-notional classifier; no positive cherry entries. |
| `dev-task-scenario-bars-estimate-ui` | [`126881f0`](https://github.com/latentwill/xvision/commit/126881f04a1e4b157dea7f57ef7fa26abfe38761) | MERGED | 0/590 | delete-later | fix(scenario): bars estimate reacts to Context bars input; no positive cherry entries. |
| `dev-task-seed-scaffolding-cleanup` | [`ed2b6e6e`](https://github.com/latentwill/xvision/commit/ed2b6e6ebd8f060d24382cad2e4561647d6f056c) | SUPERSEDED | 1/1 | delete-later | [`ab21de88`](https://github.com/latentwill/xvision/commit/ab21de88): scaffold-seed cleanup landed with updated seeded-artifact tests. |
| `dev-task-trace-dock-hide-stub` | [`799ff081`](https://github.com/latentwill/xvision/commit/799ff0819a22ed620a96b5be1b520da72b26b98b) | MERGED | 0/1 | delete-later | fix(trace-dock): hide state.transition stub in Advanced view too; no positive cherry entries. |
| `dev-task-track-plan-touches` | [`427be7ec`](https://github.com/latentwill/xvision/commit/427be7ec1f3a6914498bc46bf577c287fbaeb904) | SUPERSEDED | 1/1 | delete-later | [`e80e1595`](https://github.com/latentwill/xvision/commit/e80e1595): filter runtime/state/events landed with revised indicator implementation. |
| `dev-task-track-plan-touches-engine` | [`c968e35f`](https://github.com/latentwill/xvision/commit/c968e35f708affac5e704531f6642c7138c71a07) | SUPERSEDED | 2/2 | delete-later | [`e80e1595`](https://github.com/latentwill/xvision/commit/e80e1595) and [`1a70eec6`](https://github.com/latentwill/xvision/commit/1a70eec6): filter runtime and backtest wiring landed with revised migration/fixtures. |
| `dev-task-trader-noop-skip` | [`aed6d40d`](https://github.com/latentwill/xvision/commit/aed6d40d5f9ab9fbbf95d93039f13a4376acd336) | SUPERSEDED | 1/1 | delete-later | [`32ef94e9`](https://github.com/latentwill/xvision/commit/32ef94e9): trader no-op suppression landed with updated token/role fixtures. |
| `dev-task-ui-scrollbars-always-visible` | [`6c3964f3`](https://github.com/latentwill/xvision/commit/6c3964f39d2d5e19c665d9c83171669e524944ec) | SUPERSEDED | 2/593 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`9e94ada2`](https://github.com/latentwill/xvision/commit/9e94ada2); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-v2a-driver-tour` | [`55c0c7f8`](https://github.com/latentwill/xvision/commit/55c0c7f865d4ed20f2038bb688c116240985f543) | SUPERSEDED | 2/579 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`2886bdfb`](https://github.com/latentwill/xvision/commit/2886bdfb); individual git cherry entries differ because of squash/history rewriting. |
| `dev-task-v2a-in-app-docs` | [`72998072`](https://github.com/latentwill/xvision/commit/729980723728b7b6bd77533f7a8e96873d280d92) | SUPERSEDED | 1/599 | delete-later | [`9b4a5a96`](https://github.com/latentwill/xvision/commit/9b4a5a96): in-app docs route landed; content now comes from crates/xvision-dashboard/wiki. |
| `dev-task-watcher-v1-shape` | [`4731a7dd`](https://github.com/latentwill/xvision/commit/4731a7dd99424f093717242836f4ed0f249074f6) | UNIQUE | 1/1 | keep | Unmerged watcher-v1 shape specification, implementation plan, and team contract. |
| `dev-task-wizard-normalizer-cleanup` | [`769828bd`](https://github.com/latentwill/xvision/commit/769828bd8837dffbcd5a329ecd81695121c2a64e) | UNIQUE | 2/2 | salvage | Tighten create_scenario JSON schema and remove Qwen-specific normalization repairs. |
| `dev-tradingview-1-chart-api-indicators` | [`8e8d6355`](https://github.com/latentwill/xvision/commit/8e8d635552cf83e072e9fbb4f188a850b4797acc) | MERGED | 0/409 | delete-later | test(engine): cover cached scenario chart indicators; no positive cherry entries. |
| `dev-tradingview-2-run-compare-charts` | [`2721450b`](https://github.com/latentwill/xvision/commit/2721450b5af8a8c060173e84ab88c2cabbda4987) | SUPERSEDED | 1/410 | delete-later | [`94226ad6`](https://github.com/latentwill/xvision/commit/94226ad6): chart table fallback landed; remaining difference is old team bookkeeping. |
| `dev-tradingview-3-scenario-strategy-charts` | [`5e74e63a`](https://github.com/latentwill/xvision/commit/5e74e63a3d6e1f11ee10cc959412502698687dbf) | MERGED | 0/411 | delete-later | feat(web): add scenario chart table fallback; no positive cherry entries. |
| `dev-tradingview-4-live-wizard-preview` | [`2bf009b9`](https://github.com/latentwill/xvision/commit/2bf009b913eedbe515941314975b908f0b84172f) | MERGED | 0/412 | delete-later | feat(web): merge live indicator tail events; no positive cherry entries. |
| `dev-v2a-driver-tour-infrastructure` | [`1a71cdfa`](https://github.com/latentwill/xvision/commit/1a71cdfa685629d5ac67b94e779016f765e3d600) | SUPERSEDED | 1/439 | delete-later | [`2886bdfb`](https://github.com/latentwill/xvision/commit/2886bdfb) and [`4031f6b4`](https://github.com/latentwill/xvision/commit/4031f6b4): shipped first-run tour and later tour redesign supersede the old tours/ infrastructure. |
| `dev-xvn-wt-t1` | [`b9580627`](https://github.com/latentwill/xvision/commit/b958062730b748ee2a27c83f3d9f78a6daf63e5d) | SUPERSEDED | 1/1 | delete-later | [`eb82b67d`](https://github.com/latentwill/xvision/commit/eb82b67d): XVN_HOME path fix landed; optimizer CLI subsequently moved to commands/optimize.rs and retains path_defaults_resolve_under_xvn_home. |
| `dev-xvn-wt-t2` | [`4b286c53`](https://github.com/latentwill/xvision/commit/4b286c5315c86770c656f9e5e7c8c14b02d2a571) | SUPERSEDED | 2/2 | delete-later | [`eb82b67d`](https://github.com/latentwill/xvision/commit/eb82b67d) and [`93c1725e`](https://github.com/latentwill/xvision/commit/93c1725e): T1 paths and T2 provider/model gate and seeded config landed; now in commands/optimize.rs. |
| `dev-xvn-wt-t3` | [`99917bf1`](https://github.com/latentwill/xvision/commit/99917bf1d8bb18cc1710dffb6539e16634bbbfdb) | SUPERSEDED | 1/1 | delete-later | [`25a958f3`](https://github.com/latentwill/xvision/commit/25a958f3): secrets-file key fallback landed; provider resolution and optimizer dispatch were subsequently refactored. |
| `dev-xvn-wt-t4` | [`56642dcb`](https://github.com/latentwill/xvision/commit/56642dcb8ef837def9793619900469a5be7eadd6) | MERGED | 0/1 | delete-later | fix(risk): resolve show-config defaults under \$XVN_HOME; no positive cherry entries. |
| `dev-xvn-wt-t5` | [`95ad4f41`](https://github.com/latentwill/xvision/commit/95ad4f419af4deaf6630afe87fe45cfa29a8f6ee) | MERGED | 0/1 | delete-later | chore(cli): add ls alias to provider/eval list; fix scenario-discovery help; no positive cherry entries. |
| `dev-xvn-wt-t7` | [`542d8548`](https://github.com/latentwill/xvision/commit/542d85480ea6cc447e93a2d1819872219abb33a7) | SUPERSEDED | 1/1 | delete-later | [`14cd26a3`](https://github.com/latentwill/xvision/commit/14cd26a3) and [`e664515b`](https://github.com/latentwill/xvision/commit/e664515b): example trader attachment landed, then provider defaults were corrected. |
| `dev-xvn-wt-t8` | [`c61b2397`](https://github.com/latentwill/xvision/commit/c61b23971008336de3943c78af545d2dfa61da18) | MERGED | 0/1 | delete-later | chore(cli): reconcile eval-run/model-bakeoff help with shipped behavior; no positive cherry entries. |
| `dev-xvn-xvision-src-wt-qa-polish` | [`de0c15e6`](https://github.com/latentwill/xvision/commit/de0c15e664e6322488e4f99b7ecef8b74fae6d21) | SUPERSEDED | 5/5 | delete-later | Aggregate branch delta has the same stable patch ID as main commit [`dd747f33`](https://github.com/latentwill/xvision/commit/dd747f33); individual git cherry entries differ because of squash/history rewriting. |
| `olympus-hermes-xvision` | [`3a643c49`](https://github.com/latentwill/xvision/commit/3a643c4937787f4eff6da4a7697215baca09533a) | UNIQUE | 4/4 | keep | GHCR wiki-context notes retain unique planning/intake edits; filter QA content already landed. |
| `olympus-hermes-xvision-clawpatch` | [`f8690f05`](https://github.com/latentwill/xvision/commit/f8690f05279ff8a22cd63e2c068f2a6cfa755617) | UNIQUE | 2/2 | salvage | Clawpatch leftovers: file-backed SQLite fixtures, deterministic janitor times, slot reset, preview-on-demand, and mobile drawer changes. |
| `olympus-home-xvision` | [`73b0594d`](https://github.com/latentwill/xvision/commit/73b0594d941e4009a85cedf586ff9c9f4d8b8933) | UNIQUE | 1/3 | keep | Large retained research corpus, free-model configs, wiki additions, skill files, and visual experiments. |
| `olympus-ows-xvision` | [`0c55a989`](https://github.com/latentwill/xvision/commit/0c55a9894d856ef2ed71d80b4dd7e0558deb0985) | UNIQUE | 3/3 | keep | Mixed Olympus audit/docs archive, scenario clone/UI edits, and scratch strategy experiments. |
| `olympus-xvision-ar2-t9-followup` | [`feb6c3b0`](https://github.com/latentwill/xvision/commit/feb6c3b034430db3c80a383333d95ce8aa2a340a) | SUPERSEDED | 1/1 | delete-later | [`d99a9ed5`](https://github.com/latentwill/xvision/commit/d99a9ed5) and [`27e8b74b`](https://github.com/latentwill/xvision/commit/27e8b74b): evening-cycle command and smoke test landed separately; autoresearch CLI was replaced by optimize.rs and current autooptimizer cycle tests. |

## UNIQUE work retained for review

Pass results below refer to the current salvage checkout. They do not validate an archived branch. No UNIQUE branch was merged wholesale.

### dev-deploy-xvision

Deploy checkout: local provider config, Gemini prompt/filter experiments, and management-action display patch.

Recommendation: **keep**. Positive cherry commits: **1**; all unreachable commits: **1**. Review delta: **12 files, +324/−2 lines**, from baseline [`ed66bdf3`](https://github.com/latentwill/xvision/commit/ed66bdf36cfaa5db84c4386128f085875db6e846).

Tests: Snapshot adds no tests. Current decision-view tests pass; snapshot not executed. Its decision-view.ts references priorSideForRow after removing its declaration, so this source is not a verified runnable build.

Non-equivalent commits: [`8d18f537`](https://github.com/latentwill/xvision/commit/8d18f537460eff62739aee445f7b7fdb49a816d2).

<details>
<summary>Changed files</summary>

- `CLAUDE.md`
- `config/default.toml`
- `config/default.toml.pre-zen`
- `frontend/web/src/components/eval-detail/decision-view.ts`
- `xvn-work/gemini_adx_flip_prompt.md`
- `xvn-work/gemini_adx_trend_filter.json`
- `xvn-work/gemini_long_gate_prompt.md`
- `xvn-work/gemini_short_gate_prompt.md`
- `xvn-work/gemini_short_scalp_prompt.md`
- `xvn-work/gemini_sparse_momentum_filter.json`
- `xvn-work/gemini_sparse_momentum_prompt.md`
- `xvn-work/gemini_sparse_short_filter.json`

</details>

### dev-docs-agent-training-pipeline-research

Unmerged agent-training pipeline research and a broad docs ignore-rule change.

Recommendation: **keep**. Positive cherry commits: **1**; all unreachable commits: **1**. Review delta: **2 files, +99/−5 lines**, from baseline [`0a4ff0c0`](https://github.com/latentwill/xvision/commit/0a4ff0c02fd547a77f577f4f5f6538ee6108f4af).

Tests: Research only; no executable tests added. Do not restore the broad .gitignore edit.

Non-equivalent commits: [`2195905b`](https://github.com/latentwill/xvision/commit/2195905bd9442a15922df188390b034a923eb831).

<details>
<summary>Changed files</summary>

- `.gitignore`
- `docs/research/xvision-agent-training-pipeline.md`

</details>

### dev-feature-logging-event-log

Unmerged logging and durable-event design proposal.

Recommendation: **keep**. Positive cherry commits: **1**; all unreachable commits: **375**. Review delta: **1 files, +403/−0 lines**, from baseline [`3f85dee2`](https://github.com/latentwill/xvision/commit/3f85dee2383040ce904cfc1ef47c46bbcb70e4ff).

Tests: Design only; no executable tests.

Non-equivalent commits: [`c65d715a`](https://github.com/latentwill/xvision/commit/c65d715aecc3c9389e59c32b674b8c3fa9edab8c).

<details>
<summary>Changed files</summary>

- `docs/superpowers/specs/2026-05-13-logging-and-durable-events-v0-design.md`

</details>

### dev-fix-paper-cash-aware-gate-and-reset

Cash-aware paper gate, paper-account reset script, multi-asset plan, and deploy-image pruning.

Recommendation: **keep**. Positive cherry commits: **2**; all unreachable commits: **2**. Review delta: **4 files, +377/−14 lines**, from baseline [`1d6a01ff`](https://github.com/latentwill/xvision/commit/1d6a01ffd502cef7565bde14447bd4e206626c21).

Tests: No tests added for the reset script; its bash -n syntax check passes. Paper engine has tests, but this snapshot was not run. Script contacts a broker and was not executed.

Non-equivalent commits: [`365dc14e`](https://github.com/latentwill/xvision/commit/365dc14ebdb85676039dbfc1cc120071d9fbb698), [`ba7b5cc6`](https://github.com/latentwill/xvision/commit/ba7b5cc640c13239aaf7784be45c541d29f161a6).

<details>
<summary>Changed files</summary>

- `crates/xvision-engine/src/eval/executor/paper.rs`
- `docs/superpowers/plans/2026-05-19-multi-asset-portfolio-allocation.md`
- `scripts/alpaca-paper-reset.sh`
- `scripts/deploy-image.sh`

</details>

### dev-task-watcher-v1-shape

Unmerged watcher-v1 shape specification, implementation plan, and team contract.

Recommendation: **keep**. Positive cherry commits: **1**; all unreachable commits: **1**. Review delta: **3 files, +884/−0 lines**, from baseline [`4d08bae9`](https://github.com/latentwill/xvision/commit/4d08bae91a6374241179e9e09aefdb0dcb88ba4e).

Tests: Design only; no executable tests.

Non-equivalent commits: [`4731a7dd`](https://github.com/latentwill/xvision/commit/4731a7dd99424f093717242836f4ed0f249074f6).

<details>
<summary>Changed files</summary>

- `docs/superpowers/plans/2026-05-21-watcher-v1-implementation.md`
- `docs/superpowers/specs/2026-05-21-watcher-v1-shape.md`
- `team/contracts/watcher-v1-shape.md`

</details>

### dev-task-wizard-normalizer-cleanup

Tighten create_scenario JSON schema and remove Qwen-specific normalization repairs.

Recommendation: **salvage**. Positive cherry commits: **2**; all unreachable commits: **2**. Review delta: **1 files, +522/−203 lines**, from baseline [`aed9fd8e`](https://github.com/latentwill/xvision/commit/aed9fd8e5859fe5c79c53f8412b5b4b596c9c757).

Tests: Adds inline Rust schema regression tests in wizard_loop.rs. Snapshot not run; current dashboard tests cannot compile because GateRecord lacks holdout_epsilon.

Non-equivalent commits: [`86b002af`](https://github.com/latentwill/xvision/commit/86b002af9f06017ee86563b0c24901256cd952e6), [`769828bd`](https://github.com/latentwill/xvision/commit/769828bd8837dffbcd5a329ecd81695121c2a64e).

<details>
<summary>Changed files</summary>

- `crates/xvision-dashboard/src/wizard_loop.rs`

</details>

### olympus-hermes-xvision

GHCR wiki-context notes retain unique planning/intake edits; filter QA content already landed.

Recommendation: **keep**. Positive cherry commits: **4**; all unreachable commits: **4**. Review delta: **4 files, +326/−5 lines**, from baseline [`57505cba`](https://github.com/latentwill/xvision/commit/57505cbaced0b9987da64330a8fc9fe606a4850e).

Tests: Documentation/config only; no tests added. The wiki build layout has since moved; keep as historical context.

Non-equivalent commits: [`882e27ce`](https://github.com/latentwill/xvision/commit/882e27ce36dcaa1348867bf3dc600acd27b1c15f), [`6385253d`](https://github.com/latentwill/xvision/commit/6385253d376a9ff38ea16f832d871aec5155aa11), [`5e53f240`](https://github.com/latentwill/xvision/commit/5e53f240545811d2b86f863950bf052fa25729e9), [`3a643c49`](https://github.com/latentwill/xvision/commit/3a643c4937787f4eff6da4a7697215baca09533a).

<details>
<summary>Changed files</summary>

- `.dockerignore`
- `docs/QA/2026-05-23-filter-strategy-eval-efficiency.md`
- `docs/superpowers/plans/2026-05-12-ghcr-build-optimization.md`
- `team/intake/2026-05-20-docs-user-and-agent-wiki.md`

</details>

### olympus-hermes-xvision-clawpatch

Clawpatch leftovers: file-backed SQLite fixtures, deterministic janitor times, slot reset, preview-on-demand, and mobile drawer changes.

Recommendation: **salvage**. Positive cherry commits: **2**; all unreachable commits: **2**. Review delta: **18 files, +361/−90 lines**, from baseline [`57505cba`](https://github.com/latentwill/xvision/commit/57505cbaced0b9987da64330a8fc9fe606a4850e).

Tests: Adds/edits Rust integration tests and agents.test.tsx / WizardPreviewChart.test.tsx. Current frontend suite passes; snapshot not run. Review UX changes separately from test-only fixes.

Non-equivalent commits: [`882e27ce`](https://github.com/latentwill/xvision/commit/882e27ce36dcaa1348867bf3dc600acd27b1c15f), [`f8690f05`](https://github.com/latentwill/xvision/commit/f8690f05279ff8a22cd63e2c068f2a6cfa755617).

<details>
<summary>Changed files</summary>

- `.dockerignore`
- `crates/xvision-engine/src/eval/export.rs`
- `crates/xvision-engine/tests/api_eval.rs`
- `crates/xvision-engine/tests/api_eval_attest.rs`
- `crates/xvision-engine/tests/api_eval_compare.rs`
- `crates/xvision-engine/tests/api_eval_min_notional.rs`
- `crates/xvision-engine/tests/api_eval_run.rs`
- `crates/xvision-engine/tests/eval_attestation.rs`
- `crates/xvision-engine/tests/eval_retry_from_completed.rs`
- `crates/xvision-engine/tests/eval_retry_idempotency.rs`
- `crates/xvision-engine/tests/eval_store.rs`
- `crates/xvision-observability/tests/janitor.rs`
- `frontend/web/src/components/agent/AgentForm.tsx`
- `frontend/web/src/components/agent/SlotForm.tsx`
- `frontend/web/src/components/agent/agents.test.tsx`
- `frontend/web/src/components/chart/WizardPreviewChart.test.tsx`
- `frontend/web/src/components/chart/WizardPreviewChart.tsx`
- `frontend/web/src/components/mobile/MobileDrawer.tsx`

</details>

### olympus-home-xvision

Large retained research corpus, free-model configs, wiki additions, skill files, and visual experiments.

Recommendation: **keep**. Positive cherry commits: **1**; all unreachable commits: **3**. Review delta: **119 files, +139455/−0 lines**, from baseline [`8294251b`](https://github.com/latentwill/xvision/commit/8294251bfe98e87c5513f496b09802709f4581b9).

Tests: No dedicated executable regression tests in the non-equivalent snapshot commit. Research/config archive not executed.

Non-equivalent commits: [`73b0594d`](https://github.com/latentwill/xvision/commit/73b0594d941e4009a85cedf586ff9c9f4d8b8933).

<details>
<summary>Changed files</summary>

- `config/autooptimizer-free-long.toml`
- `config/autooptimizer-free.toml`
- `crates/xvision-dashboard/wiki/degen-virtuals-agent-investigation.md`
- `crates/xvision-dashboard/wiki/index.toml`
- `crates/xvision-dashboard/wiki/kinamix-academy-ai-course-landscape.md`
- `crates/xvision-dashboard/wiki/roadmap.md`
- `docs/competitor/elfa-ai.md`
- `docs/dev/skills/README.md`
- `docs/dev/skills/trueorigin/SKILL.md`
- `docs/operator/free-model-catalog.md`
- `docs/operator/free-only-strategy-expansion-audit.md`
- `docs/operator/free-only-strategy-expansion.md`
- `docs/plans/2026-06-18-001-fix-worktree-build-and-fmt-churn-plan.md`
- `docs/plans/2026-06-23-002-slow-llm-graceful-handling-plan.md`
- `docs/pythonscripts.zip`
- `docs/testnft/dspy-mono-crosshatch.html`
- `docs/testnft/dspy-seed-generator.html`
- `docs/testnft/xvision-bauhaus-strategy-metrics.html`
- `docs/testnft/xvision-dspy-color-field.html`
- `docs/testnft/xvision-riso-lead-poster.html`
- `docs/testnft/xvision-riso-multi-strategy-sheet.html`
- `research/100x/onboarding-ux-presentation-2026-05-31/phase_1_results.json`
- `research/100x/onboarding-ux-presentation-2026-05-31/phase_1_synthesis.md`
- `research/100x/onboarding-ux-presentation-2026-05-31/pipeline.log`
- `research/100x/onboarding-ux-presentation-2026-05-31/pipeline_report.json`
- `research/degen-virtuals-2026-06-09/100x-agent-investigations/phase_1_results.json`
- `research/degen-virtuals-2026-06-09/100x-agent-investigations/phase_1_synthesis.md`
- `research/degen-virtuals-2026-06-09/100x-agent-investigations/pipeline.log`
- `research/degen-virtuals-2026-06-09/100x-agent-investigations/pipeline_report.json`
- `research/degen-virtuals-2026-06-09/100x_tasks.json`
- `research/degen-virtuals-2026-06-09/100x_tasks_phased.json`
- `research/degen-virtuals-2026-06-09/agent_01_958.json`
- `research/degen-virtuals-2026-06-09/agent_02_1168.json`
- `research/degen-virtuals-2026-06-09/agent_03_1138.json`
- `research/degen-virtuals-2026-06-09/agent_04_1147.json`
- `research/degen-virtuals-2026-06-09/agent_05_1129.json`
- `research/degen-virtuals-2026-06-09/agent_06_1119.json`
- `research/degen-virtuals-2026-06-09/agent_07_1085.json`
- `research/degen-virtuals-2026-06-09/agent_08_998.json`
- `research/degen-virtuals-2026-06-09/agent_09_862.json`
- `research/degen-virtuals-2026-06-09/agent_10_1142.json`
- `research/degen-virtuals-2026-06-09/agent_11_873.json`
- `research/degen-virtuals-2026-06-09/agent_12_1074.json`
- `research/degen-virtuals-2026-06-09/agent_13_779.json`
- `research/degen-virtuals-2026-06-09/agent_14_1078.json`
- `research/degen-virtuals-2026-06-09/agent_15_992.json`
- `research/degen-virtuals-2026-06-09/agent_16_1104.json`
- `research/degen-virtuals-2026-06-09/agent_17_1134.json`
- `research/degen-virtuals-2026-06-09/agent_18_875.json`
- `research/degen-virtuals-2026-06-09/agent_19_395.json`
- `research/degen-virtuals-2026-06-09/agent_20_1044.json`
- `research/degen-virtuals-2026-06-09/chunk_1001-de5795eb8b8cf558.js`
- `research/degen-virtuals-2026-06-09/chunk_1124-6e51d653ac122a2b.js`
- `research/degen-virtuals-2026-06-09/chunk_1255-1e9a537968501932.js`
- `research/degen-virtuals-2026-06-09/chunk_2b873d2b-a99f03d513ac3950.js`
- `research/degen-virtuals-2026-06-09/chunk_4bd1b696-182b6b13bdad92e3.js`
- `research/degen-virtuals-2026-06-09/chunk_5202-9e5a27ce33259fa0.js`
- `research/degen-virtuals-2026-06-09/chunk_5239-148458a7b569bf67.js`
- `research/degen-virtuals-2026-06-09/chunk_6252-a928b513ce479244.js`
- `research/degen-virtuals-2026-06-09/chunk_7447-696b5e8365ad8d6b.js`
- `research/degen-virtuals-2026-06-09/chunk_988-2121b5ab13745173.js`
- `research/degen-virtuals-2026-06-09/chunk_c16f53c3-409df1246fc2628c.js`
- `research/degen-virtuals-2026-06-09/chunk_layout-0344d7f316833d79.js`
- `research/degen-virtuals-2026-06-09/chunk_main-app-f3336e172256d2ab.js`
- `research/degen-virtuals-2026-06-09/chunk_page-f45231ebec550a82.js`
- `research/degen-virtuals-2026-06-09/chunk_polyfills-42372ed130431b0a.js`
- `research/degen-virtuals-2026-06-09/chunk_webpack-552df89fff02adf2.js`
- `research/degen-virtuals-2026-06-09/fetch_degen_top20.py`
- `research/degen-virtuals-2026-06-09/fetch_virtuals_metadata.py`
- `research/degen-virtuals-2026-06-09/generate_wiki_doc.py`
- `research/degen-virtuals-2026-06-09/home.html`
- `research/degen-virtuals-2026-06-09/top20_summary.json`
- `research/degen-virtuals-2026-06-09/top20_summary_enriched.json`
- `research/degen-virtuals-2026-06-09/virtuals_68781.json`
- `research/degen-virtuals-2026-06-09/virtuals_72179.json`
- `research/degen-virtuals-2026-06-09/virtuals_72858.json`
- `research/degen-virtuals-2026-06-09/virtuals_73338.json`
- `research/degen-virtuals-2026-06-09/virtuals_73354.json`
- `research/degen-virtuals-2026-06-09/virtuals_74208.json`
- `research/degen-virtuals-2026-06-09/virtuals_74345.json`
- `research/degen-virtuals-2026-06-09/virtuals_74860.json`
- `research/degen-virtuals-2026-06-09/virtuals_75501.json`
- `research/degen-virtuals-2026-06-09/virtuals_75846.json`
- `research/degen-virtuals-2026-06-09/virtuals_75850.json`
- `research/degen-virtuals-2026-06-09/virtuals_75857.json`
- `research/degen-virtuals-2026-06-09/virtuals_76622.json`
- `research/degen-virtuals-2026-06-09/virtuals_76665.json`
- `research/degen-virtuals-2026-06-09/virtuals_76669.json`
- `research/degen-virtuals-2026-06-09/virtuals_76673.json`
- `research/degen-virtuals-2026-06-09/virtuals_76677.json`
- `research/degen-virtuals-2026-06-09/virtuals_76682.json`
- `research/degen-virtuals-2026-06-09/virtuals_76891.json`
- `research/degen-virtuals-2026-06-09/virtuals_app/app_virtuals_74345.html`
- `research/degen-virtuals-2026-06-09/virtuals_app/index-nWKRiAal.js`
- `research/degen-virtuals-2026-06-09/virtuals_app/main-v4.js`
- `research/degen-virtuals-2026-06-09/virtuals_app/manifest.json`
- `research/kinamix-academy-ai-courses-2026-06-09/adjacent-ai-agent-automation-product/phase_1_results.json`
- `research/kinamix-academy-ai-courses-2026-06-09/adjacent-ai-agent-automation-product/phase_1_synthesis.md`
- `research/kinamix-academy-ai-courses-2026-06-09/adjacent-ai-agent-automation-product/pipeline.log`
- `research/kinamix-academy-ai-courses-2026-06-09/adjacent-ai-agent-automation-product/pipeline_report.json`
- `research/kinamix-academy-ai-courses-2026-06-09/ai-use-courses/phase_1_results.json`
- `research/kinamix-academy-ai-courses-2026-06-09/ai-use-courses/phase_1_synthesis.md`
- `research/kinamix-academy-ai-courses-2026-06-09/ai-use-courses/pipeline.log`
- `research/kinamix-academy-ai-courses-2026-06-09/ai-use-courses/pipeline_report.json`
- `research/kinamix-academy-ai-courses-2026-06-09/openclaw-hermes/phase_1_results.json`
- `research/kinamix-academy-ai-courses-2026-06-09/openclaw-hermes/phase_1_synthesis.md`
- `research/kinamix-academy-ai-courses-2026-06-09/openclaw-hermes/pipeline.log`
- `research/kinamix-academy-ai-courses-2026-06-09/openclaw-hermes/pipeline_report.json`
- `research/kinamix-academy-ai-courses-2026-06-09/run_research.sh`
- `research/kinamix-academy-ai-courses-2026-06-09/vibe-coding-apps/phase_1_results.json`
- `research/kinamix-academy-ai-courses-2026-06-09/vibe-coding-apps/phase_1_synthesis.md`
- `research/kinamix-academy-ai-courses-2026-06-09/vibe-coding-apps/pipeline.log`
- `research/kinamix-academy-ai-courses-2026-06-09/vibe-coding-apps/pipeline_report.json`
- `scripts/run-free-optimizer-24h.sh`
- `strategies/README.md`
- `strategies/orb/README.md`
- `strategies/orb/orb_failed_breakout_reversal.md`
- `strategies/orb/orb_opening_range_breakout.md`
- `strategies/orb/orb_pdh_confirmation.md`

</details>

### olympus-ows-xvision

Mixed Olympus audit/docs archive, scenario clone/UI edits, and scratch strategy experiments.

Recommendation: **keep**. Positive cherry commits: **3**; all unreachable commits: **3**. Review delta: **55 files, +4634/−150 lines**, from baseline [`19a3073d`](https://github.com/latentwill/xvision/commit/19a3073df101ee56469526fdbaa388b6b4019899).

Tests: Edits FiringSection.test.tsx and scenarios-detail.test.tsx; current frontend suite passes. Snapshot not run; split docs, experiments, and code before reuse.

Non-equivalent commits: [`ab0816a6`](https://github.com/latentwill/xvision/commit/ab0816a6f2eb088023d411c2f27f8b62171626d3), [`2becccd7`](https://github.com/latentwill/xvision/commit/2becccd76626622bd6f4c1dd2ddf2776123f1c56), [`0c55a989`](https://github.com/latentwill/xvision/commit/0c55a9894d856ef2ed71d80b4dd7e0558deb0985).

<details>
<summary>Changed files</summary>

- `FOLLOWUPS.md`
- `crates/xvision-dashboard/README.md`
- `crates/xvision-dashboard/src/routes/scenarios.rs`
- `crates/xvision-dashboard/wiki/2026-06-01-xvn-qa-pass-1-dashboard-settings.md`
- `crates/xvision-dashboard/wiki/2026-06-01-xvn-qa-pass-2-live-alpaca-agents.md`
- `crates/xvision-dashboard/wiki/agentic-strategy-tooling.md`
- `crates/xvision-dashboard/wiki/business-and-app-ideas.md`
- `crates/xvision-dashboard/wiki/cli-reference.md`
- `crates/xvision-dashboard/wiki/experiments.md`
- `crates/xvision-dashboard/wiki/index.toml`
- `crates/xvision-dashboard/wiki/mcp.md`
- `crates/xvision-dashboard/wiki/research-index.md`
- `crates/xvision-dashboard/wiki/xvision.md`
- `crates/xvision-dashboard/wiki/xvn-roadmap.md`
- `docs/audits/2026-06-01-xvn-scaper-final-incident-note.md`
- `docs/audits/2026-06-01-xvn-scaper-worklog.txt`
- `docs/superpowers/plans/2026-05-29-autoresearcher-research-synthesis-plan.md`
- `docs/superpowers/plans/2026-05-29-chat-rail-context-switching.md`
- `docs/superpowers/plans/2026-05-29-dspy-autoresearcher-integration.md`
- `docs/superpowers/plans/2026-05-29-plan-1-data-integrations-capabilities.md`
- `docs/superpowers/plans/2026-05-29-plan-2-multi-agent-routing.md`
- `docs/superpowers/plans/2026-05-29-plan-3-loose-framework-bots.md`
- `docs/superpowers/plans/2026-05-29-xvision-local-llama-cpp-ollama-providers.md`
- `docs/superpowers/plans/2026-06-03-001-feat-finance-trading-autoresearch-sweep.md`
- `docs/superpowers/plans/xvn/2026-05-29-xvn-clinesdk-gap-analysis.md`
- `docs/superpowers/plans/xvn/2026-05-29-xvn-plan-gap-answers.md`
- `docs/superpowers/plans/xvn/2026-05-29-xvn-trace-observability-plan.md`
- `docs/superpowers/plans/xvn/README.md`
- `frontend/web/src/api/eval-review.ts`
- `frontend/web/src/api/scenarios.ts`
- `frontend/web/src/api/strategies.ts`
- `frontend/web/src/components/agent/AgentForm.tsx`
- `frontend/web/src/components/diagnostics/AgentDiagnosticsView.tsx`
- `frontend/web/src/components/shell/ChatRail.tsx`
- `frontend/web/src/components/strategy/FiringSection.test.tsx`
- `frontend/web/src/routes/scenarios-detail.test.tsx`
- `frontend/web/src/routes/scenarios-detail.tsx`
- `scripts/xvn-remote.py`
- `strategies/README.md`
- `strategies/donchian_exhaustion_short_scalp.md`
- `team/board-v2.md`
- `team/board.md`
- `tmp/btc_active_market_filter.json`
- `tmp/btc_eth_sol_asymmetric_donchian_bollinger.json`
- `tmp/btc_eth_sol_asymmetric_v2.json`
- `tmp/btc_eth_sol_asymmetric_v3_deepseek_v4pro.json`
- `tmp/btc_eth_sol_regime_split_donchian_bollinger.json`
- `tmp/btc_eth_sol_regime_split_v2.json`
- `tmp/btc_only_donchian_bollinger_filter.json`
- `tmp/btc_only_donchian_bollinger_strategy.json`
- `tmp/btc_strict_breakout_retest_gate_filter.json`
- `tmp/multi_asset_donchian_bollinger_filter.json`
- `tmp/multi_asset_donchian_bollinger_strategy.json`
- `tmp/scalp_strategy.json`
- `tmp/scenario_crypto_bull_30d_jan_2025.json`

</details>

## Requested small-fix salvage

Only `dev-xvn-wt-t1` through `t5`, `t7`, and `t8` exist in the fetched snapshot set. There is no `t6` snapshot. Each is an alias of the named development branch in the complete table.

| Work | Original commit | Main evidence | Cherry-pick attempt |
| --- | --- | --- | --- |
| T1 optimizer paths | `b9580627` | `eb82b67d`; current `commands/optimize.rs` path resolver/test | Conflict in `crates/xvision-cli/src/commands/autooptimizer.rs`; aborted. |
| T2 optimizer provider config | `4b286c53` | `93c1725e`; provider/model override, launch gate, seeded config | Conflicts in `autooptimizer.rs` and `commands/example/seed.rs`; aborted. Its T1 dependency is already present in revised form. |
| T3 provider-key fallback | `99917bf1` | `25a958f3`; shared secrets fallback | Conflicts in `autooptimizer.rs`, `crates/xvision-engine/src/api/eval.rs`, and `src/api/settings/providers.rs`; aborted. |
| T4 risk paths | `56642dcb` | `4283f5ab`; individual cherry equivalent | Conflict in removed `crates/xvision-cli/src/commands/risk.rs`; aborted. Already merged, not missing work. |
| T5 CLI aliases | `95ad4f41` | `6222f7d7`; individual cherry equivalent | Conflict in `crates/xvision-cli/src/commands/eval/mod.rs`; aborted. Already merged. |
| T8 help text | `c61b2397` | `9ebcdb39`; individual cherry equivalent | Conflict in `commands/eval/mod.rs`; aborted. Already merged. |
| T7 example trader agent | `542d8548` | `14cd26a3`, then provider-default correction `e664515b` | Conflicts in `commands/example/seed.rs` and `crates/xvision-engine/src/strategies/templates.rs`; aborted. |
| QA Skills columns | `ea88e52a` | Combined QA commit `dd747f33` | Empty cherry-pick; skipped. |
| QA Stage Pattern hint | `a1e585f2` | `dd747f33`; visible hint remains in `MemorySurface.tsx` | Empty cherry-pick; skipped. |
| QA dark-mode form controls | `4caa0b1f` | `dd747f33`; controls subsequently redesigned | Conflict in `frontend/web/src/routes/scenarios-detail.tsx`; aborted as a whole commit. |
| QA marketplace errors | `d3325885` | `dd747f33`; real API errors are surfaced | Empty cherry-pick; skipped. |
| QA historical report | `de0c15e6` | `dd747f33`, then intentional removal `36a316d4` | Applied cleanly, then removed from the local salvage history because it restores an intentionally removed internal document. |

Outcome: **8 conflicting commits skipped, 3 empty commits skipped, 1 historical-document commit excluded after review, 0 code commits retained**. No partial hunks were carried over. No workflow files were changed or pushed, so workflow-write token permission was not needed. The new triage report is explicitly requested and is force-added as the one exception to main’s ignored `docs/` policy.

## Validation

Tests used default Rust features and CPU-only local execution. GPU training, opt-in live-broker tests, server access, and deployment were outside this run. No code/config files differ from pinned main, so the Rust failures are failures of the baseline checkout in this environment.

| Check | Result |
| --- | --- |
| Frontend `pnpm test` | **3,085 passed**, 327 test files. |
| Frontend `pnpm exec tsc -b` | Passed. |
| Frontend `pnpm build` | Passed; generated the dashboard static assets needed by RustEmbed. |
| Agent daemon `pnpm test` | **162 passed**, 25 test files. |
| Agent conductor `npm test` | **66 passed**, 11 test files. |
| Python `python3 -m unittest discover -s scripts -p 'test_*.py'` | **13 passed**. |
| Rust `scripts/cargo test --workspace --locked` | Blocked at dashboard test compilation: `GateRecord` initializer at `crates/xvision-dashboard/src/routes/autooptimizer.rs:1765` lacks `holdout_epsilon`. The initial missing-static-assets error was resolved by building the frontend. |
| Rust workspace excluding dashboard's own tests, with `--no-fail-fast` | **4,847 passed, 67 failed, 46 ignored**, across 359 completed test targets/summaries; exit 101. Dashboard still compiles as a dependency. |
| Archived paper-reset script `bash -n` | Passed syntax check only; script was not executed. |
| Report verification | All 146 refs appear exactly once; class totals and UNIQUE file/line counts checked; `git diff --cached --check` passed. |

Rust commands used a clone-local `CARGO_TARGET_DIR`, four build jobs, and disabled debug symbols to bound disk usage:

```bash
CARGO_TARGET_DIR="$PWD/target" CARGO_BUILD_JOBS=4 \
CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 \
scripts/cargo test --workspace --exclude xvision-dashboard --locked --no-fail-fast
```

The existing regressions for `path_defaults_resolve_under_xvn_home`, `run_cycle_provider_override_and_launchable_gate`, `seed_writes_autooptimizer_config_at_registered_provider`, `first_seed_creates_strategy_with_trader_agent`, and `resolve_key_falls_back_to_secrets_file_when_env_unset` pass. QA-area tests also pass: Skills (5), MemorySurface (7), and FixtureMarketplaceData (22). The current deploy-patch area, `decision-view.test.ts`, passes all 17 tests. These results support retaining main's versions.

Rust failures include obsolete short/default-prompt fixtures, the now-disabled Groq provider, lineage inspection, and CLI/schema contract drift. The following 19 targets failed. The complete workspace is **not green**, and dashboard tests could not be run:

- `-p xvision-cli --test autooptimizer_cli_inspect`
- `-p xvision-cli --test cli_surface`
- `-p xvision-cli --test eval_batch_run`
- `-p xvision-cli --test experiment_run`
- `-p xvision-cli --test model_bakeoff_cli`
- `-p xvision-cli --test strategy_cli`
- `-p xvision-cli --test strategy_dryrun_format`
- `-p xvision-cli --test strategy_import_pine_cli`
- `-p xvision-engine --lib`
- `-p xvision-engine --test api_eval_run`
- `-p xvision-engine --test autooptimizer_parent_policy`
- `-p xvision-engine --test autooptimizer_workspace_check`
- `-p xvision-engine --test eval_launch_gate_guardrails`
- `-p xvision-engine --test eval_model_override`
- `-p xvision-engine --test eval_retry_from_completed`
- `-p xvision-engine --test eval_run_scenario`
- `-p xvision-engine --test intra_bar_maker_taker`
- `-p xvision-engine --test strategy_export_import`
- `-p xvision-mcp --lib`


## Follow-up recommendations

1. Keep the deploy snapshot as provenance. Review its timeline fix separately because the saved patch has an unresolved identifier.
2. Review the wizard-schema cleanup and Olympus clawpatch snapshot in separate changes. Preserve useful tests, update them to current APIs, and validate behavior before porting UI or normalization changes.
3. Keep the paper-reset helper and remaining research/config archives. The helper performs broker actions and is outside this CPU-only salvage task.
4. After review, the 136 MERGED/SUPERSEDED branch names are delete-later candidates. This task deletes none of them.
