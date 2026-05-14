# Xvision V2-V4 Action Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `verification-before-completion` before claiming any roadmap slice is complete. Do not run Cargo, Rust builds, or Rust tests on the deploy host; use static checks locally and GitHub CI for Rust verification.

**Goal:** Turn the current followups, specs, and todo notes into one execution plan for V2 through V4.

**Architecture:** V2 finishes operator usability and hardens the system for testnet execution. The blockchain testnet track proves every marketplace and NFT flow before V3 expands autonomous research. V4 is the audited mainnet contract and marketplace launch.

**Tech Stack:** Rust workspace, `xvision-engine`, `xvision-dashboard`, React/Vite frontend, axum dashboard API, SQLite runtime state, optional `xvision-identity`, ERC-8004 contracts, Mantle Sepolia testnet, Alpaca paper, Orderly testnet.

**Current Status:** V1 docs are aligned with current code. This plan is the active roadmap; older `FOLLOWUPS.md` SLF/F entries are source anchors, not the execution order.

## Phase Gates

| Phase | Theme | Exit Gate |
|---|---|---|
| V2A | Ease of use sweep | A new operator can install, tour, read docs, run examples, and understand results without repo spelunking. |
| V2B | Security hardening | Local, remote, broker, and testnet actions require explicit auth, safe defaults, audit trails, and kill switches. |
| V2C | Blockchain testnet | NFT mint, buy, sell, delegate, marketplace listing, reputation, and validation receipt flows work end-to-end on testnet only. |
| V3 | Autoresearcher and final UI/UX | Strategy mutation, judging, lineage, dashboard review, and rollback are operator-grade and visually final. |
| V4 | Smart contract go-live | Contracts and runbooks are audited, deployed off testnet, monitored, and guarded by mainnet launch controls. |

## V2A - Ease Of Use Sweep

**Priority:** first. This lowers friction before adding higher-risk blockchain and autonomous execution paths.

**Source anchors:**
- `FOLLOWUPS.md`: F36 Driver.js guided tours, F25 xvision Claude Code skill refresh.
- `docs/superpowers/plans/2026-05-10-settings-and-onboarding-plan.md`
- `docs/superpowers/plans/2026-05-10-command-palette-plan.md`
- `docs/superpowers/plans/2026-05-12-agent-access-and-cli-discoverability.md`
- `docs/superpowers/plans/2026-05-12-remote-cli-over-tailscale.md`
- `docs/superpowers/specs/2026-05-12-agent-access-and-cli-discoverability-spec.md`
- `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md`
- `docs/dashboard.md`, `frontend/README.md`, `frontend/DESIGN.md`, `docs/design/*`

**Work packages:**

1. Driver.js guided tours
   - Add tours for first run, strategy authoring, agent creation, scenario creation, eval run review, settings, and live chart views.
   - Persist dismissed/completed tour state under `$XVN_HOME`.
   - Expose "restart tour" from command palette and docs/help.
   - Gate tours so they do not block keyboard workflows or automation.

2. In-app docs page
   - Add a dashboard docs/help route with task-oriented pages: first run, create a strategy, create an agent, run a scenario, review eval results, connect provider keys, use remote CLI.
   - Link docs pages from empty states, command palette, settings, and error views.
   - Keep docs local/offline-first so the deployed dashboard remains useful without external network access.

3. Tutorials and examples
   - Ship example strategies, agents, scenarios, and sample run artifacts that exercise the happy path without live orders.
   - Add tutorials for local backtest, Alpaca paper, remote CLI job, and blockchain testnet preview.
   - Make examples resettable so repeated demos start from a clean state.

4. Discoverability polish
   - Ensure command palette covers all main navigation and creation actions.
   - Normalize empty states across strategies, agents, scenarios, runs, settings, docs, and live views.
   - Refresh the xvision Claude Code skill so agent workers land on current architecture, no-Cargo deploy-host rule, dashboard routes, and roadmap gates.

**Exit checks:**
- Every primary dashboard surface has a tour or contextual docs entry.
- A fresh operator can complete the first-run tutorial with no live credentials.
- Static docs checks pass: `git diff --check` and targeted Markdown link/name checks.
- Frontend verification is run off-host or through approved CI; deploy host does not run Rust builds/tests.

## V2B - Security Hardening

