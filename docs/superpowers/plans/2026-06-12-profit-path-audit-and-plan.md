# Profit Path — Audit, Leverage Ranking & Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan work-unit-by-work-unit.
> Each work unit (WU) below is a contract: exact files, the failing test to write first, the
> verification that proves it, and its dependencies. Expand each WU via the mandatory TDD
> workflow (`superpowers:test-driven-development`); the coverage gate
> (`.coverage-thresholds.json`) is BLOCKING before PR creation. All branch work in
> `.worktrees/<name>` with `CARGO_TARGET_DIR` set; build via `scripts/cargo`.

**Date:** 2026-06-12 · **Method:** 7 parallel read-only audit agents over the full repo +
local SQLite DBs, with all load-bearing findings re-verified by direct read by the lead
architect. No code was modified.

**Plan review gate history (3 adversarial reviewers per iteration, fresh instances):**
iter 1 — Feasibility PASS / Completeness FAIL (3) / Scope PASS; iter 2 — PASS / FAIL (3,
narrower) / PASS; iter 3 — PASS / FAIL (2, factual file-list precision) / PASS. All 8
blocking findings were incorporated; the final two (exhaustive `MetricsSummary` literals;
`tools.rs:2505` is a test fixture) were fixed after the third iteration without a fourth
formal re-review — flagged here per gate escalation protocol. Feasibility re-verified every
load-bearing line anchor as exact across all three iterations.

**Goal:** Make XVN users able to build genuinely profitable, scalable trading strategies —
real compounding profit across the full path: blank → composed strategy → backtest they can
trust → live trading → scaled.

**Architecture (of this plan):** Two load-bearing fixes — make the eval number *mean* what it
claims (backtest↔live execution parity) and give it an *honesty envelope* (statistical trust
frame) — then make live runs survivable enough to compound, then point the optimizer at the
now-meaningful number. Everything else is deliberately skipped or reduced to targeted footgun
fixes.

**Tech stack:** existing — Rust workspace (tokio/axum/sqlx-SQLite), Vite/React SPA,
`xvision-eval` statistical machinery (bootstrap/gate) lifted onto the engine path. No new
dependencies anticipated beyond what the workspace already has.

---

## Part 1 — Audit

Convention: **(F)** = verified by direct file/DB read (file:line cited), **(J)** = judgment.
Claims marked ✓lead were independently re-verified by the lead architect, not just a subagent.

### How this audit relates to 2026-06-10

The hackathon audit (`2026-06-10-turing-hackathon-audit-and-task-graph.md`) was demo-spine
scoped. This audit is profit-path scoped and post-dates ~hundreds of commits (v0.21→v0.36).
Notable drift since: F5 (live risk-veto gap) is **fixed** — daily-loss-kill +
max-concurrent-positions now ported to the live path (`backtest.rs:3576–3642`) (F); F10
(stop atomicity) is **largely fixed** — cancel now flattens before terminating
(`backtest.rs:3041–3076`) (F); the 20% position cap was removed in `1a5c8e58` (consistent on
both paths) (F).

### Area 1 — Eval engine accuracy & capability

**Yardstick:** the headline backtest number is a causally-clean, cost-realistic estimate of
live performance on the same data; backtest and live numbers come from the same machine; the
number carries an honest statistical envelope; ideally, backtest↔live tracking error is
measured.

**Current state (strong core, untrusted edges):**
- Fill simulation is genuinely good: T+1-open fills, linear/volume-share slippage,
  half-spread, maker/taker fees, latency interpolation, partial fills
  (`crates/xvision-engine/src/eval/executor/traits.rs:395–572`); borrow cost on shorts
  (`backtest.rs:2372–2387`); per-bar/per-asset fee overrides (`backtest.rs:2180–2233`). (F)
- Look-ahead is clean on the trader path: decide at T, fill at T+1 open
  (`backtest.rs:1083–1087`); history slice strictly pre-T (`backtest.rs:1103–1105`); SL/TP
  detected on bar T but filled at T+1 open, not at the stop price (`sltp.rs:231–251`,
  `backtest.rs:4940–4956`). (F)
- `net_return_pct` subtracts LLM inference cost from gross (`eval/metrics.rs:114–123`) —
  honest and rare. (F)
- **The statistical machinery exists but is bolted to the wrong harness**: bootstrap Δ-Sharpe
  CI (`xvision-eval/src/bootstrap.rs`), anti-overfit gate (`xvision-eval/src/gate.rs:57–101`),
  look-ahead prober (`prober/lookahead.rs`) are reachable only via `xvn metrics`/`xvn gate`
  on the hardcoded baseline `Algorithm`s — never on the engine path that produces the number
  users see (engine imports xvision-eval only for baselines, `backtest.rs:32`). (F)

**How measured today: it isn't.** No walk-forward, no out-of-sample split, no
backtest-vs-live tracking error anywhere in `crates/` (F). No CI, no overfit flag, no
minimum-N guard on the product eval path (F).

**Single biggest weakness:** backtest and live are not the same simulation — see Area 3; the
divergences mean the backtest number is computed by a different machine than the one that
trades live, and nothing measures the gap.

**Secondary (cited):** annualization assumes 365×24h for all assets — equities Sharpe
mis-scaled (`eval/metrics.rs:96–102`; `xvision-eval` hardcodes 8760) (F); early-stop
inherited flat bars stay in the equity curve feeding Sharpe/drawdown
(`backtest.rs:1446–1485`) (F); perp funding never modeled — `funding_rate_8h` always `None`
in eval (`xvision-core/src/market.rs:68`); the 2026-06-09 funding-rate plan is docs-only (F);
regime labels derived from the whole window (`eval/regime.rs:58`) — latent look-ahead **if**
injected into prompts (unconfirmed) (J); determinism receipt hashes inputs but temperature
isn't pinned (J).

**Unknowns:** whether regime label reaches the trader prompt; realistic per-run trade-count
distributions (local DBs nearly empty — see Part 2); whether eval runs at temperature 0.

### Area 2 — Strategy Optimizer effectiveness (incl. DSPy)

**Yardstick:** selection signal from data the mutation never saw; wins must clear noise, not
a point-estimate epsilon; improvement gated against a no-intelligence counterfactual; a
recorded history of accepted improvements surviving fresh data.

