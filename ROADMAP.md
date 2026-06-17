# xvision planning hub

Last consolidated: 2026-05-21.

This is the canonical place to look before starting new product work. It
consolidates future-facing items from `FOLLOWUPS.md`, `team/board.md`,
`team/board-v2.md`, current intake docs, docs plans/specs, marketing plans,
and strategy idea docs.

Original planning files are intentionally left in place for source context.
After this hub is reviewed, run the follow-up sweep in the final section to
replace duplicated queues with links back here.

## Current execution

Use this section for near-term board work. Do not create contracts directly
from older plan files unless the item is listed here or promoted through
intake.

| Priority | Item | State | Source |
|---|---|---|---|
| 1 | Lists v1 phase 2a: migrate `/eval-runs` to `<ResponsiveListCard>` + `useListState` + URL state | ready, serial first | `team/board.md`, `team/contracts/list-migrate-eval-runs.md` |
| 2 | Lists v1 phase 2b: migrate `/strategies` | ready, after 2a | `team/board.md`, `team/contracts/list-migrate-strategies.md` |
| 3 | Lists v1 phase 2c: migrate `/scenarios` + `/agents`, then delete transitional `<ListPagination>` JSX primitive | ready, after 2b | `team/board.md`, `team/contracts/list-migrate-decisions-and-tail.md` |
| 4 | V2E eval trace foundation | ready, must land before downstream V2E work | `team/board-v2.md`, `team/contracts/eval-trace-surface-foundation.md` |
| 5 | V2E candle integrity + manifest, cost model, intra-bar fills, lookahead prober, broker-rule findings, net-of-inference metric | ready, mostly after trace foundation | `team/board-v2.md`, `team/intake/2026-05-19-eval-accuracy-and-trace-surface.md` |
| 6 | V2F strategy authoring knowledge surface | intake only, not decomposed | `team/intake/2026-05-20-strategies-folder-and-template-refactor.md` |
| 7 | Capability-first agent model and recorder unification | needs design note before contract | `team/board-v2.md` |
| 8 | User-configurable review-agent profile | needs design note before contract | `team/board-v2.md` |
| 9 | V2 walk-back research | needs competitor/research note before contract | `team/board-v2.md` |
| 10 | Marketplace all-included strategy dependencies | needs design note before V2C marketplace contracts | `team/board-v2.md` |

Status reconciliation: `team/board.md` says V2A driver tour and in-app docs
merged and were archived. `team/board-v2.md` still lists them as active. Treat
V2A as closed unless the conductor reopens a specific V2A follow-up.

## Roadmap by phase

### V2B - Security and operability

| Item | Planned work | Source anchors |
|---|---|---|
| Dashboard/API auth boundary | Auth for mutating dashboard routes, read/write separation, session/token handling, unauthorized states | F35, V2-V4 action plan |
| Remote CLI orphan recovery | Startup sweep for stale CLI jobs, cancellation, stale lock cleanup, output caps, audit trail | F37, remote CLI specs |
| Broker/wallet/testnet kill switch | Paper/testnet labels, notional/order/leverage/loss limits, global pause for broker and chain actions | V2-V4 action plan |
| Secret redaction | Keep provider, broker, wallet, and service tokens out of logs, SSE, run artifacts, exported examples | V2-V4 action plan |
| Audit and observability | Audit log view for auth, jobs, broker, wallet, marketplace, contract writes; health checks for provider/testnet/broker | V2-V4 action plan |

### V2C - Blockchain testnet

| Item | Planned work | Source anchors |
|---|---|---|
| Mantle Sepolia identity/reputation deploy | Deploy or refresh ERC-8004 identity/reputation surfaces; update addresses; smoke register + feedback | SLF2, ADR 0008 |
| Strategy NFT lifecycle | Mint strategy NFTs from stable manifests; persist token mappings; display token, owner, manifest hash, provenance | SLF3 |
| Reputation writes | Post per-cycle performance receipts and read them back in dashboard surfaces | SLF4 |
| Validation receipts | Add signed-oracle held-out backtest receipts and readback | SLF5 |
| Marketplace testnet flows | List, buy, sell, delegate/license strategy NFTs with explicit testnet labels | marketplace spec, blockchain plans |
| Genealogy on chain | Version `program.md`, parent hashes, manifest lineage, reconstructable tree | SLF8 |
| Orderly testnet smoke | Complete credential onboarding and place/cancel test order | F5 |
| Reputation leaderboards | Product layer for testnet strategy rankings, badges, seasonal/capital/risk brackets | F34 |

### V2D - Agent memory

