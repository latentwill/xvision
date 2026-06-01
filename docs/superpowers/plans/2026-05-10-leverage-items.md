# Leverage Items Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Source:** Cross-run synthesis in [`docs/superpowers/research/2026-05-10-ideonomy-explorations.md`](../research/2026-05-10-ideonomy-explorations.md) Theme J ("Several 1-day investments offer disproportionate leverage") + Triage discussion 2026-05-10.
> **Independent of:** wallet plan amendments and terminology rename — each item below is self-contained. Items E (eod) and G (runtime agent rename) touch the wallet plan's data model and should run AFTER the wallet plan amendments doc lands. Items A/B/C/D/F are pure docs/marketing and can run any time.

---

**Goal:** Ship five high-leverage items the research surfaced as having outsized return-per-day-of-work: a hackathon narrative one-pager, a first-user-conversion README rewrite, a scale-tier MANUAL.md addendum, a scheduled end-of-day operator report, and a runtime agent-rename feature. None individually are large; together they reposition the project externally and reduce daily operator load.

**Architecture:** Five independent items grouped by surface (docs A-D and F; CLI+data E; CLI+data+UI G). Items A-D and F are docs-only and ship as PRs to the repo's markdown surfaces. Item E adds a `xvn eod` CLI command that emits a deterministic markdown report from the audit log + ledger; once Plan 2c (durable scheduler) lands, the same command is registered as a scheduled job. Item G adds a `display_name` column to the `strategies` table (created in wallet plan Group B.3), a CLI subcommand to set it, and renders it in the dashboard.

**Tech Stack:** No new deps for items A-D, F. Item E uses existing `chrono`, `sqlx`, `serde_json`. Item G uses existing `clap`, `sqlx`. Plan 2c integration for E is a single registration call against Plan 2c's scheduler API once it lands.

**Out of scope (intentionally not in this plan):**
- The 1-pager's actual marketing copy is the operator's call. This plan ships the *file* with a written-by-Claude first draft as a starting point; the operator iterates.
- Public disclosure SLA — replaced by README warning per Triage 2026-05-10. The README rewrite (Item B) carries the warning.
- OFAC screening — explicitly NOT in this plan; tracked in FOLLOWUPS.md (wallet amendments Group G.4).
- README marketing screenshots / videos — out of scope; if needed, capture separately.

**v1 test cut (eval + strategy engines on Alpaca paper):**

The v1 test slice is "author a strategy → backtest it → paper-trade it on Alpaca." Scheduler (Plan 2c), blockchain/8004 (Plan 5 + wallet plan), and autooptimizer (AR-1/2/3) are all out of that slice. That has knock-on effects for items in *this* plan:

| Item | Lands in v1 test? | What ships / what waits |
|---|---|---|
| A — Hackathon 1-pager | ✅ ship | Pure docs. |
| B — README rewrite | ✅ ship | Pure docs. Warning copy aligns with the v1 test surface (no "live daemon" or "chain attestation" claims). |
| C — MANUAL scale-tier | ✅ ship | Pure docs. |
| D — Incident response | ✅ ship | Pure docs. The `xvn kill / unhalt / emergency-close / reconcile` references assume the live daemon — leave them in (the runbook is forward-looking) but note in the section header that v1 test mode is single-shot eval, not a long-running deployment. |
| E — `xvn eod` report | 🟡 partial | **Task E.1 ships** (the CLI command + tests, against existing audit log + ledger + global_state tables). **Task E.2 waits** — scheduler registration depends on Plan 2c. Operators run `xvn eod` manually after each test session; MANUAL Item C already documents the manual cadence. |
| F — README warning | ✅ ship | Folded into Item B. |
| G — Runtime agent rename | ❌ skip | Both the migration (G.1) and module API (G.2) target the `strategies` table created in **wallet plan Group B.3**, which is part of Plan 5 (blockchain) — out of v1 test scope. The CLI verb (G.3) and budget surfacing (G.4) inherit the same dep. Pick this up alongside the wallet plan. |

**Recommended v1-test execution order from this plan:** A → B → C → D → E.1. Items E.2 and G fall out automatically when their upstream plans land.

---

## File structure

```
docs/
├── HACKATHON-1-PAGER.md                          # NEW (Item A) — narrative for judges/sponsors
└── superpowers/
    ├── plans/2026-05-10-leverage-items.md        # this file

README.md                                          # MODIFY (Item B) — first-user conversion + use-at-own-risk warning
MANUAL.md                                          # MODIFY (Item C) — append "Scale tiers" section (N=10/100/1000)

crates/
├── xvision-cli/src/commands/
│   ├── eod.rs                                    # NEW (Item E) — xvn eod report subcommand
│   └── agent.rs                                  # NEW (Item G) — xvn agent rename / show / list (only the rename arm new; show/list may already exist as renamed strategy command)
│
├── xvision-data/src/migrations/
│   └── 20260510000020_strategies_display_name.sql  # NEW (Item G) — adds display_name column
│
└── xvision-data/src/strategies.rs                # MODIFY (Item G) — add set_display_name / get_display_name
```