**Current state:** holdout *structure* is real — disjoint day/untouched windows, mutator
never sees holdout metrics, gate requires improvement on both
(`autooptimizer/config.rs:232–239`, `gate.rs:135–173`) (F). All four mutation axes
(Prose/Param/Tool/Filter) are mechanically wired with a real constrained-JSON LLM mutator
(`mutator.rs:119–124, 816–977`) (F). But: the gate is a **pure point-estimate epsilon**
(default 0.05) — no variance, no CI, no minimum trade count (`gate.rs:124–206`) (F);
`cycle_loosen` lowers ε after dry spells (`cycle_loosen.rs:30–48`) (F); `edge_over_random`
(a genuine seeded random-trader counterfactual) is computed and stored but explicitly
"informational; never gating" (`cycle.rs:1444–1466`) (F). The DSPy flywheel defaults off
(`config.rs:249`), compiles via LLM self-evaluation (never a backtest), marks patterns
`active` with zero validation (`dspy_flywheel.rs:60–87`), and uses a degenerate constant
embedding `vec![1.0]` (`dspy_flywheel.rs:16–18`) (F). `mutate-once` non-mock hardcodes
sharpes (1.0,1.0,1.0,1.0) → always gate-fails outside `--mock` (`optimize.rs:2013–2027`) (F).

**How measured today / empirics:** local DBs show the optimizer has **never produced a real
candidate**: `lineage_nodes`, `autooptimizer_gate_records`, `optimization_runs` etc. all 0
rows; the only 2 recorded cycles are empty no-ops ("no candidate produced"), and the honesty
check vacuously reports `passed=1` for them (`cycle.rs:312–353`) (F).

**Single biggest weakness:** the gate cannot distinguish edge from noise (point-estimate ε,
no N floor, counterfactual non-gating) — when it does run, an accepted "improvement" is as
likely overfit as real.

**Unknowns:** whether any non-mock cycle has run on a remote host; realized trade
counts/variance of real cycle backtests.

### Area 3 — Live trading harness

**Yardstick (7 bars):** real venue; backtest parity; survives the night (restart/reconnect/
errors); cheap when idle; scales capital; atomic controls; measured track record. **Clears
0 of 7 fully today; closest on parity-of-vetoes and controls.**

**Current state:** Alpaca paper works end-to-end (`broker_surface.rs:451,490`); Orderly
testnet partial; Alpaca live stubbed; Orderly mainnet + real money hard-blocked in two
places (`live_config.rs:231,349`; `api/eval.rs:3606–3608`) (F). Cost accounting landed well —
per-decision tokens + $ persisted and surfaced in eval detail + `eval watch`
(`eval/cost.rs:109`, migration 018) (F).

**The live decision path executes a different strategy than backtest** ✓lead:
1. **No SL/TP engine live** — zero sltp references in the live region; every
   stop/target/trailing/max-bars the trader emits is silently ignored live; briefing gets
   `bars_held: 0, stop_loss_price: 0.0, take_profit_price: 0.0`
   (`backtest.rs:3504–3507`) (F, ✓lead).
2. **No deterministic filter gate live** — `decide_one_live` calls `run_pipeline`
   unconditionally (`backtest.rs:3510`); a FilterGated strategy that is cheap in backtest
   pays full model cost every bar live (F, ✓lead).
3. **No cadence gate live** — cadence is used only for bar-period/annualization math in the
   live region (`backtest.rs:2999–3000, 3066, 3386`) (F, ✓lead).
4. **Live win_rate is always null** — `let wins = 0u32; let realized_count = 0u32;`
   (immutable) passed to metrics (`backtest.rs:2968–2969`) (F, ✓lead).
5. No borrow cost live; min-notional gate not wired live (test `#[ignore]`d,
   `tests/risk_min_notional.rs:219`) (F).

**Survivability:** daemon restart marks running evals Failed (`server.rs:886`) and abandons
broker positions; relaunch seeds the book flat from `scenario.capital.initial`
(`backtest.rs:2956`) with **no position reconciliation** → silent double-exposure (F).
Recoverable broker errors (429, MarketClosed) kill the run — `is_recoverable()` exists but
is never consulted (`backtest.rs:3282–3293`) (F). Production WS subscription retries with no
backoff (`alpaca_live.rs:266–397`) (F). **SafetyGate is dead code at the submit boundary** —
`check_broker_submit` referenced only by its own tests; `RealBrokerFills::submit` calls the
broker directly (F, ✓lead). No working-order cancellation API on `BrokerSurface` (F).
Sizing compounds intra-run only; no account-equity sync; portfolio/shared-capital modes
`bail!("not yet implemented")` (`backtest.rs:2943–2949`) (F).

**How measured today:** zero live runs in local DBs; no fidelity/uptime/P&L track record (F).

**Single biggest weakness:** unattended restart is unsafe (no reconciliation + crash-marks-
Failed) — a real money-loss path — tied with the parity gap above, which is owned by Area 1/WS1.

### Area 4 — Agent improvements: ClineSDK, Memory

**Yardstick:** an improvement counts only if it is on the decision path by default, changes
what the model sees/does, and its effect is measurable.

**Current state:** Cline runtime is the deploy default (`config/default.toml:14`;
`Dockerfile.deploy:177`) and adds trajectory record/replay, no-decision recovery,
think-block stripping (`execute_cline.rs`) (F). **Memory is structurally unreachable in the
default config**: `execute_slot_cline` has no memory parameters; memory recall/write exists
only on the LlmDispatch path (`execute.rs:321–384, 626–762`), `MemoryMode` defaults Off
everywhere, both `memory.db` files are empty (0 rows) (F). The cortex full-deployment plan
has not merged to main (J/F). Skills are storage without runtime — `skill_ids` exist but no
code injects PromptFragment content into any prompt (F). Critic/Intern capabilities are
stubs returning `"stub critique"` / `"stub intern"` into briefings with no warning
(`dispatch_capability.rs:327–329, 559–581`) (F). SidecarPool crash recovery is test-only
(zero engine call sites) (F). No wall-clock timeout on Cline runs (`DEFAULT_MAX_WALL_MS =
u32::MAX`) (F).

**How measured today:** no A/B of memory-on/off or cline-vs-dispatch exists anywhere;
flywheel health cards measure plumbing inventory, not decision quality (F).

**Single biggest weakness:** memory is fully-built infrastructure that has never influenced
a single real decision — and on the default runtime it *cannot*.

### Area 5 — Agent CLI / operator friction

**Yardstick:** ≤8 commands blank→live-paper with zero silent failures; measured: **14–16
commands, 3 undiscoverable required steps, 12 verified footguns** (F).

**Headline findings:** the known `submit_decision` footgun is fixed on the CLI atomic path
(`strategy.rs:1044–1052` + B23 regression tests) but **regressed onto the MCP path** —
`xvn_strategy_create_atomic` creates the agent with `allowed_tools: Vec::new()`
(`xvision-mcp/src/tools.rs:1348–1365`) (F, ✓lead): every strategy an AI agent composes via
chat-rail/MCP is silently non-functional. Live mode requires a stop flag, undocumented in
MANUAL.md (`eval/mod.rs:768–774` vs `MANUAL.md:344–390`) (F). `--stream-progress` emits only
start/end events and documents events that never fire (`eval/mod.rs:870–893`) (F). Missing
`secrets/providers.toml` is silently ignored (`lib.rs:263–265`) (F).
`ShowDecision`/`ShowBriefing` default to relative `data/store.db`, not `$XVN_HOME`
(`lib.rs:82,89`) (F). Press-audit agent-ergonomics tracks: 0 of 6 shipped (F).