**Priority:** starts after V2A skeleton is usable; can overlap with testnet scaffolding, but must finish before public testnet flows.

**Source anchors:**
- `FOLLOWUPS.md`: F35 dashboard API auth, F37 remote CLI orphan recovery, F21 ACPX backend open items.
- `docs/superpowers/plans/2026-05-12-remote-cli-over-tailscale.md`
- `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md`
- `docs/superpowers/plans/2026-05-10-xvn-scheduling-and-agent-cli*.md`
- `docs/superpowers/specs/2026-05-10-xvn-scheduling-and-agent-cli-design.md`
- `docs/superpowers/plans/2026-05-11-typed-exit-codes.md`

**Work packages:**

1. Dashboard and API auth
   - Add explicit auth for mutating dashboard API routes.
   - Separate read-only status endpoints from write/run/broker/testnet endpoints.
   - Add session expiry, local bind defaults, remote access warnings, and clear unauthorized states.

2. Secret handling
   - Audit provider, broker, wallet, and service tokens.
   - Keep secrets out of logs, SSE events, run artifacts, and exported examples.
   - Add redaction tests for API errors and job logs.

3. Remote CLI job safety
   - Implement orphan recovery, cancellation, and stale lock cleanup.
   - Record user, source, command class, start/end status, and exit code for every remote job.
   - Add max runtime, output size caps, and explicit allowlists for high-risk commands.

4. Broker and wallet guardrails
   - Require testnet/paper labels on non-local execution.
   - Add per-run limits for notional, order count, leverage, and max loss.
   - Add global pause/kill switch for broker and chain actions.

5. Audit and observability
   - Create an audit log view for auth changes, remote jobs, broker actions, wallet actions, marketplace actions, and contract writes.
   - Add health checks for provider credentials, testnet RPC, and broker connectivity.

**Exit checks:**
- No mutating route is callable without an explicit auth path.
- A failed or orphaned remote CLI job is visible, cancellable, and recoverable.
- Secret redaction is covered in tests or CI logs.
- Testnet and paper execution cannot be confused with mainnet/live execution in UI, API, logs, or config.

## V2C - Blockchain Testnet

**Priority:** begins after security basics land; remains testnet-only until V4.

**Source anchors:**
- `FOLLOWUPS.md`: F5 Orderly testnet smoke, SLF2 ERC-8004 testnet deploy, SLF3 mint per-strategy NFT, SLF4 reputation writes, SLF5 validation receipts, SLF8 genealogy on-chain, F34 reputation leaderboards.
- `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`
- `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`
- `docs/superpowers/specs/2026-05-09-marketplace-plugin-design.md`
- `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md`
- `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-amendments.md`
- `docs/erc-8004-agent-uses.md`, `docs/HACKATHON-1-PAGER.md`
- `decisions/0006-non-custodial-wallets.md`, `decisions/0008-erc8004-deployment.md`, `decisions/0010-strategy-loom-pivot.md`

**Work packages:**

1. Testnet identity and wallet foundation
   - Deploy or refresh ERC-8004 identity/reputation surfaces on Mantle Sepolia.
   - Add deterministic strategy and agent identity manifests.
   - Add non-custodial agent wallet flow with explicit operator consent and no mainnet defaults.

2. Strategy NFT lifecycle
   - Mint strategy NFTs from stable manifests.
   - Persist strategy-to-token mappings in local runtime state.
   - Display token, owner, manifest hash, and chain provenance in dashboard.

3. Marketplace testnet flows
   - List a strategy NFT.
   - Buy and sell on testnet.
   - Delegate or license use of a listed strategy.
   - Show marketplace state in dashboard with clear testnet labeling.

4. Reputation and validation receipts
   - Write per-cycle reputation receipts.
   - Write signed validation receipts for held-out backtests.
   - Display receipt links, hashes, and status in run detail, strategy detail, and leaderboard views.

5. Broker and exchange testnet smoke
   - Complete Orderly testnet credential path and smoke trade.
   - Keep Alpaca paper and local backtest as safer defaults.
   - Prove buy/sell/order lifecycle in testnet without crossing into mainnet/live capital.

6. Genealogy and leaderboards
   - Connect strategy lineage to chain-backed identity and receipts.
   - Add reputation leaderboard views for testnet strategies.
   - Keep off-chain local data as the source of detailed eval metrics; chain records store hashes/provenance.