---

## Item A — Hackathon 1-pager narrative (1 day)

**Why:** Run 12 of the research identified the 1-pager as touching every external audience (judges, sponsors, prospective users). A clear opening sentence is leverage; absence of one is a missed conversion.

### Task A.1: Draft `docs/HACKATHON-1-PAGER.md`

**Files:**
- Create: `docs/HACKATHON-1-PAGER.md`

- [ ] **Step 1: Write the 1-pager skeleton with placeholder sections**

Create `docs/HACKATHON-1-PAGER.md`:

```markdown
# xvision — Non-custodial AI trading agents that improve themselves

**For:** Mantle hackathon judges + sponsors + first-100 users.
**Status:** Draft. The operator iterates this file directly.

---

## The single-sentence pitch

Unlike FTX, Binance, or any custodial trading platform, **xvision never holds
your trading capital — only the authority to trade with it.** You fund your own
Orderly account, xvision signs orders with a scoped key it can't withdraw with,
and every decision is on-chain attestable. An overnight autooptimizer
generates new strategy variants, evaluates them against a held-out judge, and
seals the survivors as immutable lineage NFTs.

## What's running

- **Trading rail:** Orderly Network on Mantle. Non-custodial — your USDC stays
  in your account, xvision holds a trading-only Ed25519 key with explicit scope
  enforcement at the broker layer.
- **AutoOptimizer:** mutates a seed strategy across the configuration manifold
  (briefing format, prompt scaffolding, model selection, risk envelope), runs
  each variant against a backtest harness, gates survivors through an LLM judge,
  and seals only the variants that beat their parent on out-of-sample data.
- **Provenance:** every variant has a lineage NFT (ERC-8004) recording its
  parent, its mutations, and its sealed performance. Reputation is portable
  across platforms.
- **Operator surface:** kill switch, emergency-close, per-agent budget caps,
  audit log of every order's full lifecycle (emit → risk → simulate → sign →
  submit → fill → close).

## What's load-bearing in the demo

1. **Live autooptimizer run** — show one mutator iteration: variant in,
   judge verdict out, lineage NFT minted.
2. **Kill switch** — `xvn kill --all` halts every dispatcher in <1s.
3. **Audit log replay** — recover position state from the audit log alone.
4. **Marketplace browser** — show how a depositor would discover and delegate
   to a sealed agent. (Marketplace contract is Plan 5; demo uses staged data.)

## Why now

Three converging things make this the right week:
- The wallet rail (this plan) lands the trust story.
- ERC-8004 with reputation portability is a real economic primitive that the
  marketplace turns on.
- LLM judge quality has crossed the threshold where automated evaluation of
  trading-strategy edge is more reliable than human review.

## Risks we're explicit about

- **Single-process key custody.** v1 uses an encrypted-at-rest Ed25519 key in
  the operator's process. v2 paths: MPC, smart-account scoping, browser-issued
  ephemeral keys.
- **Cross-margin contagion.** v1 ships either margin-mode isolation (if Orderly
  supports it for our setup) or an aggregate-margin guard that fails closed.
  Not the only safety mechanism.
- **Reputation gaming.** Attestations are gated to operator + judges in v1.
  Open governance is a v2 problem.

## What we're asking from sponsors

(Operator fills in: judge time / Orderly testnet credits / Mantle priority
support / etc.)

## Where to read more

- Architecture: `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`
- Implementation: `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md`
  + amendments
- Research lineage: `docs/superpowers/research/2026-05-10-ideonomy-explorations.md`
```

- [ ] **Step 2: Commit**

```bash
git add docs/HACKATHON-1-PAGER.md
git commit -m "docs: hackathon 1-pager — non-custodial trust + autooptimizer narrative"
```

---

## Item B — README rewrite for first-user conversion (1 day)

**Why:** Run 12 + Theme A — the existing README is for engineers exploring the codebase, not for first users deciding whether to fund an Orderly account. Run 11 also flagged "docs scale faster than user count" and "what does N=1 use look like" — the README is where N=1 reads.

### Task B.1: Rewrite README.md

**Files:**
- Modify: `README.md` (root)

- [ ] **Step 1: Read the current README**

Run: `cat README.md | head -50` to remember structure. Preserve any sections that have unique value (license, contributing, sponsor logos if present).

- [ ] **Step 2: Replace the content with**

Replace the README.md content with:

```markdown
# xvision

**Non-custodial AI trading agents.** xvision runs LLM-driven trading strategies
against your own broker account, with explicit scope enforcement so xvision
itself never holds your funds. An overnight autooptimizer mutates and
evaluates new strategy variants automatically.

> ⚠️ **This is alpha software. Use at your own risk.** xvision executes real
> trades against real money on whatever broker account you connect. The
> non-custodial design means xvision can't drain your account, but a buggy
> strategy or risk-engine misconfiguration absolutely can lose money. Read the
> safety section below before connecting a non-trivial balance.

## What it does

- Runs trading strategies as LLM-driven decision pipelines (briefing → trader →
  risk gate → execution).
- Holds an Orderly trading-only Ed25519 key per user that can place orders but
  cannot withdraw, transfer, or mint.
- Enforces per-strategy hard-cap × dynamic-quota budgets via a race-free
  reservation pattern; no strategy can exceed its cap even under burst load.
- Logs every order's full lifecycle (emit → risk → simulate → sign → submit →
  fill → close) to an append-only audit log; positions can be reconstructed
  from the log alone.
- Runs an overnight autooptimizer that mutates seed strategies, evaluates
  variants on held-out backtests, and seals survivors as immutable lineage
  artifacts.

## What it does NOT do

- Custody trading capital. You fund your own Orderly account; xvision only
  holds the authority to place trades against it.
- Process withdrawals or transfers. The Orderly trading key is scoped to
  trading only; the broker layer enforces this independently.
- Run unsupervised on production capital without operator oversight. The
  current design assumes a single operator monitoring the system.

## Quickstart (for first users)

This walks through running xvision against Orderly testnet with no real money.

```bash
# 1. Clone and build
git clone https://github.com/your-org/xvision
cd xvision
cargo build --release

# 2. Generate an EVM signing key (or use an existing one)
# 3. Set up Orderly testnet account with that key
# 4. Initialize xvision
export CREDENTIAL_SECRET=$(openssl rand -hex 32)
./target/release/xvn setup
# follow prompts to register Orderly account on testnet

# 5. Issue a trading-only key
./target/release/xvn key issue --user op
# Verify: ./target/release/xvn key verify <pubkey>

# 6. Configure a strategy from a template
./target/release/xvn strategy templates
./target/release/xvn strategy create --from buy_and_hold --agent-id my-first-agent

# 7. Set a budget
./target/release/xvn budget set --agent my-first-agent --hard-cap 100

# 8. Run a single trader cycle and inspect the result
./target/release/xvn run --agent my-first-agent --cycle-id $(uuidgen)
./target/release/xvn audit agent --agent my-first-agent --since 1h
```

## Safety

xvision assumes a single operator who monitors the system and can intervene.
Critical operator commands:

- `xvn kill --strategy <id>` — halt one agent, in-flight positions stay open
- `xvn kill --all` — global halt, every dispatcher refuses new orders
- `xvn unhalt --strategy <id>` — resume after halt
- `xvn emergency-close --all` — flatten every position via market orders
- `xvn audit agent --agent <id> --since 1h` — see every decision in the last hour

The non-custodial design closes one failure mode (xvision can't drain you) but
opens others:
- A buggy strategy can lose its hard-cap allocation. Set caps small at first.
- The autooptimizer can produce a variant that overfits the judge. Lineage
  attestations are explicit about which strategies are sealed (auditable) vs
  which are still mutating (use-with-care).
- Cross-margin contagion: if Orderly applies losses across the whole account,
  one strategy's drawdown can trigger another's stop-loss. v1 either uses
  isolated margin (if available) or fails-closed on aggregate utilization > 85%.

## Architecture

- **Trading rail** (this scope): non-custodial, broker-side scope enforcement,
  off-chain SQLite audit log + reservation ledger.
- **Marketplace rail** (separate scope, Plan 5): on-chain protocol for fees +
  delegation. xvision.io would run this; a self-hosted instance does not need
  it.
- **AutoOptimizer** (separate scope, AR-1/AR-2/AR-3): the mutator + judge +
  lineage seal pipeline.

## Documentation

- `MANUAL.md` — operator runbook (commands, daily checklist, scale tiers)
- `docs/superpowers/specs/` — design specifications
- `docs/superpowers/plans/` — implementation plans (executable)
- `docs/HACKATHON-1-PAGER.md` — narrative pitch

## License

(Operator: confirm license; previous content preserved here.)
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: rewrite README for first-user conversion + alpha-warning"
```

---

## Item C — MANUAL.md scale-tier addendum (1-2 days)

**Why:** Run 11 — three architectural breaks happen at N=10, not N=1000. Operators making capital-allocation decisions need to see "what changes when I'm running 1 vs 10 vs 100 vs 1000 agents" before they're surprised by a break at N=10.

### Task C.1: Append the "Scale tiers" section to MANUAL.md

**Files:**
- Modify: `MANUAL.md` (root) — append section if absent; replace if a placeholder exists.

- [ ] **Step 1: Confirm MANUAL.md exists; if not, create it**