| Item | Planned work | Source anchors |
|---|---|---|
| Cortex memory plan | Write the missing cortex integration plan before contracts | `team/board-v2.md` V2D notes |
| `xvision-memory` crate/sidecar | Rust memory layer, local-only sidecar, storage/retrieval shape | install customizer spec, V2D notes |
| Per-agent memory mode | `MemoryMode { off, global, agent_scoped }`, dispatcher wiring, namespaces | V2D notes |
| Memory selector UI | Agent edit surface for memory mode | V2D notes |
| Eval-review memory surface | Show whether memory was injected and what was used | V2D notes |

### V2E - Eval accuracy and trace surface

| Item | Planned work | Source anchors |
|---|---|---|
| Trace-surface foundation | Schema enrichment, cycle features parquet, determinism receipts, findings backrefs, indexed cycles | item 17, intake |
| Candle integrity + manifest | OHLC sanity, gaps, timestamp monotonicity, duplicate guard, fixture hashes, feed/calendar/timezone manifests | items 18, 21 |
| Per-bar costs + volume share | Per-bar fee/slip/spread arrays and quadratic volume-share slippage with caps | items 19, 20 |
| Intra-bar fill ordering | Stop/TP/limit branch logic, `FillBranch`, minimal `OrderState`, maker/taker fee classification | item 24 |
| Lookahead-bias prober | Two-pass sliced replay diff and findings | item 22 |
| Broker-rule findings | Crypto-first rule table and rejected-order findings | item 23 |
| Net-of-inference metric | Gross vs inference cost vs net return, compare/run UI, cost-dominance finding | item 25 |
| Later eval waves | Paper/live calibration, trust-receipt renderer, equities readiness, partial fills, latency, market impact, anti-overfitting suite | V2E intake "Out of this intake" |

### V2F - Strategy authoring and user knowledge

| Item | Planned work | Source anchors |
|---|---|---|
| Strategies folder | Per-workspace `.xvn/strategies/` with `notes/`, `docs/`, `strategy-files/`, `evals/`, `library/`; read-only agent tools | intake track 1 |
| Pre-populate strategy library | Copy or link 44 strategy idea templates, markdown backlog, and freqtrade playlist | intake track 2 |
| Agent-pipeline template expansion | Add 4-8 agent composition templates | intake track 3 |
| Strategy ideas tool | `list_strategy_ideas(filter)` for wizard/runtime agents | intake track 4 |
| Wizard prompt refresh | Teach wizard to consult strategies folder and keep templates optional | intake track 5 |
| User import flow | `xvn strategies import` plus dashboard drop-zone and minimal PDF/CSV summaries | intake track 6 |
| Later strategy-folder work | Agent-writable folder, quality scoring, marketplace starter-template listings, eval-result imports | V2F intake "Out of this intake" |

### V3 - Autoresearcher and final UX

| Item | Planned work | Source anchors |
|---|---|---|
| Evening Karpathy loop | Mutate strategy programs, paper-test held-out windows, accept/reject, mint/receipt accepted variants | SLF9 |
| Autoresearcher dashboard | Candidate queue, lineage tree, judge rationale, eval deltas, accepted/rejected mutations, rollback | AR plans |
| Cross-pollination | Agents read top-K reputation and program diffs before proposing mutations | SLF13 |
| Scenario replay modes | Stepped, accelerated, realtime replay | F31 |
| Chart and UI polish | Lightweight chart aesthetics/customization, final dashboard pass, possible Advanced Charts upgrade | F32, F33 |
| Agent social feed | Identity-signed agent commentary tied to actual activity | F29 |
| Pluggable trader stage | `TraderBackend` trait as a first-class strategy implementation shape | SLF15 |

### V4 - Mainnet readiness

| Item | Planned work | Source anchors |
|---|---|---|
| Contract audit | External audit, launch flags, monitored deployment, rollback/runbook | ADR 0008, smart contract specs |
| Mainnet controls | Disable mainnet until V2 security, V2C testnet, and V3 UX/autoresearcher gates pass | V2-V4 action plan |

## Legacy follow-up IDs still carrying future work

These IDs remain useful as search handles and rationale anchors. If a row is
already covered by the phase tables above, schedule from the phase table, not
from `FOLLOWUPS.md` directly.