**How measured today:** only the 2026-05-25 press audit; no automated blank→paper-run
regression exists (J).

**Single biggest weakness:** the MCP atomic-create tool-seeding regression (FG-1).

### Area 6 — User-facing UI friction

**Yardstick:** a new user travels the whole path in the SPA without CLI; results carry the
context needed to believe them.

**Headline findings:** the eval result carries **no statistical trust frame** — detail page
shows TOTAL PNL / MAX DRAWDOWN / SHARPE / NET % with no sample size, no CI, no fee/slippage
echo, no synthesized-row disclosure, no overfit/robustness signal (greps on
`eval-runs-detail.tsx` for ci/confidence/slippage/noop_skip/gate all empty) (F); the same
bare numbers also render on `eval-runs-detail-mobile.tsx:352–363` and — highest stakes — on
`live-run-detail.tsx:146` (live P&L headline) (F). Decision count appears in compare but not
the run-detail stat grid (F). A "Run eval →" CTA on the strategy inspector DOES exist
(`authoring.tsx:1731–1736`, linking `/eval-runs?strategy=<id>&start=1`) — an earlier
subagent claim that it was missing was wrong; residual friction is its
disabled/non-launchable state handling (F, corrected by gate review). The StartEvalPanel
scenario dropdown shows only
name + window — no regime/fees context, though scenarios-detail has all of it (F). No
strategy templates at creation (agents have a TemplatePicker; strategies don't); `xvn
example` seeds aren't surfaced; a folder empty-state literally instructs a CLI command (F).
No portfolio/aggregate view across strategies (F). Live launch UI is Alpaca-paper-crypto
only, bounded (bar-limit) not daemon, with that constraint barely explained (F). First-run
tour is a driver.js popup, violating the repo's own no-popups rule (F). Mobile home is a
chat window whose AI lacks eval-history tools (prior-audit F7/F12, open) (F).

**How measured today:** nothing — no analytics/funnel events; one manual design audit
(2026-06-10) (F).

**Single biggest weakness:** the trust frame — four authoritative-looking numbers with zero
context for believing them; a Sharpe 2.3 from 12 decisions looks identical to one from 500.

---

## Part 2 — The eval-trust hypothesis: verdict

**Hypothesis:** "trust in the eval number gates everything downstream."

**Verdict: PARTIALLY CONFIRMED — structurally confirmed, and refined.**

**Confirmed — the number is the universal currency (F):** Sharpe/total-return is the
headline on the eval list (`routes/eval-runs.tsx:336–337`), detail hero
(`eval-runs-detail.tsx:286–301`), compare sort (`eval-compare.tsx:167`), the optimizer's
objective and gate (`autooptimizer/gate.rs:18–39`), the attestation verdict mapping
(`eval/attestation_verdict.rs:134–170`), and marketplace cards (`ListingCard.tsx:83–88`).
Every downstream surface inherits whatever the number is.

**Refined — "trust" decomposes into two independent gaps, both real:**
1. **Statistical honesty gap:** the number ships with no CI, no N guard, mis-annualization
   for equities, synthesized rows in the curve; the validation machinery that exists
   (bootstrap, gate, regime stratification) is orphaned from the product path, and the gate's
   own header says "reportable but not blocking" (`xvision-eval/gate.rs:6–8`) (F).
2. **Execution fidelity gap:** even a perfectly honest backtest number would not predict
   live, because the live path executes a different strategy (no SL/TP, no filter, no
   cadence gate — Area 3) (F, ✓lead).

**The twist (empirical caveat):** the local DBs contain essentially no real data — 1
null-metric fixture eval run, 0 decisions, 0 attestations, 0 optimizer lineage (F). The
attestation loop that would discipline the number against live reality is vacuous today:
`verdict(metrics.sharpe, 0.0)` — listed_sharpe hardcoded to 0.0, so every successful run
trivially "Endorses" (`chain_attestation.rs:71`) (F); marketplace Sharpe is fixture data (F).
Real usage data presumably lives on the Tailscale nodes (e.g. the 21-trade SORB session
recorded in operator memory) — **not inspected here**; claims about actual user behavior are
bounded accordingly.

**Operational conclusion:** the hypothesis is the right organizing principle — but "fix
trust" means fixing *both* gaps. WS1 (fidelity) and WS2 (honesty envelope) are jointly the
spine of this plan; neither alone restores the chain backtest→live→profit.

---

## Part 3 — Leverage ranking & explicit skips

Ranked by effect on "users achieve real compounding profit," not breadth:

1. **WS1 Backtest↔live execution parity** — the product's core promise. Without it, a
   trusted eval number still doesn't transfer to live P&L; with it, every eval improvement
   compounds. Also collapses live idle cost (filter gate port).
2. **WS2 Honesty envelope on the eval number** — lets users (and the optimizer) tell edge
   from noise *before* risking capital; mostly wiring of machinery that already exists.
3. **WS3 Live survivability & compounding** — restart safety, error retry, SafetyGate at the
   boundary, equity-based sizing. Required for "unattended" and "compounding."
4. **WS4 Optimizer statistical floor** — pointed at the now-meaningful number; reuses WS2
   machinery; ends with the first real, evidenced optimizer campaign.
5. **WS5 Authoring footguns** — small fixes with outsized path-reliability gains (the MCP
   seeding regression is critical).

**Skipped entirely, with justification (recommendation: do NOT schedule):**
- **Cortex memory deployment / Cline-runtime unification** — zero evidence of decision-quality
  effect; unmeasurable until WS1/WS2 exist; merging the stale branch is risk without payoff
  now. Exception: WU5.5 stub-honesty warning (tiny).
- **Skills runtime injection** — dormant feature, off the profit path.
- **Marketplace/attestation wiring** — off the profit path; the vacuous
  `verdict(sharpe, 0.0)` is flagged so nobody demos it as real (WU2.6 labels only).
- **Portfolio execution mode backend** — `bail!` today; implementing it is large structural
  work (out of scope per constraints).
- **Real-money unblock / new venues (bybit, Alpaca live)** — deliberately keep blocked until
  WS1–WS3 land and a paper track record exists; unblocking earlier optimizes for a number
  that doesn't transfer.
- **Funding-rate enrichment** — brushes the "no new data sources" constraint; skipped; WU2.5
  adds an honest "funding excluded" disclosure for perp scenarios instead.
- **Mobile home redesign, tour-popup migration, CLI T2–T9 ergonomics, verb aliases,
  remote-CLI polish** — friction, not blockers; none gate the profit path.

---

## Part 4 — Execution plan

### Conflict zones & parallelization (read first)

| Zone | Owner | Notes |
|---|---|---|
| `crates/xvision-engine/src/eval/executor/backtest.rs` **live region (≈ line 2906+, `run_inner_live`/`decide_one_live`)** | **WS1 track — single writer** | WU3.3 (error retry, lives in the live loop) executes inside the WS1 track after WU1.1–1.4 |
| `backtest.rs` **timeline/backtest region** (e.g. WU2.3's early-stop lines 1446–1485) | WS2 engine track has write access to this region | the single-writer rule above covers the live region only; WS2 announces backtest-region edits to the WS1 track before merging |
| `crates/xvision-engine/src/eval/metrics.rs`, `run.rs`, finalize path in `api/eval.rs` | WS2 engine track | coordinate with WS1 only on `compute_run_metrics` signature changes |
| `crates/xvision-execution/**` (incl. `broker_surface.rs`, `orderly.rs`), `crates/xvision-engine/src/eval/executor/real_broker_fills.rs`, `crates/xvision-data/src/alpaca_live.rs`, `crates/xvision-engine/src/safety/**` | WS3 track | disjoint from WS1's loop edits except WU3.3; note crate locations — `safety/gate.rs` is in xvision-engine, `alpaca_live.rs` in xvision-data, `BrokerErrorClass` in xvision-execution/broker_surface.rs |
| `crates/xvision-engine/src/autooptimizer/**` | WS4 track | starts after WU2.1 merges |
| `crates/xvision-mcp/**` | WU5.1 | independent |
| `frontend/web/src/**` | WU2.5/2.6, WU5.2/5.3 — one frontend track | ts-export regen: commit ONLY intended `types.gen` files; revert drift |

WS1, WS2(engine), WS3, WS5 start **in parallel** (disjoint zones). WU2.5 (UI) follows the
WS2 engine fields. WS4 follows WU2.1. Each WU = one worktree branch + PR; run
`bash scripts/board-lint.sh` if you touch `team/` contracts.

**Guardrail compliance, plan-wide:** worktree isolation; `scripts/cargo` wrapper; TDD +
coverage gate; no popups / no right-rail / theme borders for all UI; terminology lock
(`cycle_id`, `Strategy`, `autooptimizer` never bare `optimizer`); any new operator-facing
concept on optimizer surfaces needs a row in
`docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`; schema changes follow
the `cycle-migration` skill (prefer `metrics_json` fields over new columns to avoid
migrations); deploy guardrails untouched.

---

### WS1 — Backtest↔live execution parity (highest leverage)

#### WU1.0 — Live/backtest parity harness (build FIRST; the regression net)
- **Files:** Create `crates/xvision-engine/tests/live_parity.rs`; reuse the engine's existing
  mock/stub dispatch (the canned-echo LLM + `StubPaperTester` patterns from
  `optimize.rs:748–761, 1790–1796` show the seams) and a mock `BrokerSurface`
  (`broker_surface_mock_orders` test shows the pattern).
- **What:** drive the SAME strategy with a deterministic scripted dispatch through (a) the
  backtest composition over injected bars and (b) the live composition fed identical bars via
  a mock stream + mock broker filling at next-open. Assert equality of: decision count,
  decision sequence, fills, equity curve, and `MetricsSummary` fields. Encode today's known
  divergences as an explicit `KNOWN_DIVERGENCES` table of `#[ignore]`d or expected-fail
  cases: filter-gate, sltp, cadence, win_rate, borrow-cost, min-notional. Each later WU flips
  one entry to asserted-parity.
- **Why:** turns "parity" from prose into an executable definition; it is also the demo
  artifact (the divergence list shrinking to zero IS the story).
- **Verify:** `scripts/cargo test -p xvision-engine --test live_parity` green with the
  divergence table matching current reality (i.e. parity asserts pass for what already
  matches: sizing, daily-loss-kill, max-concurrent, pyramid-flip).
- **Depends:** none. **Demonstrable: yes.**

#### WU1.1 — Port the deterministic filter gate to the live loop
- **Files:** Modify `backtest.rs` live region (`run_inner_live`/`decide_one_live`,
  pre-`run_pipeline` at ~3510), mirroring the backtest FilterHook block (~1030–1095),
  including `wake_when_in_position` semantics and `filter_blocked`/provenance events.
- **Why:** fidelity (FilterGated strategies currently behave differently live) AND cost (an
  idle live run pays full model cost every bar — ~$2.70/hr on 1-min opus). This single port
  is most of the live-cost story.
- **Verify (failing test first):** parity-harness case `filter_gated_parity`: same fire-bars
  on both paths AND zero model-dispatch calls on blocked bars (assert via the scripted
  dispatch's call counter). Token accounting asserts blocked bars cost 0.
- **Depends:** WU1.0. **Demonstrable: yes** (token cost per idle hour before/after).

#### WU1.2 — Port the SL/TP engine to the live path
- **Files:** Modify `backtest.rs` live loop: thread `PositionRiskState` + `sltp.rs`
  `check_and_update` over streamed bars; exits submitted through the broker (market close
  per current backtest semantics: detect on bar T, exit at next opportunity — document the
  live analogue precisely); replace the zeroed briefing fields at `backtest.rs:3504–3507`
  with real `bars_held`/`stop_loss_price`/`take_profit_price`.
- **Why:** the largest single fidelity gap — strategies whose edge is exit discipline never
  exit live as backtested; this silently converts winners into unbounded-risk positions.
- **Verify:** parity-harness case `sltp_parity`: stop-hit and target-hit scripts exit on the
  same bars with the same realized P&L on both paths; briefing-context equality assert
  (live seed == backtest seed for identical state).
- **Depends:** WU1.0; same single-writer track as WU1.1. **Demonstrable: yes.**

#### WU1.3 — Cadence gate on live
- **Files:** `backtest.rs` live loop — decide only on `decision_cadence_minutes` boundaries
  (mirror the timeline-loop gate at `backtest.rs:979`); non-cadence bars still update
  marks/SLTP state.
- **Why:** a 1h-cadence strategy on a 1m stream currently decides 60× more often live —
  different strategy, 60× cost.
- **Verify:** parity-harness `cadence_parity`: equal decision counts; unit test: 1h cadence
  on simulated 1m stream → 1 decision/hour, SLTP still checked every bar.
- **Depends:** WU1.0 (+ WU1.2 for the SLTP-still-runs assert).

#### WU1.4 — Live trade accounting parity (win_rate, borrow cost)
- **Files:** `backtest.rs` live region: make `wins`/`realized_count` mutable and count
  realized round-trips exactly as the backtest bookkeeping does; apply `compute_borrow_cost`
  on short holds (or, if deliberately excluded, persist `borrow_cost_excluded: true` into
  `metrics_json` and surface it in WU2.5 — do not leave it silent).
- **Why:** every live run today reports `win_rate = null` and overstates short P&L; users
  cannot compare live to backtest even informally.
- **Verify:** parity-harness `metrics_parity`: win_rate non-null and equal across paths for
  the same script; unit test on the live close path increments wins.
- **Depends:** WU1.0.

#### WU1.5 — Min-notional veto parity
- **Files:** wire the min-notional gate into the live decision path; un-`#[ignore]` the live
  case in `crates/xvision-engine/tests/risk_min_notional.rs:219`.
- **Verify:** that test passes un-ignored.
- **Depends:** WU1.0.

---

### WS2 — Honesty envelope on the product eval number

#### WU2.1 — Statistical envelope computed at run finalize
- **Files:** Modify `crates/xvision-eval/src/bootstrap.rs` (new function — see below),
  `crates/xvision-engine/src/eval/run.rs:347` (`MetricsSummary` lives HERE, not in
  metrics.rs), `crates/xvision-engine/src/eval/metrics.rs` (computation), the finalize path
  in `api/eval.rs`; **plus the three exhaustive `MetricsSummary { … }` literals without
  `..Default::default()` that WILL break compile when fields are added** —
  `crates/xvision-cli/src/commands/optimize.rs:750` (StubPaperTester, production),
  `crates/xvision-mcp/src/tools.rs:3063` (test helper),
  `crates/xvision-dashboard/tests/cli_jobs_eval_run_bridge.rs:56` (test helper) — update
  them (add `..Default::default()`) in the same PR, and scan for any others
  (`rg 'MetricsSummary \{' --type rust`).
- **What — be precise, the existing module is NOT sufficient:**
  `xvision-eval/src/bootstrap.rs` today exposes only `paired_bootstrap_sharpe_delta`
  (two-series paired comparison). This WU adds a **single-series percentile bootstrap**
  alongside it:
  `pub fn bootstrap_metric_ci(returns: &[f64], metric: MetricFn, iters: usize, seed: u64) -> Option<Ci>`
  where `Ci { low: f64, high: f64 }` — resample `returns` with replacement (seeded RNG for
  determinism, matching the paired function's style), recompute the metric per resample,
  take the 2.5/97.5 percentiles; returns `None` for n < 2. Engine depends on
  `xvision-eval` for this (it already imports the crate for baselines, `backtest.rs:32`).
  At finalize, apply it to per-bar returns for Sharpe and total-return →
  `sharpe_ci_low/high`, `return_ci_low/high`, persisted into `metrics_json` along with
  `n_trades`, `n_real_decisions`, `n_synthesized_decisions`, and
  `insufficient_sample: bool` (floor: n_trades < 10 → true; constant named, documented,
  asserted in tests). **Two use cases, two functions:** this single-series CI is the
  product-run envelope; WU4.1's optimizer gate uses the *existing*
  `paired_bootstrap_sharpe_delta` on per-bar deltas — do not conflate them.
  **Serde/back-compat (required):** all new `MetricsSummary` fields are `Option<…>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` (the
  `inference_cost_quote_total` pattern) so old `metrics_json` rows round-trip and importing
  files that construct via `..Default::default()` or read fields stay source-compatible;
  the three exhaustive-literal sites named in **Files** above do need the one-line edit.
  **ts-export ownership (required, this WU):** `MetricsSummary` derives `ts_rs::TS` →
  changing it regenerates `frontend/web/src/api/types.gen/MetricsSummary.ts`. WU2.1 itself
  runs `scripts/cargo test -p xvision-engine --features ts-export` and commits ONLY
  `MetricsSummary.ts` (revert all unrelated `types.gen` drift per the known
  chronically-stale-regen gotcha), so the generated type lands in the same PR as the Rust
  change and the frontend track (WU2.5) builds against it.
- **Why:** the number users act on gets an uncertainty band; the optimizer (WS4) gates on
  the same module's machinery — one home, two consumers.
- **Verify (failing tests first):** unit test in `xvision-eval`: synthetic i.i.d. normal
  returns with known positive mean → CI brackets the analytic Sharpe and `seed` makes it
  reproducible; n=1 → `None`. Engine tests: n=3-trade run → `insufficient_sample=true`;
  zero-trade run → CI fields absent/null, flagged; old `metrics_json` without the new
  fields deserializes.
- **Depends:** none (parallel to WS1; coordinate `compute_run_metrics` signature with WS1
  track). **Demonstrable: yes.**

#### WU2.2 — Annualization by market calendar
- **Files:** `eval/metrics.rs:96–102` (+ `xvision-eval` hardcoded 8760): periods/year derived
  from the scenario's market calendar — crypto 24/7 = 525600/cadence; US equities =
  252×390/cadence-minutes.
- **Why:** equity-strategy Sharpe is currently inflated by a wrong calendar — a silent
  cross-asset comparison bug.
- **Verify:** unit tests per asset class with hand-computed expected factors; existing tests
  updated deliberately (not blindly).
- **Flag:** changes every displayed equities Sharpe — note in PR description and CHANGELOG as
  a number-shift event. **Stale rows:** old `metrics_json` rows are NOT recomputed; persist
  an `annualization_calendar` marker in `metrics_json` going forward, and WU2.5 renders a
  quiet "legacy annualization" note when the marker is absent — disclose, don't rewrite
  history.
- **Depends:** none.

#### WU2.3 — Synthesized-row hygiene
- **Files:** `eval/metrics.rs` + the early-stop path (`backtest.rs:1446–1485`): equity-curve
  metrics computed on the true (non-inherited) curve, or inherited flat bars excluded from
  the Sharpe/drawdown computation; persist synthesized counts (`noop_skip`, early-stop
  inherited, graph-gated) into `metrics_json`.
- **Why:** an early-stopped run currently gets a diluted/distorted curve; users can't see
  how much of "n decisions" was synthetic.
- **Verify:** unit test: run early-stopped at bar k has identical Sharpe to the same run
  truncated at bar k; counts present in `metrics_json`.
- **Depends:** coordinate with WU2.1 (same files — same track).

#### WU2.4 — Evidence grade on the product path (disclose, don't block)
- **Files:** new small module `crates/xvision-engine/src/eval/evidence.rs` + call in
  finalize; extend `xvn eval compare`/`experiment` markdown output.
- **What:** per-run `evidence_grade` (A–D) from: n_trades floor, CI excludes 0, fees model
  present, single- vs multi-regime coverage (when run via batch/experiment), and
  baseline-beat (run beats buy-hold baseline — already computed per run). Philosophy:
  blocking stays where it already is (mint gate); the run path discloses.
- **Why:** one glanceable answer to "should I believe this number?" for users and for the
  chat-rail agent; consumed by WU2.5.
- **Verify:** unit tests over grading matrix; golden-file test for
  `eval compare --markdown` including the grade column.
- **Depends:** WU2.1.

#### WU2.5 — UI trust frame (inline, no popups, no right rail)
- **Files:** `frontend/web/src/routes/eval-runs-detail.tsx` (stat grid + an inline full-width
  evidence strip), `routes/eval-runs-detail-mobile.tsx:352–363` (same stat grid on the phone
  breakpoint), `routes/live-run-detail.tsx:146` (live P&L headline — the highest-stakes
  consumer of the bare number), `routes/eval-runs.tsx` (list columns),
  `routes/eval-compare.tsx`; build the chips/strip as shared components under
  `frontend/web/src/components/` so all five routes render one implementation. Regenerate
  ONLY the intended `types.gen` files. Coordination note: the chat rail's eval rich block
  (`crates/xvision-engine/src/chat_session/rich_blocks.rs`) also surfaces `MetricsSummary` —
  add the envelope fields there in the same PR or file an explicit follow-up; do not leave
  it silently inconsistent.
- **What:** surface — N trades + real-vs-synthesized decisions; CI band rendered with the
  Sharpe/return stats; evidence grade chip; fees/slippage model echo from the scenario (and
  "funding excluded" chip for perp scenarios); regime label; explicit zero-trade/failed
  states (failed runs must not render as 0%-return runs).
- **Why:** Area 6's single biggest weakness; converts WS2's engine work into user-visible
  trust on every surface that shows the number — desktop, mobile, and live.
- **Verify:** vitest component tests for each chip/state on each of the five routes;
  dark-mode border rules; an agent-browser screenshot pass on the live dev dashboard (tall
  viewport per operator memory) appended to `docs/design-audit/`.
- **Depends:** WU2.1–2.4. **Demonstrable: yes** (before/after screenshot).

#### WU2.6 — Downstream consumers honor the envelope (honesty labels)
- **Files:** `features/autooptimizer/selectors/buildHeadline.ts` (ΔSharpe ± CI when
  available); marketplace `ListingCard.tsx` + fixtures (label fixture-sourced Sharpe as
  "sample data"; never render fixture numbers in the same visual register as real ones).
- **Verify:** vitest; grep-level assert that no fixture metric renders unlabeled.
- **Depends:** WU2.1 (optimizer part can land after WS4's gate emits CIs).

---

### WS3 — Live survivability & compounding

#### WU3.1 — Position reconciliation at live launch
- **Files:** `crates/xvision-execution/src/broker_surface.rs` (the `BrokerSurface` trait +
  `MockBrokerSurface`), `alpaca.rs` (paper surface — delegates to the existing
  `AlpacaApi::list_positions`, used internally at `alpaca.rs:566`), `orderly.rs` (map the
  venue positions endpoint), the Alpaca-live stub (returns its usual stub error),
  `bybit.rs:319` (`BybitPaperSurface` — a production impl exported from `lib.rs:19` even
  though the venue is unwired; implement from its internal paper book, do not stub-error),
  noting `AlpacaLiveSurface`'s impl block also lives in `broker_surface.rs` (~:689), not
  `alpaca.rs`,
  the live launch path in `api/eval.rs`, **and every test `impl BrokerSurface` block** —
  `tests/eval_executor_live_loop.rs` alone has ~8; the compiler will enumerate the rest.
- **What:** `list_positions` is NOT on the `BrokerSurface` trait today (only
  `submit_order`/`position`/`balance`/`buying_power`). Add
  `async fn open_positions(&self, assets: &[Asset]) -> Result<Vec<BrokerPosition>, BrokerError>`
  to the trait **deliberately WITHOUT a default implementation** — a silent
  `Ok(vec![])` default would report "flat" and recreate exactly the failure this WU kills;
  forcing every impl (mocks return `Ok(vec![])` explicitly, stubs return their stub error)
  makes the compiler enumerate the full scope. Then, at live launch before the first
  decision: read positions for the run's assets; non-empty → refuse with a typed,
  actionable error (default) or adopt into the book behind an explicit `--adopt-positions`
  flag; seed the book from broker truth, not scenario config. An `Err` from
  `open_positions` (e.g. stub venue) → refuse to launch, stating reconciliation is
  unavailable.
- **Why:** closes the silent double-exposure path (Area 3's biggest weakness).
- **Verify (failing test first):** mock broker holding a position → launch refused with the
  typed error; adopt path seeds book to broker state; clean broker → unchanged behavior;
  `open_positions` error → launch refused with the reconciliation-unavailable error.
- **Depends:** none. **Demonstrable: yes** (with WU3.2 as a chaos demo). Scope note: this WU
  is bigger than it looks (trait change ripples through all impls + test mocks) — budget it
  as M, not S.

#### WU3.2 — Crash-orphan reconciliation report
- **Files:** `xvision-dashboard/src/server.rs:886` (`fail_orphan_runs`) + a small engine
  helper + one CLI/UI surface.
- **What:** when a live run is orphaned by restart, don't just mark Failed: query the broker
  for open positions attributable to the run; emit a supervisor note + an "orphaned
  position" entry with a one-action flatten (CLI verb + inline row in the live console — no
  popup). Full run-resume is explicitly out of scope (structural).
- **Verify:** integration test: simulated restart with open mock-broker position → orphan
  report row exists; flatten action closes it.
- **Depends:** WU3.1 (shares the reconciliation helper). **Demonstrable: yes** — the
  kill-daemon-mid-position rehearsal.

#### WU3.3 — Recoverable-error retry in the live loop
- **Files:** `backtest.rs:3282–3293` region — **executes inside the WS1 track** (single
  writer) after WU1.1–1.4.
- **What:** consult `BrokerErrorClass::is_recoverable()`; bounded retry with exponential
  backoff + per-run retry budget; budget exhaustion or fatal class → terminate as today;
  every retry emits a supervisor note.
- **Verify:** unit test injecting 429 → run survives and recovers; fatal error → terminates;
  budget exhaustion → terminates with note.
- **Depends:** WS1 ports merged (file ownership), WU1.0 harness for regression.

#### WU3.4 — Production WS reconnect backoff
- **Files:** `alpaca_live.rs:266–397` (`run_apca_subscription_task`).
- **What:** apply the exponential backoff that the other reconnect path already implements
  (`alpaca_live.rs:69,546`) so a blip doesn't burn the 5-reconnect budget in milliseconds.
- **Verify:** unit test on retry timing/budget consumption with a flapping mock stream.
- **Depends:** none.

#### WU3.5 — SafetyGate enforced at the submit boundary
- **Files:** `real_broker_fills.rs` (`submit`, line 76) + `safety/gate.rs`.
- **What:** `RealBrokerFills::submit` calls `gate.check_broker_submit` before the broker
  call; blocked → no order, supervisor note, `risk_veto`-style event. This makes the
  venue-label mismatch + notional caps + global pause actually fire where orders leave the
  process.
- **Verify:** extend `tests/safety_gate.rs`: submit through `RealBrokerFills` with a tripped
  gate → broker never called (mock asserts zero submissions), note emitted.
- **Depends:** none. (Coordinate with WS1 only if `FillRequest` plumbing changes.)

#### WU3.6 — Account-equity sizing (compounding)
- **Files:** live launch path + `real_broker_fills.rs:105` sizing input.
- **What:** opt-in `capital_mode = account_equity`: size `risk_pct` off broker account
  equity read at launch (and refreshed on each realized close), instead of static
  `scenario.capital.initial`. Default unchanged.
- **Why:** the goal is literally compounding; today profits never feed position size without
  manual config edits.
- **Verify:** unit test: mock account equity changes → sizing follows; default mode
  byte-identical to today.
- **Depends:** WU3.1 (account read plumbing).

#### WU3.7 — Working-order cancellation on stop/flatten
- **Files:** `BrokerSurface` trait + `alpaca.rs`, `orderly.rs`; flatten/cancel paths in
  `backtest.rs` (WS1-track coordination for the call site).
- **What:** add `cancel_open_orders(asset)` to the `BrokerSurface` trait (same
  no-default-impl discipline and same impl enumeration as WU3.1 — including
  `BybitPaperSurface` and the test mocks); flatten/cancel call it before/with closes —
  matters for Orderly venue-side SL/TP algo orders left resting after a stop.
- **Verify:** mock broker asserts cancels issued on flatten and on cancel; Orderly surface
  unit test maps the venue cancel endpoint.
- **Depends:** **WU3.1** (same trait + same impl files — land after it, same WS3 track);
  call-site timing in `backtest.rs` coordinated with the WS1 track.

---

### WS4 — Optimizer statistical floor (starts after WU2.1)

#### WU4.1 — Gate on uncertainty, not point estimates
- **Files:** `autooptimizer/gate.rs`, `cycle.rs`, `config.rs`; terminology-lock doc row for
  any new operator-facing flag (e.g. `--min-trades`, "confidence floor").
- **What:** a candidate passes only if (a) CI-low of Δobjective > 0 on BOTH the day and
  untouched windows — using the *existing* `paired_bootstrap_sharpe_delta`
  (`xvision-eval/src/bootstrap.rs`) over per-bar return series (this is the paired use case;
  WU2.1's single-series `bootstrap_metric_ci` is the product-run envelope — distinct
  functions, same module), (b) both child runs clear the n_trades floor (new config field
  `min_trades_per_window`, default 10), (c) `edge_over_random` becomes gating by default
  (child CI-low over the seeded random baseline > 0), disabled via new config field
  `edge_gate_enabled = false`, (d) `cycle_loosen` cannot drop `min_improvement` below a hard
  floor constant. Keep the existing regime-matrix and inversion checks. **Terminology lock:**
  both new operator-facing knobs get rows in
  `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md` (developer names
  `min_trades_per_window` / `edge_gate_enabled`; operator names e.g. "minimum trades" /
  "beat-random check") in the same PR.
- **Why:** Area 2's biggest weakness — today the optimizer would bless noise; after this it
  optimizes the same honest number users see.
- **Verify (failing tests first):** synthetic-distribution tests — pure-noise mutations
  accepted at ≤5% rate over 200 simulated gates; injected-true-edge mutations accepted;
  existing canary suite still passes; vacuous honesty-check fixed (no-candidate cycles must
  not report `passed=1` — `cycle.rs:312–353`).
- **Depends:** WU2.1.

#### WU4.2 — Fix `mutate-once` non-mock stub
- **Files:** `optimize.rs:2013–2027`.
- **What:** wire the real `CachedBacktestPaperTester` exactly as `run-cycle` does — today
  the non-mock path hardcodes (1.0,1.0,1.0,1.0) and always gate-fails, silently wasting
  operator time. (Decision made here: wire it, do not remove the verb; fall back to removal
  with a typed error ONLY if execution uncovers a hard blocker, documented in the PR.)
- **Verify:** integration test: non-mock `mutate-once` against cached bars produces real,
  non-hardcoded scores reflected in the gate record.
- **Depends:** none (same crate as WU4.1 — same track).

#### WU4.3 — DSPy pattern activation honesty
- **Files:** `dspy_flywheel.rs:60–87`, `dspy_flywheel.rs:16–18`.
- **What:** compiled patterns persist as `staged`, never auto-`active`; activation requires a
  recorded validation comparison (a backtest of the pattern-bearing strategy vs its
  predecessor on the untouched window, using the WU4.1 gate). Replace the degenerate
  `vec![1.0]` embedding with the workspace's real embedding path, or rename the recall mode
  so it doesn't claim semantics it doesn't have.
- **Why:** today an unvalidated LLM-self-scored prompt can silently change strategies.
- **Verify:** unit test: compile without validation → `staged`; with passing validation →
  `active`; failing → stays staged with the comparison recorded.
- **Depends:** WU4.1.

#### WU4.4 — One real optimizer campaign (acceptance + demo)
- **What:** non-mock `xvn optimize run --strategy <seed>` on a workstation against cached
  bars: ≥3 full cycles with real candidates; gate records, lineage rows, cost rows persisted;
  honesty canary non-vacuous; a written one-page verdict (accepted or correctly-rejected,
  with CI evidence) committed to `docs/superpowers/plans/` as the campaign report.
- **Why:** the optimizer has never produced a real candidate (0 rows everywhere); this is
  the first end-to-end evidence either way — and the demo.
- **Verify:** the DB rows + the report. **Depends:** WU4.1–4.3. **Demonstrable: yes.**

---

### WS5 — Authoring-path footguns (parallel-safe, small)

#### WU5.1 — MCP atomic create seeds `submit_decision` (CRITICAL)
- **Files:** `crates/xvision-mcp/src/tools.rs:1348–1365` (the production
  `xvn_strategy_create_atomic` path); mirror the CLI fix (`strategy.rs:1044–1052`) —
  seeding is **role-conditional** (trader-role slots get `["ohlcv","submit_decision"]`),
  matching the CLI semantics — plus its B23-style regression tests. Note: the other
  `allowed_tools: Vec::new()` at `tools.rs:2505` is inside a test fixture
  (`mcp_flywheel_lineage_returns_optimizer_hash_proof`) — audit it, but change it only if
  it asserts trader behavior; do not blindly apply the fix to test fixtures. Also make
  `xvn_list_templates` return a typed redirect instead of `[]`, and remove the ghost
  `xvn_create_strategy_agent` docstring reference (`tools.rs:135,784`).
- **Why:** every strategy composed by an AI agent via MCP is silently non-functional today —
  the AI-native authoring path regressed the exact footgun the CLI fixed.
- **Verify (failing test first):** MCP integration test: `xvn_strategy_create_atomic` with
  role=trader → agent's `allowed_tools` contains `submit_decision`; strategy passes
  `assert_launchable`; `eval validate` exit 0.
- **Depends:** none. **Demonstrable: yes** (chat-rail compose → validate green).

#### WU5.2 — Strategy-inspector launch CTA: harden, don't build (small)
- **Premise correction (gate review):** a "Run eval →" CTA already exists at
  `authoring.tsx:1731–1736`, linking `/eval-runs?strategy=<id>&start=1` (param shape:
  `strategy=<id>` + boolean `start=1`, per `eval-runs.tsx:111,144`). Do NOT build a new one.
- **Files:** `frontend/web/src/routes/authoring.tsx`.
- **What (residual):** disabled state with a stated reason when the strategy is not
  launchable (diagnostics not green), and test coverage for both states; verify the link
  lands with the strategy preselected and the panel open.
- **Verify:** vitest: CTA enabled+routes for an eval-ready strategy; disabled-with-reason
  for a non-launchable one. If inspection shows both already exist, close this WU as a
  no-op with a one-line note.
- **Depends:** none.

#### WU5.3 — Scenario context in the launch panel
- **Files:** `StartEvalPanel` in `frontend/web/src/routes/eval-runs.tsx`.
- **What:** scenario dropdown rows (or an inline detail line under the select) show regime
  tags, window, and whether a fees/slippage model is configured — the data already exists on
  scenarios (`scenarios.tsx:351–353`, scenarios-detail).
- **Why:** scenario choice determines what the backtest proves; today it's name-only.
- **Verify:** vitest with tagged/untagged scenario fixtures.
- **Depends:** none.

#### WU5.4 — CLI honesty batch
- **Files/what (one PR):**
  1. MANUAL.md live section documents the required stop flags (`eval/mod.rs:768–774`).
  2. Missing `secrets/providers.toml` → startup stderr warning naming the path
     (`lib.rs:263–265`).
  3. `ShowDecision`/`ShowBriefing` default `--db` to `$XVN_HOME/xvn.db` (`lib.rs:82,89`).
  4. `--stream-progress` emits real per-decision progress events (the engine already has
     per-decision persistence/SSE to hook) — or, at minimum, the help text stops promising
     events that never fire (`eval/mod.rs:870–893`). Prefer real events.
  5. Failed runs: `eval show` non-verbose prints `run.error` (`eval/mod.rs:1325–1378`).
- **Verify:** unit tests per item (warning emitted; default path resolution; progress events
  ≥1 per decision in a 3-decision scripted run; error line present).
- **Depends:** none.

#### WU5.5 — Stub-capability honesty
- **Files:** `dispatch_capability.rs` (stubs at 327–329, 559–581) + `strategy diagnostics` /
  `eval validate` warning path.
- **What:** a strategy whose graph includes Critic/Intern capabilities gets a diagnostics +
  validate warning: "capability X is a stub; its output is placeholder text and will degrade
  results." No behavior change to the pipeline.
- **Verify:** unit test: diagnostics JSON contains the warning for a Critic-bearing strategy;
  absent otherwise.
- **Depends:** none.

#### WU5.6 — Strategy templates in the UI (optional, P2)
- **Files:** `frontend/web/src/routes/strategies-new.tsx` + a backend route exposing the
  `xvn example` seed set.
- **What:** a template picker (mirroring the agents `TemplatePicker`) offering the curated
  example strategies at creation; blank remains available.
- **Verify:** vitest; creating from template yields an eval-ready strategy (asserted via the
  readiness check).
- **Depends:** none. Schedule only after WU5.1–5.5.

---

## Part 5 — Sequencing summary

```
parallel from day 0:
  WS1: WU1.0 → WU1.1 → WU1.2 → WU1.3 → WU1.4 → WU1.5 → (WU3.3) → (WU3.7 call sites)
  WS2: WU2.1+WU2.3 → WU2.4 → WU2.5 → WU2.6        (WU2.2 anytime)
  WS3: WU3.1 → WU3.2 → WU3.6   |   WU3.4, WU3.5 anytime
  WS5: WU5.1 (first), WU5.2–5.5 anytime, WU5.6 last
after WU2.1:
  WS4: WU4.1+WU4.2 → WU4.3 → WU4.4
```

**Demonstrable milestones:** (1) parity divergence table shrinking to zero
(WU1.0–1.5); (2) trust-frame before/after screenshot (WU2.5); (3) kill-daemon-mid-position
chaos rehearsal with reconciliation (WU3.1–3.2); (4) first real optimizer campaign report
(WU4.4); (5) chat-rail-composed strategy passing `eval validate` (WU5.1).

**The acceptance test for the whole plan** (run when WS1–WS3 land): one strategy, one
scenario family — backtest (with envelope) vs a multi-day live paper run on the same asset;
publish the first backtest↔live tracking-error number XVN has ever had. That number — not
any single PR — is the metric this plan exists to create and then improve.

## Part 6 — Assumptions & open unknowns (carried into execution, not papered over)

1. **Local DBs ≠ production truth.** All "0 rows" empirics are this workstation. Before
   tuning n-floors (WU2.1/WU4.1 constants), spend one hour querying the Tailscale nodes'
   run/decision distributions via the remote CLI job API. If real runs are plentiful there,
   Part 2's "the number doesn't exist yet" caveat softens; the structural findings stand.
2. **Regime-label leak** (whole-window labels into prompts) — unconfirmed; WU2.5's regime
   chip work should trace the seed builder and either confirm clean or file a follow-up.
3. **Eval temperature/determinism** — not located in this audit; if temperature isn't pinned,
   the same run can produce different numbers; check during WU2.1 and surface in the
   evidence grade if non-deterministic.
4. **Alpaca paper fill realism** is a broker-side unknown; the parity harness deliberately
   uses a mock broker so it tests OUR machine, not Alpaca's.
5. **Orderly perp funding P&L drift** remains a known, now-disclosed (WU2.5) gap; revisit
   the funding-rate plan post-this-wave if perps become the primary live venue.
6. **`feat/live-trading-hackathon` worktree** may contain live wiring ahead of main — diff
   before starting WS3 to avoid re-implementing or colliding.