Run: `test -f MANUAL.md && echo exists || echo missing`. If missing, create with a top-level header:

```markdown
# xvision Operator Manual

This is the runbook for operating a xvision instance. For end-user docs, see
`README.md`. For implementation plans, see `docs/superpowers/plans/`.
```

- [ ] **Step 2: Append the scale-tier section**

Append:

```markdown
## Scale tiers

xvision's design assumes a single operator at v1. Several architectural
breakpoints surface at specific user/agent counts; this section documents them
so capital + ops decisions can be planned, not stumbled into.

### N = 1 (single-operator, today)

- **Custody:** single env-var `CREDENTIAL_SECRET` encrypts the operator's
  trading key. Acceptable.
- **Operations:** operator runs `tail -f` on tracing, fires `xvn` commands
  manually. Acceptable; ~2-6 hrs/day.
- **Compliance:** open-source code, self-hosted. No OFAC screening obligation
  on the maintainers (operator's jurisdiction is operator's responsibility).
- **Storage:** single SQLite file. Backups via `sqlite3 .backup` once a day if
  trades > $0.

### N = 10 (multiple users on one operator-managed instance)

Three things break here:

- **Custody:** the env-var-derived single secret encrypts every user's trading
  key. One env var compromise → 10 keys lost. **Migrate to:** per-user HKDF-
  derived key (already implemented in `TradingKeyStore`); rotate the master
  secret quarterly.
- **Operator load:** 6 hrs/day becomes 12. **Migrate to:** scheduled `xvn eod`
  reports + alert routing (Item E of this plan); operator-on-call rotation if
  > 1 person.
- **Compliance:** the moment xvision's marketplace contract takes fees from
  10 distinct EVM addresses, OFAC screening becomes load-bearing for the
  hosting entity (not the open-source code itself). **Migrate to:** OFAC
  screening at the marketplace contract event handler. Tracked in FOLLOWUPS.

### N = 100

- **Storage:** SQLite write throughput hits its ceiling around hundreds of
  concurrent writes/sec. Reservations + audit-log + ledger all serialize. WAL
  mode helps to ~thousands; beyond that, evaluate Postgres.
- **AutoOptimizer cost:** at N=100 with each agent generating 100 mutator
  variants/night × 50K-token briefings × Sonnet-class evaluation, the LLM bill
  is ~$15K/month. **Migrate to:** subscription tier or hosted-runtime line
  (research Theme G).
- **Reputation governance:** when 100+ agents have attestations, the question
  "who can attest?" becomes load-bearing. v1 gates attestations to operator +
  judges. **Migrate to:** explicit governance ladder before this scale.
- **Custody (continued):** at N=100, single-process key custody becomes a real
  concentration risk. **Migrate to:** MPC or smart-account paths (FOLLOWUPS).

### N = 1000

- **Storage:** Postgres mandatory.
- **Operations:** 24/7 on-call. Incident-response runbook required (see
  `## Incident response` below).
- **Distribution:** one operator/instance no longer scales; multi-tenant
  deployment with per-tenant isolation. Effectively a v3 architecture.

### Where the breakpoints come from

- N=1 → N=10 ops break: research Run 8 (operator daily journal — daily review
  becomes full-time at N=10).
- N=10 → N=100 storage + autooptimizer cost: research Run 11 (scaling tree).
- N=100 → N=1000 distribution: research Run 11 + Run 4 (mutation-loop cost).

### Default cadence

- Run `xvn eod` daily (scheduled via Plan 2c when it lands; manual until then).
- Read MANUAL.md once a quarter to confirm the scale tier still matches reality.
- Review FOLLOWUPS.md monthly for items that have become load-bearing.
```

- [ ] **Step 3: Commit**

```bash
git add MANUAL.md
git commit -m "docs(manual): scale-tier addendum (N=1/10/100/1000 breakpoints)"
```

---

## Item D — Incident response template (0.5 day)

**Why:** Run 7 — having the playbook before the incident is the difference between a recoverable event and a public disaster.

### Task D.1: Append "Incident response" section to MANUAL.md

**Files:**
- Modify: `MANUAL.md`

- [ ] **Step 1: Append**

```markdown
## Incident response

Use this checklist when something is wrong or might be wrong. The order is
fixed: contain first, diagnose second, communicate third, post-mortem fourth.

### 1. Contain (≤ 5 min)

- [ ] Run `xvn kill --all` to halt every dispatcher. New orders blocked.
- [ ] Decide whether to also `xvn emergency-close --all`. Defaults: YES if
      "wrong direction" exposure is suspected, NO if you're investigating a
      tooling glitch with no exposure component.
- [ ] Post a one-line status to wherever your status channel is: "Halt at
      <UTC time>; investigating <one-line>." Don't wait for completeness.

### 2. Diagnose (≤ 30 min)