| ID | Status in this hub | Canonical placement |
|---|---|---|
| SLF2-SLF5 | future, blockchain testnet | V2C |
| SLF6-SLF7 | future seed strategy population | V2F/V3 strategy population |
| SLF8 | future genealogy substrate | V2C |
| SLF9 | future autoresearcher loop | V3 |
| SLF10-SLF11 | future dashboard/delegation UX | V2C/V3 |
| SLF13-SLF16 | future narrative/product/demo work | V3/marketing |
| F5 | future testnet smoke | V2C |
| F6-F8 | defensive harness hygiene | V2E or maintenance intake |
| F18 | multi-asset decision schema | V2F/V2C prerequisite when multi-asset resumes |
| F19-F20 | Orderly SDK/upstream cleanup | maintenance |
| F21 | ACPX/open agent backend open items | V2B/V3 |
| F22 | enum hygiene | maintenance |
| F24 | DeepSeek intern route notes | agent/provider docs |
| F25 | xvision Claude Code skill | V2A follow-up only if reopened |
| F26 | GitHub Actions Node 20 deprecation | maintenance, time-sensitive |
| F27-F28 | install customizer and plugin architecture | V3 or post-V3 platform work |
| F29 | agent social feed | V3 optional product layer |
| F30 | custom scenario eval | mostly shipped; remaining PR/review follow-up only |
| F31 | replay modes | V3 |
| F32 | Lightweight Charts | mostly shipped; remaining QA/customization goes V3 |
| F33 | Advanced Charts | post-F32 optional |
| F34 | reputation leaderboards | V2C/V3 |
| F35 | dashboard API auth | V2B |
| F36 | guided tours | closed unless V2A reopened |
| F37 | remote CLI orphan recovery | V2B |
| F38-F40 | QA6/QA8 reliability wrappers | verify before scheduling; many subitems appear closed in board archives |

Closed or obsolete IDs are not scheduled here: SLF1, SLF12, CVF/F1-F3,
F9-F17, F23, F26-F28 CV-only references that moved to `xvision-play`, and
other archived board contracts.

## Product ideas and non-engineering queues

| Area | Future items | Source |
|---|---|---|
| Marketing demos | Live overnight run, build-from-scratch strategy, risk-gate veto walkthrough, lineage tree explorer | `marketing/demos/README.md` |
| Remotion templates | `run-summary.tsx` next; compare-runs, strategy-lineage, live-build-demo, verdict-trace deferred | `marketing/templates/README.md` |
| Voices | `narrator-neutral` next; founder clone and narrator-warm deferred | `marketing/voices/README.md` |
| Public copy | Capture and verify X reference before using in README, hackathon one-pager, website, pitch deck | `docs/marketing-followups.md` |
| Strategy ideas | 44 JSON strategy templates plus freqtrade playlist become seed library for V2F | `docs/strategies/`, `strategies/` |
| Deferred route/archetype designs | Live cockpit, marketplace, journal, autoresearch routes, Pass-Ribbon, Canvas, Power Notebook, Spreadsheet, Slot Machine | `v1-shipping-plan.md`, `docs/design/gptprompts-v1.md`, `docs/superpowers/plans/2026-05-10-deferred-archetypes-roadmap.md` |
| Mobile later work | Multi-user/social, offline-first, deeper mobile inspection and PWA/Web Push only after auth model is real | `frontend/MOBILE.md` |

## Source map

Active ordering and status:

- `team/board.md` - current execution board.
- `team/board-v2.md` - V2/V3/V4 phase board; contains some stale V2A status.
- `team/contracts/*.md` - executable work contracts.
- `team/intake/*.md` - decomposition inputs before contracts exist.

Roadmap and follow-up rationale:

- `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` - phase roadmap.
- `FOLLOWUPS.md` - legacy F/SLF source anchors.
- `v1-shipping-plan.md` - v1 cut and deferred scope.
- `implementation-plan.md` and `v1-build-steps.md` - older phase plan history.

Design/product sources:

- `frontend/DESIGN.md`, `frontend/MOBILE.md`.
- `docs/design/ui-elements.md`, `docs/design/gptprompts-v1.md`,
  `docs/design/gptprompts.md`, `docs/design/themes*.md`.
- `docs/superpowers/specs/*.md`, especially standard list component,
  agent run observability, trading charts, marketplace, wallets, and
  autoresearcher specs.

Marketing and ideas:

- `marketing/**/README.md`, `docs/marketing-followups.md`.
- `docs/strategies/`, `strategies/`, `docs/tests/`.

## Follow-up sweep after this hub is accepted

1. Add a short pointer at the top of `FOLLOWUPS.md`, `team/board-v2.md`,
   `v1-shipping-plan.md`, and `implementation-plan.md` saying future scheduling
   starts in `ROADMAP.md`.
2. Reconcile stale V2A state in `team/board-v2.md`.
3. Move closed F/SLF entries from `FOLLOWUPS.md` into an archive section or
   replace them with links to this hub.
4. Convert duplicated "not yet decomposed" tables in `team/board-v2.md` into
   links to this hub plus the specific intake docs.
5. Add a lightweight lint/check that flags new `TODO`, `follow-up`,
   `deferred`, or `planned` sections in docs unless they link to this hub or an
   intake/contract.
6. Re-run the source sweep with:
   `rg -n "TODO|follow[- ]?up|future|planned|deferred|roadmap|idea|not yet decomposed" --glob '*.md' --glob '!team/archive/**' --glob '!.worktrees/**'`
   and either add genuinely new items here or mark them historical.