**Exit checks:**
- Mint, list, buy, sell, delegate/license, reputation write, validation write, and receipt readback all pass on testnet.
- Every chain action is tagged testnet in UI/API/logs.
- Mainnet code paths remain disabled by configuration and launch flags.
- Contract addresses, deployment commands, and rollback notes are documented.

## V3 - Autoresearcher And Final UI/UX

**Priority:** after V2 testnet flows are safe and demoable. This phase turns Xvision into a research loop rather than only an operator tool.

**Source anchors:**
- `FOLLOWUPS.md`: SLF9 Karpathy loop, SLF13 cross-pollination, F29 agent social feed, F31 scenario replay modes, F32/F33 charting followups.
- `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md`
- `docs/superpowers/plans/2026-05-09-autoresearcher-1-mutator-lineage-gate-seal.md`
- `docs/superpowers/plans/2026-05-09-autoresearcher-2-cycle-judge-evals.md`
- `docs/superpowers/plans/2026-05-09-autoresearcher-3-dashboard.md`
- `docs/superpowers/research/2026-05-10-ideonomy-explorations.md`
- `docs/superpowers/research/2026-05-11-agents-page-design-space.md`
- `docs/superpowers/specs/2026-05-11-tradingview-charts-design.md`
- `docs/superpowers/plans/2026-05-11-tradingview-charts-*.md`

**Work packages:**

1. Autoresearcher loop
   - Add strategy mutation proposals with lineage metadata.
   - Run candidate strategies through fixed scenario sets and held-out evals.
   - Judge candidates with deterministic gates before any LLM-assisted ranking.
   - Seal accepted mutations with manifest hashes and parent links.

2. Autoresearcher dashboard
   - Show candidate queue, lineage tree, judge rationale, eval deltas, accepted/rejected mutations, and rollback actions.
   - Connect accepted strategies to testnet NFT identity where enabled.
   - Add operator approval gates for any candidate that can reach broker or chain surfaces.

3. Scenario replay and evaluation depth
   - Add replay modes for known market regimes, custom windows, and adverse conditions.
   - Expand per-agent metrics and compare views.
   - Keep run artifacts reproducible enough to re-score candidates later.

4. Final UI/UX pass
   - Resolve visual inconsistencies across strategies, agents, scenarios, evals, live charts, settings, docs, and marketplace views.
   - Finish chart interactions, loading states, error states, responsive behavior, and accessibility pass.
   - Ensure docs, examples, and tours reflect final routes and naming.

5. Optional product layers
   - Agent social feed, if it clarifies research history rather than adding noise.
   - Cross-pollination between strategy families after lineage and rollback are reliable.
   - Plugin architecture only where it removes real coupling in marketplace/research integrations.

**Exit checks:**
- Autoresearcher can propose, evaluate, accept/reject, seal, and display a mutation cycle.
- Every autonomous action has budget, sandbox, and approval limits.
- UI/UX review passes on desktop and mobile viewports.
- Examples, tutorials, and tours remain accurate after V3 changes.

## V4 - Smart Contract Go-Live

**Priority:** final launch phase. No off-testnet launch until V2 security, V2C testnet, and V3 UX/autoresearcher gates are complete.

**Source anchors:**
- `decisions/0008-erc8004-deployment.md`
- `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`
- `docs/superpowers/specs/2026-05-09-marketplace-plugin-design.md`
- `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`
- Testnet deployment notes produced during V2C.

**Work packages:**

1. Contract readiness
   - Freeze contract interfaces.
   - Complete internal review, external audit, and remediation.
   - Document upgradeability, ownership, admin keys, fee controls, and emergency pause behavior.

2. Mainnet launch controls
   - Add explicit mainnet feature flags separate from testnet flags.
   - Require multi-step operator confirmation for off-testnet writes.
   - Add chain-id and address allowlists.
   - Add monitoring for contract events, failed writes, marketplace anomalies, and wallet balances.

3. Migration from testnet
   - Define which testnet identities, strategies, ratings, and receipts are migrated, reissued, or discarded.
   - Publish provenance rules so testnet performance is not misrepresented as live/mainnet proof.
   - Create launch data snapshots and rollback plans.

4. Marketplace launch
   - Enable mainnet mint/list/buy/sell/delegate flows.
   - Publish user-facing risk disclosures and operator docs.
   - Monitor first transactions and lock launch if anomalies appear.