- [ ] Pull the last hour of audit log: `xvn audit agent --since 1h --all`.
- [ ] Cross-check positions: `xvn reconcile --user op --dry-run` — server
      state vs ledger.
- [ ] Identify whether the issue is:
      - **Strategy bug** (specific agent producing wrong decisions)
      - **Risk engine miss** (decision passed risk that shouldn't have)
      - **Execution glitch** (signed payload mismatched, fill mismatched)
      - **Broker outage** (Orderly returned 5xx)
      - **Operator error** (wrong CLI command run)
- [ ] If the issue is constrained to one agent, halt that agent specifically
      and unhalt the others: `xvn kill --strategy <id>; xvn unhalt --all`.

### 3. Communicate (≤ 60 min after detection)

- [ ] Update status channel with what you've found.
- [ ] If user funds are or were at risk, the open-source disclosure SLA is:
      a public summary within 7 days of containment (not 30 — sooner is more
      credible). Post-launch, this can be a `SECURITY.md` policy.

### 4. Post-mortem (within 7 days)

- [ ] Write up: timeline, root cause, what worked, what didn't, what changes.
- [ ] If the post-mortem identifies a missing safety check, add a task to a
      plan that addresses it. Don't leave the gap open.
- [ ] If the post-mortem reveals a policy or runbook gap, update MANUAL.md.
```

- [ ] **Step 2: Commit**

```bash
git add MANUAL.md
git commit -m "docs(manual): incident response checklist"
```

---

## Item E — `xvn eod` end-of-day report (1 day)

**Why:** Run 8 — operator daily review is repetitive and pattern-matchable; a deterministic markdown report saves 30 min/day immediately and gets better as the data model matures. Schedules naturally via Plan 2c.

### Task E.1: Implement `xvn eod` command

**Files:**
- Create: `crates/xvision-cli/src/commands/eod.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs` — `pub mod eod;`
- Modify: `crates/xvision-cli/src/lib.rs` — add `Eod(EodArgs)` to the Command enum and dispatch

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-cli/tests/eod_cli.rs`:

```rust
use std::process::Command;

#[tokio::test]
async fn eod_renders_when_no_activity() {
    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["eod", "--db", ":memory:", "--no-orderly"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("# xvision EOD report"));
    assert!(stdout.contains("No new positions"));
}
```

- [ ] **Step 2: Implement the command**

Create `crates/xvision-cli/src/commands/eod.rs`:

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use clap::Args;
use sqlx::SqlitePool;

#[derive(Args, Debug)]
pub struct EodArgs {
    /// Window length in hours (default 24).
    #[arg(long, default_value_t = 24)]
    pub hours: u64,
    /// Path to xvision DB.
    #[arg(long)]
    pub db: Option<String>,
    /// Skip Orderly state checks (useful in tests + offline runs).
    #[arg(long)]
    pub no_orderly: bool,
}

pub async fn run(args: EodArgs) -> Result<()> {
    let now = Utc::now();
    let since_ms = (now - chrono::Duration::hours(args.hours as i64)).timestamp_millis();

    let db_url = args.db.unwrap_or_else(|| std::env::var("XVN_DB_PATH").unwrap_or_else(|_| ":memory:".into()));
    let pool = SqlitePool::connect(&db_url).await?;

    println!("# xvision EOD report — {}", now.format("%Y-%m-%d %H:%M UTC"));
    println!();
    println!("**Window:** last {} hours (since {}).", args.hours,
        DateTime::from_timestamp_millis(since_ms).unwrap().format("%Y-%m-%d %H:%M UTC"));
    println!();

    // Section: positions opened/closed
    let opened: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM positions WHERE opened_at >= ?",
    ).bind(since_ms).fetch_optional(&pool).await?.unwrap_or(0);
    let closed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM positions WHERE closed_at >= ?",
    ).bind(since_ms).fetch_optional(&pool).await?.unwrap_or(0);
    let realized: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(realized_pnl_usdc), 0) FROM positions WHERE closed_at >= ?",
    ).bind(since_ms).fetch_optional(&pool).await?.unwrap_or(0.0);

    println!("## Positions");
    println!();
    if opened == 0 && closed == 0 {
        println!("No new positions opened or closed in the window.");
    } else {
        println!("- Opened: {}", opened);
        println!("- Closed: {}", closed);
        println!("- Realized PnL (USDC): {:.2}", realized);
    }
    println!();

    // Section: rejections
    let rejects: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM decisions WHERE stage = 'reject' AND occurred_at >= ?",
    ).bind(since_ms).fetch_optional(&pool).await?.unwrap_or(0);

    println!("## Rejections");
    println!();
    if rejects == 0 {
        println!("Zero rejections — clean window.");
    } else {
        println!("{} order(s) rejected in window. Top reasons:", rejects);
        let rows = sqlx::query_as::<_, (String, i64)>(
            "SELECT json_extract(payload_json, '$.reason') AS reason, COUNT(*) FROM decisions \
             WHERE stage = 'reject' AND occurred_at >= ? GROUP BY reason ORDER BY 2 DESC LIMIT 5",
        ).bind(since_ms).fetch_all(&pool).await.unwrap_or_default();
        for (r, c) in rows {
            println!("- {}: {}", r, c);
        }
    }
    println!();

    // Section: halt status
    let halt_ok: bool = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT halted_at FROM global_state WHERE id = 1",
    ).fetch_optional(&pool).await.unwrap_or(None).flatten().is_none();

    println!("## Halt status");
    println!();
    if halt_ok {
        println!("Global halt: NOT set. System is live.");
    } else {
        println!("⚠️ Global halt is SET. Use `xvn unhalt --all` to clear after diagnosis.");
    }
    println!();

    // Section: reservation reaper
    let reaped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pending_reservations WHERE reaped_at >= ?",
    ).bind(since_ms).fetch_optional(&pool).await?.unwrap_or(0);

    println!("## Reservation hygiene");
    println!();
    println!("- Reservations expired by reaper: {}", reaped);
    if reaped > 50 {
        println!("- ⚠️ Reaper rate is high ({} > 50 / window). Investigate dispatcher crashes.", reaped);
    }
    println!();

    // Section: agent activity summary
    println!("## Per-agent activity");
    println!();
    let rows = sqlx::query_as::<_, (String, i64, f64)>(
        "SELECT agent_id, COUNT(*), COALESCE(SUM(realized_pnl_usdc), 0) FROM positions \
         WHERE opened_at >= ? GROUP BY agent_id ORDER BY 3 DESC",
    ).bind(since_ms).fetch_all(&pool).await.unwrap_or_default();
    if rows.is_empty() {
        println!("No agent activity in window.");
    } else {
        println!("| Agent | Positions | Realized PnL (USDC) |");
        println!("|---|---|---|");
        for (agent, n, pnl) in rows {
            println!("| {} | {} | {:.2} |", agent, n, pnl);
        }
    }
    println!();

    if !args.no_orderly {
        // Future: Orderly account snapshot (margin, equity, open orders)
        println!("## Orderly snapshot");
        println!();
        println!("(Snapshot available once `xvn live` is wired; deferred for v1.)");
        println!();
    }

    Ok(())
}
```

- [ ] **Step 3: Wire into CLI**

In `crates/xvision-cli/src/lib.rs`, add to the `Command` enum:

```rust
/// End-of-day operator report.
Eod(commands::eod::EodArgs),
```

And in the dispatch match:

```rust
Command::Eod(args) => commands::eod::run(args).await,
```

In `crates/xvision-cli/src/commands/mod.rs`, add `pub mod eod;`.

- [ ] **Step 4: Run the test and confirm it passes**

Run: `cargo test -p xvision-cli eod`
Expected: pass.

- [ ] **Step 5: Smoke-test against a populated DB**

Run: `cargo run -p xvision-cli -- eod --hours 168 --db /path/to/xvn.db`
Expected: a markdown report with sensible numbers.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(cli): xvn eod end-of-day report (research Run 8)"
```

### Task E.2: Schedule `xvn eod` via Plan 2c (when 2c lands)

**Files:**
- (Conditional) Modify: Plan 2c's scheduler config / job-registration hook.

- [ ] **Step 1: If Plan 2c has shipped, register the EOD job**

Inspect `crates/xvision-engine/src/scheduler` (or wherever Plan 2c installed its scheduler API). Register:

```rust
scheduler.register_job(JobSpec {
    id: "xvn-eod".to_string(),
    cron: "0 17 * * *".to_string(),  // 17:00 every day, in the operator's tz
    handler: Box::new(|_ctx| Box::pin(async {
        xvision_cli::commands::eod::run(EodArgs::default()).await
    })),
}).await?;
```

If Plan 2c hasn't shipped, document the manual cadence in MANUAL.md (Item C already covers this) and skip steps 2-3.

- [ ] **Step 2: Add a job-runs-daily test**

```rust
#[tokio::test]
async fn eod_job_registers_with_scheduler() {
    let s = test_scheduler().await;
    register_eod_job(&s).await.unwrap();
    assert!(s.list_jobs().await.unwrap().iter().any(|j| j.id == "xvn-eod"));
}
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(engine): schedule xvn eod via Plan 2c daily at 17:00"
```

---

## Item F — Public-status README warning (covered by Item B)

The "public disclosure SLA" item from research Theme E is intentionally NOT a binding SLA — instead, the README rewrite (Item B) carries the alpha-warning + use-at-own-risk language. No separate task here.

---

## Item G — Runtime agent rename feature (1 day)