**Exit checks:**
- Audit findings are closed or explicitly accepted.
- Mainnet dry-run checklist is complete.
- Emergency pause and rollback are tested.
- Off-testnet launch is approved with contract addresses, runbooks, monitoring, and public docs in place.

## Execution Board Shape

Use the execution board for implementation-sized tickets, not broad themes. Each ticket should carry:

- `Phase`: V2A, V2B, V2C, V3, or V4.
- `Source`: followup ID, plan/spec doc, ADR, or design doc.
- `Owner surface`: frontend, dashboard API, engine, identity/contracts, docs, CI, ops.
- `Risk`: low, medium, high.
- `Verification`: exact commands or CI checks, with the deploy-host no-Cargo constraint called out when relevant.
- `Done`: observable behavior, not implementation intent.

Suggested first board tickets:

| Order | Ticket | Phase | Effort | Source |
|---|---|---|---|---|
| 1 | Add Driver.js first-run and restart-tour infrastructure | V2A | M | F36 |
| 2 | Add in-app docs/help route and docs index | V2A | M | settings/onboarding, dashboard docs |
| 3 | Create resettable example strategies/scenarios/tutorial artifacts | V2A | M | frontend docs, eval docs |
| 4 | Add dashboard mutating-route auth boundary | V2B | L | F35 |
| 5 | Add remote CLI orphan recovery and audit trail | V2B | M | F37, remote CLI specs |
| 6 | Add broker/wallet/testnet kill switch and limits | V2B | M | security + blockchain plans |
| 7 | Deploy/refactor Mantle Sepolia identity/reputation addresses | V2C | M | SLF2, ADR 0008 |
| 8 | Implement strategy NFT mint/readback flow | V2C | L | SLF3 |
| 9 | Implement testnet marketplace list/buy/sell/delegate flow | V2C | L | marketplace spec |
| 10 | Implement reputation and validation receipt write/readback | V2C | L | SLF4, SLF5 |
| 11 | Build autoresearcher mutation/eval/judge loop | V3 | L | autoresearcher plans |
| 12 | Build autoresearcher dashboard and lineage review | V3 | L | autoresearcher dashboard plan |
| 13 | Run final UI/UX pass across dashboard surfaces | V3 | L | design docs, chart plans |
| 14 | Prepare contract audit, launch flags, and mainnet runbook | V4 | L | ADR 0008, contract specs |

## Source Material Map

| Source | Current disposition |
|---|---|
| `FOLLOWUPS.md` SLF/F queue | Historical anchors. Keep IDs for traceability, execute through this roadmap. |
| `docs/superpowers/plans/2026-05-10-frontend-*.md` | Mostly V1 implementation history; mine for UI conventions and gaps during V2A/V3 polish. |
| `docs/superpowers/plans/2026-05-10-settings-and-onboarding-plan.md` | V2A onboarding and docs route input. |
| `docs/superpowers/plans/2026-05-10-command-palette-plan.md` | V2A discoverability input. |
| `docs/superpowers/plans/2026-05-12-agent-access-and-cli-discoverability.md` | V2A/V2B bridge for CLI discoverability and access controls. |
| `docs/superpowers/plans/2026-05-12-remote-cli-over-tailscale.md` | V2B remote execution security and audit input. |
| `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md` | V2C/V4 contract surface input. |
| `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md` | V2C/V4 wallet input. |
| `docs/superpowers/specs/2026-05-09-marketplace-plugin-design.md` | V2C/V4 marketplace input. |
| `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md` | V3 source of truth for autonomous research loop. |
| `docs/superpowers/plans/2026-05-09-autoresearcher-*.md` | V3 implementation sequence. |
| `docs/design/*` and chart plans/specs | V3 final UI/UX pass input; V2A can use them for docs/tour visuals. |

## Operating Rules

- Keep V2C strictly testnet-only until V4 gates are complete.
- Do not let marketplace or wallet work bypass V2B auth, audit, limits, and kill switches.
- Keep local backtest and Alpaca paper as default safe execution modes.
- Treat chain data as provenance and receipts; keep detailed eval metrics in local/off-chain artifacts.
- Move broad roadmap items into execution-board tickets before implementation.
- Keep docs, tours, examples, and route names updated in the same PR as user-facing changes.