**Why:** User explicitly confirmed 2026-05-10 ("yes, users should be able to rename agents/nft"). The `agent_id` is a stable ULID; the human-readable name is a separate `display_name` that operators can change without breaking attestations/audit-log lineage.

### Task G.1: Migration adds `display_name` to `strategies`

**Files:**
- Create: `crates/xvision-data/src/migrations/20260510000020_strategies_display_name.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Optional human-readable name attached to an agent. The agent_id remains the
-- stable identity; display_name is mutable.

ALTER TABLE strategies ADD COLUMN display_name TEXT;

CREATE INDEX IF NOT EXISTS idx_strategies_display_name ON strategies(display_name);
```

- [ ] **Step 2: Apply and verify**

```bash
sqlite3 /tmp/xvn-rename-test.db < crates/xvision-data/src/migrations/20260510000020_strategies_display_name.sql
sqlite3 /tmp/xvn-rename-test.db ".schema strategies"
```

Expected: schema includes `display_name TEXT`.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-data/src/migrations/20260510000020_strategies_display_name.sql
git commit -m "feat(data): strategies.display_name column (runtime rename support)"
```

### Task G.2: Module API for set/get display name

**Files:**
- Modify: `crates/xvision-data/src/strategies.rs` (created in wallet plan Group B.3)

- [ ] **Step 1: Failing test**

```rust
#[sqlx::test]
async fn display_name_round_trips(pool: SqlitePool) {
    let store = StrategyConfigStore::new(pool);
    store.set("agent-a", &sample_config(), "init").await.unwrap();
    assert_eq!(store.display_name("agent-a").await.unwrap(), None);
    store.set_display_name("agent-a", "BTC Momentum v3", "operator-cli").await.unwrap();
    assert_eq!(store.display_name("agent-a").await.unwrap(), Some("BTC Momentum v3".into()));
    store.set_display_name("agent-a", "BTC Momentum v4", "operator-cli").await.unwrap();
    assert_eq!(store.display_name("agent-a").await.unwrap(), Some("BTC Momentum v4".into()));
}

#[sqlx::test]
async fn set_display_name_for_unknown_agent_errors(pool: SqlitePool) {
    let store = StrategyConfigStore::new(pool);
    let r = store.set_display_name("missing", "X", "cli").await;
    assert!(r.is_err());
}
```

- [ ] **Step 2: Implement**

In `crates/xvision-data/src/strategies.rs`:

```rust
impl StrategyConfigStore {
    pub async fn set_display_name(&self, agent_id: &str, name: &str, by: &str) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        let res = sqlx::query(
            "UPDATE strategies SET display_name = ?, updated_at = ?, updated_by = ? WHERE agent_id = ?",
        ).bind(name).bind(now).bind(by).bind(agent_id).execute(&self.pool).await?;
        if res.rows_affected() == 0 {
            anyhow::bail!("no agent with id '{}'", agent_id);
        }
        Ok(())
    }

    pub async fn display_name(&self, agent_id: &str) -> anyhow::Result<Option<String>> {
        let r: Option<String> = sqlx::query_scalar(
            "SELECT display_name FROM strategies WHERE agent_id = ?",
        ).bind(agent_id).fetch_optional(&self.pool).await?.flatten();
        Ok(r)
    }
}
```

- [ ] **Step 3: Run tests and commit**

```bash
git add -A
git commit -m "feat(data): StrategyConfigStore set/get display_name"
```

### Task G.3: CLI `xvn agent rename`

**Files:**
- Create: `crates/xvision-cli/src/commands/agent.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs`
- Modify: `crates/xvision-cli/src/lib.rs`

The existing `xvn strategy` command operates on `StrategyBundle`s (engine pipeline configs). The new `xvn agent` command operates on the deployed-agent identity (the post-rename `agent_id`). They are intentionally separate verbs.

- [ ] **Step 1: Failing CLI test**

In `crates/xvision-cli/tests/agent_cli.rs`:

```rust
#[tokio::test]
async fn agent_rename_updates_display_name() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("xvn.db");
    setup_test_db_with_agent(&db, "agent-a").await;

    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["agent", "rename", "agent-a", "BTC Momentum"])
        .env("XVN_DB_PATH", &db)
        .env("CREDENTIAL_SECRET", "00".repeat(32))
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let out2 = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["agent", "show", "agent-a"])
        .env("XVN_DB_PATH", &db)
        .env("CREDENTIAL_SECRET", "00".repeat(32))
        .output().unwrap();
    assert!(String::from_utf8_lossy(&out2.stdout).contains("BTC Momentum"));
}
```

- [ ] **Step 2: Implement**

Create `crates/xvision-cli/src/commands/agent.rs`:

```rust
use anyhow::Result;
use clap::{Args, Subcommand};

use crate::context::AppContext;

#[derive(Args, Debug)]
pub struct AgentCmd {
    #[command(subcommand)]
    pub action: AgentAction,
}

#[derive(Subcommand, Debug)]
pub enum AgentAction {
    /// Rename an agent's display_name (id is immutable).
    Rename { agent_id: String, display_name: String },
    /// Show one agent's id, display_name, and current config.
    Show { agent_id: String },
    /// List all agents with their display_names.
    List,
}

pub async fn run(cmd: AgentCmd) -> Result<()> {
    let ctx = AppContext::from_env().await?;
    match cmd.action {
        AgentAction::Rename { agent_id, display_name } => {
            ctx.strategies.set_display_name(&agent_id, &display_name, "operator-cli").await?;
            println!("Renamed {} → {}", agent_id, display_name);
        }
        AgentAction::Show { agent_id } => {
            let cfg = ctx.strategies.get(&agent_id).await?;
            let name = ctx.strategies.display_name(&agent_id).await?.unwrap_or_else(|| "(no display name)".into());
            println!("{} — {}", agent_id, name);
            println!("hard_cap_usdc: {}", cfg.hard_cap_usdc_notional);
            // ... print other fields
        }
        AgentAction::List => {
            let agents = ctx.strategies.list().await?;
            for (id, cfg) in agents {
                let name = ctx.strategies.display_name(&id).await?.unwrap_or_default();
                println!("{:<30} {:<40} cap={}", id, name, cfg.hard_cap_usdc_notional);
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Wire into CLI dispatch**

In `crates/xvision-cli/src/commands/mod.rs`, add `pub mod agent;`.

In `crates/xvision-cli/src/lib.rs`, add to `Command`:

```rust
/// Agent identity management (rename, show, list).
Agent(commands::agent::AgentCmd),
```

And dispatch:

```rust
Command::Agent(cmd) => commands::agent::run(cmd).await,
```

- [ ] **Step 4: Run tests and commit**

```bash
git add -A
git commit -m "feat(cli): xvn agent rename/show/list (runtime display_name)"
```

### Task G.4: Surface `display_name` in `xvn budget` output + dashboard

**Files:**
- Modify: `crates/xvision-cli/src/commands/budget.rs` (planned in original wallet plan Task 4.8)
- Modify: `crates/xvision-budget-ui/src/templates.rs` (created by wallet plan Group G.5)

- [ ] **Step 1: `xvn budget show` includes display_name**

In `budget show` output, prepend the display_name (or `(unnamed)`) before the agent_id:

```rust
let display = ctx.strategies.display_name(&args.agent).await?.unwrap_or_else(|| "(unnamed)".into());
println!("=== {} ({}) ===", display, args.agent);
```

- [ ] **Step 2: Dashboard template**

In `xvision-budget-ui`'s `BudgetRow`, populate `display_name`. The template renders the display_name as the row's primary label, with the agent_id as a sub-label or tooltip.

- [ ] **Step 3: Smoke-test**

Run: `cargo run -p xvision-cli -- budget show --agent agent-a`
Expected: output starts with `=== BTC Momentum (agent-a) ===` (or `(unnamed)` if not yet renamed).

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(cli,ui): surface agent display_name in budget show + dashboard"
```

---

## Self-review

**Spec coverage** — five named items from the triage discussion + research Theme J:

| Item | Source | Status |
|---|---|---|
| 1-pager hackathon narrative | Run 12; Theme A,H,I | Item A |
| README rewrite | Run 12; Theme A | Item B |
| MANUAL.md scale-tier | Run 11 | Item C |
| Incident response template | Run 7; Theme E | Item D |
| `xvn eod` scheduled report | Run 8; Theme E | Item E |
| Runtime agent rename | Triage 2026-05-10 explicit request | Item G |

Public disclosure SLA was discarded (per Triage); replaced by README warning in Item B.
OFAC was discarded for self-hosted scope (Triage); not in this plan.

**Placeholder scan** — Item A's "Operator iterates this file directly" is intentional (the 1-pager is operator copy that ships as a draft); not a TBD in the implementation sense. Item E.2's "if Plan 2c has shipped, register the EOD job" is conditional, not placeholder — it has a clear else branch (manual cadence already documented in Item C). All other steps have concrete code or commands.

**Type/name consistency** — uses `agent_id` (post-rename), `cycle_id` (post-rename), `display_name` (new column added in Item G), `StrategyConfigStore` (created in wallet plan Group B.3), `xvision-budget-ui` (renamed in wallet plan Group G.5).

**Dependencies between items:**
- A, B, C, D, F: pure docs, no code dependency, can run any order.
- E: depends on existing audit log + ledger + global_state tables (wallet plan Group B.2 + original Phase 1).
- G: depends on `strategies` table (wallet plan Group B.3) + Phase 8 standalone UI (wallet plan Group G.5).

**Order of execution recommended:** A → B → C → D → (after wallet amendments land) E → G.
