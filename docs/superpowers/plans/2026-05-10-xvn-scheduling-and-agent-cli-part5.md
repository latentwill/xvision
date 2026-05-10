# xvn Scheduling Plan — Part 5 (Tasks 19–28: CLI completeness + polish)

> Continues `2026-05-10-xvn-scheduling-and-agent-cli-part4.md`. Same goals/architecture/tech stack apply. Final part.

> **Common pattern for Tasks 19–24:** Each is "create a CLI subcommand file that thin-wraps engine API functions, register it in `commands/mod.rs` and the top-level `Command` enum, smoke test, commit." Where the existing `crates/xvision-cli/src/commands/<name>.rs` already exists (e.g., `strategy.rs`, `risk.rs`, `report.rs`), **modify** to add the new subcommands; do not replace existing functionality.

---

### Task 19: `xvn strategy` lifecycle subcommands

**Files:**
- Modify: `crates/xvision-cli/src/commands/strategy.rs`

> **Context.** Existing `strategy.rs` has `new`, `validate`, `ls`, `show`, `templates`, `run`. Add `deactivate`, `reactivate`, `archive`, `unarchive`, `delete`. Each thin-wraps `xvision_engine::api::strategy::*`.

- [ ] **Step 1: Add subcommand variants**

In the existing `strategy.rs` `enum StrategyAction` (or equivalent), add:

```rust
Deactivate { id: String, #[arg(long)] reason: String },
Reactivate { id: String },
Archive    { id: String, #[arg(long)] reason: String },
Unarchive  { id: String },
Delete     { id: String, #[arg(long)] confirm: bool },
```

- [ ] **Step 2: Add ApiContext helper (top of file)**

```rust
async fn ctx() -> anyhow::Result<std::sync::Arc<xvision_engine::api::ApiContext>> {
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = sqlx::SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    Ok(std::sync::Arc::new(xvision_engine::api::ApiContext::new(xvn_home, db)))
}
```

- [ ] **Step 3: Add the new branches to `run()` dispatch**

```rust
StrategyAction::Deactivate { id, reason } => {
    let ctx = ctx().await?;
    xvision_engine::api::strategy::deactivate(&ctx, &id, &reason, xvision_engine::api::Actor::Cli).await?;
    println!("deactivated {id}: {reason}");
}
StrategyAction::Reactivate { id } => {
    let ctx = ctx().await?;
    xvision_engine::api::strategy::reactivate(&ctx, &id, xvision_engine::api::Actor::Cli).await?;
    println!("reactivated {id}");
}
StrategyAction::Archive { id, reason } => {
    let ctx = ctx().await?;
    xvision_engine::api::strategy::archive(&ctx, &id, &reason, xvision_engine::api::Actor::Cli).await?;
    println!("archived {id}: {reason}");
}
StrategyAction::Unarchive { id } => {
    let ctx = ctx().await?;
    xvision_engine::api::strategy::unarchive(&ctx, &id, xvision_engine::api::Actor::Cli).await?;
    println!("unarchived {id}");
}
StrategyAction::Delete { id, confirm } => {
    if !confirm { anyhow::bail!("pass --confirm to delete (this is destructive)"); }
    let ctx = ctx().await?;
    xvision_engine::api::strategy::delete(&ctx, &id, xvision_engine::api::Actor::Cli).await?;
    println!("deleted {id}");
}
```

- [ ] **Step 4: Smoke test**

```bash
export XVN_HOME=/tmp/xvn-strat-smoke
rm -rf $XVN_HOME && mkdir -p $XVN_HOME/strategies/sh_smoke
echo 'name="t"' > $XVN_HOME/strategies/sh_smoke/manifest.toml
# Note: strategy create will only call record_created when an existing
# `xvn strategy new` integration writes the bundle. For the smoke test,
# write a minimal status manually via deactivate (which auto-creates audit).
cargo run -p xvision-cli -- strategy deactivate sh_smoke --reason "test"
cargo run -p xvision-cli -- strategy show sh_smoke
cargo run -p xvision-cli -- strategy reactivate sh_smoke
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/strategy.rs
git commit -m "feat(cli): xvn strategy {deactivate, reactivate, archive, unarchive, delete}"
```

---

### Task 20: `xvn risk` subcommands

**Files:**
- Modify or Create: `crates/xvision-cli/src/commands/risk.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs` (if new)
- Modify: `crates/xvision-cli/src/lib.rs` (if new)

- [ ] **Step 1: Implement**

Replace the contents of `crates/xvision-cli/src/commands/risk.rs` (or create new):

```rust
use clap::{Args, Subcommand};

use xvision_engine::api::{risk, Actor};

#[derive(Args, Debug)]
pub struct RiskCmd {
    #[command(subcommand)]
    pub action: RiskAction,
}

#[derive(Subcommand, Debug)]
pub enum RiskAction {
    Show         { deployment_id: String },
    SetCapital   { deployment_id: String, #[arg(long)] usd: f64, #[arg(long)] reason: String },
    ScaleCapital { deployment_id: String, #[arg(long)] factor: f64, #[arg(long)] reason: String },
    SetStopLoss  { deployment_id: String, #[arg(long)] atr_multiple: f32, #[arg(long)] reason: String },
    SetPositionSize { deployment_id: String, #[arg(long)] pct: f32, #[arg(long)] reason: String },
    SetMaxConcurrent { deployment_id: String, #[arg(long)] n: u32, #[arg(long)] reason: String },
    TripCircuitBreaker { deployment_id: String, #[arg(long)] reason: String },
    ResetCircuitBreaker { deployment_id: String },
}

async fn ctx() -> anyhow::Result<std::sync::Arc<xvision_engine::api::ApiContext>> {
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = sqlx::SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    Ok(std::sync::Arc::new(xvision_engine::api::ApiContext::new(xvn_home, db)))
}

pub async fn run(cmd: RiskCmd) -> anyhow::Result<()> {
    let ctx = ctx().await?;
    match cmd.action {
        RiskAction::Show { deployment_id } => {
            println!("{}", serde_json::to_string_pretty(&risk::get(&ctx, &deployment_id).await?)?);
        }
        RiskAction::SetCapital { deployment_id, usd, reason } => {
            risk::set_capital(&ctx, &deployment_id, usd, &reason, Actor::Cli).await?;
            println!("set_capital {deployment_id} = {usd}");
        }
        RiskAction::ScaleCapital { deployment_id, factor, reason } => {
            risk::scale_capital(&ctx, &deployment_id, factor, &reason, Actor::Cli).await?;
            println!("scale_capital {deployment_id} ×{factor}");
        }
        RiskAction::SetStopLoss { deployment_id, atr_multiple, reason } => {
            risk::set_stop_loss(&ctx, &deployment_id, atr_multiple, &reason, Actor::Cli).await?;
            println!("set_stop_loss {deployment_id} = {atr_multiple}");
        }
        RiskAction::SetPositionSize { deployment_id, pct, reason } => {
            risk::set_position_size_pct(&ctx, &deployment_id, pct, &reason, Actor::Cli).await?;
            println!("set_position_size_pct {deployment_id} = {pct}");
        }
        RiskAction::SetMaxConcurrent { deployment_id, n, reason } => {
            risk::set_max_concurrent_positions(&ctx, &deployment_id, n, &reason, Actor::Cli).await?;
            println!("set_max_concurrent {deployment_id} = {n}");
        }
        RiskAction::TripCircuitBreaker { deployment_id, reason } => {
            risk::trip_circuit_breaker(&ctx, &deployment_id, &reason, Actor::Cli).await?;
            println!("circuit breaker tripped on {deployment_id}");
        }
        RiskAction::ResetCircuitBreaker { deployment_id } => {
            risk::reset_circuit_breaker(&ctx, &deployment_id, Actor::Cli).await?;
            println!("circuit breaker reset on {deployment_id}");
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Wire into top-level CLI** (add `Risk(commands::risk::RiskCmd)` and dispatch — same pattern as Task 16).

- [ ] **Step 3: Smoke test**

```bash
export XVN_HOME=/tmp/xvn-risk-smoke
rm -rf $XVN_HOME
mkdir -p $XVN_HOME/deployments/dep_x
cat > $XVN_HOME/deployments/dep_x/config.json <<'EOF'
{"deployment_id":"dep_x","agent_id":"sh_t","broker":"alpaca_paper","capital_usd":1000,"stop_loss_atr_multiple":1.5,"position_size_pct":0.05,"max_concurrent_positions":3,"circuit_breaker_tripped":false}
EOF
cargo run -p xvision-cli -- risk show dep_x
cargo run -p xvision-cli -- risk set-capital dep_x --usd 500 --reason "halve"
cargo run -p xvision-cli -- risk show dep_x
```

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/risk.rs \
        crates/xvision-cli/src/commands/mod.rs \
        crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn risk subcommands — set-capital, scale-capital, knobs, circuit breaker"
```

---

### Task 21: `xvn deploy` subcommands

**Files:**
- Create: `crates/xvision-cli/src/commands/deploy.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs`
- Modify: `crates/xvision-cli/src/lib.rs`

- [ ] **Step 1: Implement**

Create `crates/xvision-cli/src/commands/deploy.rs`:

```rust
use clap::{Args, Subcommand};

use xvision_engine::api::{deploy, Actor};

#[derive(Args, Debug)]
pub struct DeployCmd {
    #[command(subcommand)]
    pub action: DeployAction,
}

#[derive(Subcommand, Debug)]
pub enum DeployAction {
    Ls,
    Show { deployment_id: String },
    Create {
        #[arg(long)] strategy: String,
        #[arg(long, default_value = "alpaca_paper")] broker: String,
        #[arg(long, default_value_t = 10_000.0)] capital: f64,
        #[arg(long, default_value_t = 1.5)] stop_loss_atr: f32,
        #[arg(long, default_value_t = 0.05)] position_size_pct: f32,
        #[arg(long, default_value_t = 3)] max_concurrent: u32,
    },
    Start { deployment_id: String },
    Stop {
        deployment_id: String,
        #[arg(long, default_value = "graceful")] mode: String,
    },
    Flatten { deployment_id: String },
    Restart { deployment_id: String },
    SwitchMode { deployment_id: String, #[arg(long)] broker: String },
}

async fn ctx() -> anyhow::Result<std::sync::Arc<xvision_engine::api::ApiContext>> {
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = sqlx::SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    Ok(std::sync::Arc::new(xvision_engine::api::ApiContext::new(xvn_home, db)))
}

pub async fn run(cmd: DeployCmd) -> anyhow::Result<()> {
    let ctx = ctx().await?;
    match cmd.action {
        DeployAction::Ls => {
            let l = deploy::list(&ctx, deploy::DepListFilter::default()).await?;
            println!("{:<28} {:<24} {:<14} {:<10} {}", "ID", "STRATEGY", "BROKER", "STATUS", "CAPITAL");
            for d in l {
                println!("{:<28} {:<24} {:<14} {:<10} ${:.2}", d.id, d.agent_id, d.broker, format!("{:?}", d.status), d.capital_usd);
            }
        }
        DeployAction::Show { deployment_id } => {
            println!("{}", serde_json::to_string_pretty(&deploy::show(&ctx, &deployment_id).await?)?);
        }
        DeployAction::Create { strategy, broker, capital, stop_loss_atr, position_size_pct, max_concurrent } => {
            let id = deploy::create(&ctx, deploy::DeploymentConfig {
                deployment_id: None, agent_id: strategy, broker,
                capital_usd: capital, stop_loss_atr_multiple: stop_loss_atr,
                position_size_pct, max_concurrent_positions: max_concurrent,
            }, Actor::Cli).await?;
            println!("created {id}");
        }
        DeployAction::Start { deployment_id } => {
            deploy::start(&ctx, &deployment_id, Actor::Cli).await?;
            println!("started {deployment_id}");
        }
        DeployAction::Stop { deployment_id, mode } => {
            let m = match mode.as_str() {
                "flatten" => deploy::StopMode::Flatten,
                "hard" => deploy::StopMode::Hard,
                _ => deploy::StopMode::Graceful,
            };
            deploy::stop(&ctx, &deployment_id, m, Actor::Cli).await?;
            println!("stopped {deployment_id} ({mode})");
        }
        DeployAction::Flatten { deployment_id } => {
            let r = deploy::flatten(&ctx, &deployment_id, Actor::Cli).await?;
            println!("flatten: {}", serde_json::to_string_pretty(&r)?);
        }
        DeployAction::Restart { deployment_id } => {
            deploy::restart(&ctx, &deployment_id, Actor::Cli).await?;
            println!("restarted {deployment_id}");
        }
        DeployAction::SwitchMode { deployment_id, broker } => {
            deploy::switch_mode(&ctx, &deployment_id, &broker, Actor::Cli).await?;
            println!("{deployment_id} broker -> {broker}");
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Wire into top-level CLI**

In `commands/mod.rs`: `pub mod deploy;`. In `lib.rs`: `Deploy(commands::deploy::DeployCmd)` + dispatch `Command::Deploy(cmd) => commands::deploy::run(cmd).await?`.

- [ ] **Step 3: Smoke test**

```bash
export XVN_HOME=/tmp/xvn-deploy-smoke
rm -rf $XVN_HOME && mkdir -p $XVN_HOME/strategies/sh_t
echo 'name="t"' > $XVN_HOME/strategies/sh_t/manifest.toml
cargo run -p xvision-cli -- deploy create --strategy sh_t --capital 500
cargo run -p xvision-cli -- deploy ls
cargo run -p xvision-cli -- deploy start <id>
```

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/deploy.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn deploy subcommands"
```

---

### Task 22: `xvn report` subcommands (extend with eod, anomaly-scan, etc.)

**Files:**
- Modify: `crates/xvision-cli/src/commands/report.rs`

> **Context.** Existing `report.rs` has the backtest-result → Markdown command. Refactor to a `Subcommand` enum containing `Backtest` (existing logic), `Eod`, `StrategyReview`, `DeploymentHealth`, `AnomalyScan`, `TokenSpend`, `Pnl`.

- [ ] **Step 1: Replace contents of `report.rs`**

```rust
use std::path::PathBuf;

use clap::{Args, Subcommand};

use xvision_engine::api::{report, Actor};

#[derive(Args, Debug)]
pub struct ReportCmd {
    #[command(subcommand)]
    pub action: ReportAction,
}

#[derive(Subcommand, Debug)]
pub enum ReportAction {
    /// Render an existing BacktestResult JSON to Markdown.
    Backtest { input: PathBuf, #[arg(long)] out: Option<PathBuf> },
    /// EOD report for live deployments.
    Eod {
        #[arg(long)] deployment: Vec<String>,
        #[arg(long)] out: Option<PathBuf>,
        #[arg(long, default_value_t = false)] no_markdown: bool,
    },
    StrategyReview { #[arg(long, default_value_t = 30)] window_days: u32 },
    DeploymentHealth { #[arg(long)] id: Option<String> },
    AnomalyScan,
    TokenSpend { #[arg(long, default_value = "month")] window: String },
    Pnl {
        #[arg(long, default_value = "month")] window: String,
        #[arg(long, default_value = "deployment")] group_by: String,
    },
}

async fn ctx() -> anyhow::Result<std::sync::Arc<xvision_engine::api::ApiContext>> {
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = sqlx::SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    Ok(std::sync::Arc::new(xvision_engine::api::ApiContext::new(xvn_home, db)))
}

pub async fn run(cmd: ReportCmd) -> anyhow::Result<()> {
    let ctx = ctx().await?;
    match cmd.action {
        ReportAction::Backtest { input, out } => {
            let md = report::backtest_report(&ctx, &input).await?;
            match out {
                Some(p) => { std::fs::write(&p, md.as_bytes())?; println!("wrote {}", p.display()); }
                None => print!("{md}"),
            }
        }
        ReportAction::Eod { deployment, out, no_markdown } => {
            let r = report::eod(&ctx, report::EodOpts {
                deployments: if deployment.is_empty() { None } else { Some(deployment) },
                baseline_arm: None,
                render_markdown: !no_markdown,
            }).await?;
            match (out, r.markdown) {
                (Some(p), Some(md)) => { std::fs::write(&p, md.as_bytes())?; println!("wrote {}", p.display()); }
                (None, Some(md)) => print!("{md}"),
                (_, None) => println!("{}", serde_json::to_string_pretty(&r)?),
            }
        }
        ReportAction::StrategyReview { window_days } => {
            let r = report::strategy_review(&ctx, report::ReviewOpts { window_days: Some(window_days), include_deactivated: false }).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        ReportAction::DeploymentHealth { id } => {
            let r = report::deployment_health(&ctx, id.as_deref()).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        ReportAction::AnomalyScan => {
            let r = report::anomaly_scan(&ctx).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        ReportAction::TokenSpend { window } => {
            let w = match window.as_str() {
                "day" => report::Window::Day,
                "week" => report::Window::Week,
                _ => report::Window::Month,
            };
            let r = report::token_spend(&ctx, w).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        ReportAction::Pnl { window, group_by } => {
            let w = match window.as_str() { "day" => report::Window::Day, "week" => report::Window::Week, _ => report::Window::Month };
            let g = match group_by.as_str() {
                "strategy" => report::GroupBy::Strategy,
                "asset" => report::GroupBy::Asset,
                "none" => report::GroupBy::None,
                _ => report::GroupBy::Deployment,
            };
            let r = report::pnl_summary(&ctx, w, g).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Update CLI dispatch** to point to the new subcommand-style `ReportCmd` (replace whatever the old `report.rs` exposed).

- [ ] **Step 3: Smoke test**

```bash
export XVN_HOME=/tmp/xvn-report-smoke
rm -rf $XVN_HOME && mkdir -p $XVN_HOME
cargo run -p xvision-cli -- report eod --no-markdown
cargo run -p xvision-cli -- report anomaly-scan
```

Expected: empty/no-deployment EOD report rendered, empty anomaly list.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/report.rs crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn report — eod, strategy-review, deployment-health, anomaly-scan, token-spend, pnl, backtest"
```

---

### Task 23: `xvn maintenance` subcommands

**Files:**
- Create: `crates/xvision-cli/src/commands/maintenance.rs`
- Modify: CLI mod + dispatch.

- [ ] **Step 1: Implement**

Create `crates/xvision-cli/src/commands/maintenance.rs`:

```rust
use clap::{Args, Subcommand};

use xvision_engine::api::{maintenance, Actor};

#[derive(Args, Debug)]
pub struct MaintenanceCmd {
    #[command(subcommand)]
    pub action: MaintenanceAction,
}

#[derive(Subcommand, Debug)]
pub enum MaintenanceAction {
    RotateLogs        { #[arg(long, default_value_t = 30)] retain_days: u32 },
    CompactEvents     { #[arg(long, default_value_t = 90)] retain_days: u32 },
    CompactAudit      { #[arg(long, default_value_t = 180)] retain_days: u32 },
    RefreshEvalCache,
    BackupLineage     { #[arg(long)] dest: std::path::PathBuf },
    VacuumDb,
    IntegrityCheck,
}

async fn ctx() -> anyhow::Result<std::sync::Arc<xvision_engine::api::ApiContext>> {
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = sqlx::SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    Ok(std::sync::Arc::new(xvision_engine::api::ApiContext::new(xvn_home, db)))
}

pub async fn run(cmd: MaintenanceCmd) -> anyhow::Result<()> {
    let ctx = ctx().await?;
    match cmd.action {
        MaintenanceAction::RotateLogs { retain_days } => {
            let r = maintenance::rotate_logs(&ctx, retain_days, Actor::Cli).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        MaintenanceAction::CompactEvents { retain_days } => {
            let r = maintenance::compact_scheduler_events(&ctx, retain_days, Actor::Cli).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        MaintenanceAction::CompactAudit { retain_days } => {
            let r = maintenance::compact_strategy_audit(&ctx, retain_days, Actor::Cli).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        MaintenanceAction::RefreshEvalCache => {
            let r = maintenance::refresh_eval_cache(&ctx, Actor::Cli).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        MaintenanceAction::BackupLineage { dest } => {
            let r = maintenance::backup_lineage(&ctx, &dest, Actor::Cli).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        MaintenanceAction::VacuumDb => {
            maintenance::vacuum_db(&ctx, Actor::Cli).await?;
            println!("vacuumed");
        }
        MaintenanceAction::IntegrityCheck => {
            let r = maintenance::integrity_check(&ctx, Actor::Cli).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Wire into CLI** (mod.rs + lib.rs dispatch).

- [ ] **Step 3: Smoke**

```bash
export XVN_HOME=/tmp/xvn-mntn-smoke
rm -rf $XVN_HOME
cargo run -p xvision-cli -- maintenance integrity-check
cargo run -p xvision-cli -- maintenance vacuum-db
```

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/maintenance.rs \
        crates/xvision-cli/src/commands/mod.rs \
        crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn maintenance subcommands"
```

---

### Task 24: `xvn autoresearch` subcommands

**Files:**
- Create: `crates/xvision-cli/src/commands/autoresearch.rs`
- Modify: CLI mod + dispatch.

- [ ] **Step 1: Implement**

Create `crates/xvision-cli/src/commands/autoresearch.rs`:

```rust
use clap::{Args, Subcommand};

use xvision_engine::api::autoresearch;

#[derive(Args, Debug)]
pub struct AutoresearchCmd {
    #[command(subcommand)]
    pub action: AutoresearchAction,
}

#[derive(Subcommand, Debug)]
pub enum AutoresearchAction {
    RunEveningCycle {
        #[arg(long)] strategy: Option<String>,
        #[arg(long)] dry_run: bool,
    },
    ListCycles { #[arg(long, default_value_t = 7)] since_days: u32 },
    ShowCycle  { cycle_id: String },
}

async fn ctx() -> anyhow::Result<std::sync::Arc<xvision_engine::api::ApiContext>> {
    let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    std::fs::create_dir_all(&xvn_home)?;
    let url = format!("sqlite://{}?mode=rwc", xvn_home.join("xvn.db").display());
    let db = sqlx::SqlitePool::connect(&url).await?;
    sqlx::migrate!("../xvision-engine/migrations").run(&db).await?;
    Ok(std::sync::Arc::new(xvision_engine::api::ApiContext::new(xvn_home, db)))
}

pub async fn run(cmd: AutoresearchCmd) -> anyhow::Result<()> {
    let ctx = ctx().await?;
    match cmd.action {
        AutoresearchAction::RunEveningCycle { strategy, dry_run } => {
            let r = autoresearch::run_evening_cycle(&ctx, autoresearch::EveningCycleOpts { agent_id: strategy, dry_run }).await;
            match r {
                Ok(rep) => println!("{}", serde_json::to_string_pretty(&rep)?),
                Err(e) => eprintln!("autoresearch not yet implemented: {e}"),
            }
        }
        AutoresearchAction::ListCycles { since_days } => {
            let since = chrono::Utc::now() - chrono::Duration::days(since_days as i64);
            let r = autoresearch::list_cycles(&ctx, since).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
        AutoresearchAction::ShowCycle { cycle_id } => {
            let r = autoresearch::show_cycle(&ctx, &cycle_id).await?;
            println!("{}", serde_json::to_string_pretty(&r)?);
        }
    }
    Ok(())
}
```

- [ ] **Step 2: Wire into CLI** (mod.rs + lib.rs).

- [ ] **Step 3: Smoke**

```bash
cargo run -p xvision-cli -- autoresearch list-cycles --since-days 7
```

Expected: empty array output (`[]`).

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/autoresearch.rs \
        crates/xvision-cli/src/commands/mod.rs \
        crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn autoresearch — run-evening-cycle, list-cycles, show-cycle (stubs until AR-2)"
```

---

### Task 25: End-to-end smoke (the spec example)

**Files:** test script only — no code changes.

- [ ] **Step 1: Author smoke script**

Create `scripts/smoke_xvn_scheduling.sh`:

```bash
#!/usr/bin/env bash
# End-to-end smoke for xvn scheduling foundation.
# Replicates the Section 11 example from the spec, end-to-end with a mock dispatch.
set -euo pipefail

export XVN_HOME=$(mktemp -d /tmp/xvn-e2e.XXXX)
echo "XVN_HOME=$XVN_HOME"

# 1. Create two strategies (manifest only — bundle proper would be `xvn strategy new`)
for sid in sh_X sh_Y sh_Z; do
    mkdir -p "$XVN_HOME/strategies/$sid"
    echo 'name="t"' > "$XVN_HOME/strategies/$sid/manifest.toml"
    cargo run -q -p xvision-cli -- strategy deactivate "$sid" --reason "smoke_init" >/dev/null
    cargo run -q -p xvision-cli -- strategy reactivate "$sid" >/dev/null
done

# 2. Create a schedule for nightly cull.
ID=$(cargo run -q -p xvision-cli -- schedule create \
  --name nightly-cull \
  --schedule "at 21:00 UTC" \
  --prompt "Review all Active strategies. Deactivate any with rolling-30d Sharpe < 0.5." \
  --allow "strategy.*,report.strategy_review,record_outcome" \
  --max-cost-usd 0.30 | awk '{print $2}')
echo "schedule id: $ID"

# 3. Mock the agent's behavior: call strategy.deactivate(sh_X, sh_Y) then record_outcome.
export XVN_MOCK_TURN_0='{"text":null,"tool_calls":[{"tool_call_id":"c1","name":"strategy.deactivate","arguments":{"id":"sh_X","reason":"Sharpe 0.32 < 0.5"}}],"stop_reason":"tool_use","tokens_in":50,"tokens_out":40,"cache_read_tokens":0,"cache_write_tokens":0}'
export XVN_MOCK_TURN_1='{"text":null,"tool_calls":[{"tool_call_id":"c2","name":"strategy.deactivate","arguments":{"id":"sh_Y","reason":"Sharpe 0.28 < 0.5"}}],"stop_reason":"tool_use","tokens_in":40,"tokens_out":30,"cache_read_tokens":0,"cache_write_tokens":0}'
export XVN_MOCK_TURN_2='{"text":"done","tool_calls":[{"tool_call_id":"c3","name":"record_outcome","arguments":{"summary":"Deactivated 2 of 3 strategies for low Sharpe","actions_taken":["strategy.deactivate sh_X","strategy.deactivate sh_Y"],"anomalies":[]}}],"stop_reason":"tool_use","tokens_in":20,"tokens_out":15,"cache_read_tokens":0,"cache_write_tokens":0}'

# 4. Trigger it manually + run daemon for ~3 seconds.
cargo run -q -p xvision-cli -- schedule run-now "$ID"
cargo run -q -p xvision-cli -- agent run --mock &
DAEMON=$!
sleep 4
kill "$DAEMON" 2>/dev/null || true
wait "$DAEMON" 2>/dev/null || true

# 5. Verify outcome.
cargo run -q -p xvision-cli -- schedule history --id "$ID"
echo "--- transcript ---"
FIRE_ID=$(sqlite3 "$XVN_HOME/xvn.db" "SELECT fire_id FROM schedule_fires WHERE schedule_id='$ID' ORDER BY started_at DESC LIMIT 1;")
cargo run -q -p xvision-cli -- schedule transcript "$FIRE_ID" | head -40

# 6. Verify strategy state.
cargo run -q -p xvision-cli -- strategy show sh_X
cargo run -q -p xvision-cli -- strategy show sh_Y
cargo run -q -p xvision-cli -- strategy show sh_Z

echo "smoke OK; XVN_HOME=$XVN_HOME"
```

- [ ] **Step 2: Run smoke script**

```bash
chmod +x scripts/smoke_xvn_scheduling.sh
bash scripts/smoke_xvn_scheduling.sh
```

Expected output highlights:
- schedule history shows one fire with `status=ok` and `summary="Deactivated 2 of 3..."`.
- transcript JSONL contains tool_call entries for `strategy.deactivate` and `record_outcome`.
- `strategy show sh_X` and `sh_Y` print `"status": "deactivated"`; `sh_Z` prints `"status": "active"`.

- [ ] **Step 3: Commit**

```bash
git add scripts/smoke_xvn_scheduling.sh
git commit -m "test: end-to-end smoke for xvn scheduling foundation"
```

---

### Task 26: Documentation — README + manual

**Files:**
- Modify: `crates/xvision-engine/README.md`
- Modify: `MANUAL.md` (or appropriate top-level docs file — check repo for the canonical user manual)

- [ ] **Step 1: Engine README**

Append to `crates/xvision-engine/README.md`:

```markdown
## Engine API (since 2026-05-10)

Typed action surface in `src/api/`:
- `strategy` — lifecycle (Active / Deactivated / Archived / Deleted)
- `risk` — per-deployment knobs (capital, stop-loss, position size, circuit breaker)
- `deploy` — deployment record CRUD + lifecycle events
- `report` — strategy review, deployment health, EOD, P&L, token spend, anomaly scan
- `maintenance` — log rotation, audit compaction, eval-cache refresh, vacuum, integrity check
- `schedule` — durable cron schedules that fire LLM-driven prompts
- `autoresearch` — AR-2 evening cycle hook (stub until AR-2 lands)

Every mutating function writes an audit row. CLI handlers + the agent runner's
tool registry both call these functions; one source of truth.

## Agent runner (since 2026-05-10)

`src/agent_runner/` is a generic tool-use loop usable for both scheduled jobs
and ad-hoc agent invocations. Pluggable via `xvision_intern::tool_dispatch::LlmToolDispatch`.

## Scheduler (since 2026-05-10)

`src/scheduler/` is a SQLite-backed cron daemon that fires schedules as
`AgentRunner` invocations. Friendly schedule expressions (`every 5m`, `at 21:00 UTC`,
`market-close`) supported via `scheduler::expr::parse`.

Default schedules ship pre-paused — `xvn schedule resume <id>` to enable:
- `eod-report` at NYSE market-close (DST-aware)
- `ar-evening-cycle` at 03:00 UTC
```

- [ ] **Step 2: User manual** — append a Scheduling section listing the CLI commands from Tasks 16–24. Use the spec's Section 3.2 as the source. Keep it short — link to the spec for full detail.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-engine/README.md MANUAL.md
git commit -m "docs: README + manual for engine API, agent runner, scheduler"
```

---

### Task 27: Workspace check

- [ ] **Step 1: Full test + lint pass**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

If any of these fail, fix the cause; do not bypass with `#[allow(...)]` or `--no-verify`. Recommit any fmt-induced changes.

- [ ] **Step 2: Commit any fmt/lint fixes**

```bash
git add -A
git commit -m "chore: fmt, clippy clean across workspace"
```

(If nothing changed, skip the commit.)

---

### Task 28: Self-review checklist

Run through the spec one last time. The plan's coverage map:

| Spec section | Covered by tasks |
|---|---|
| §2 Architecture overview | Tasks 1, 11, 14, 15 |
| §3.1 Friendly schedule expressions | Task 14 |
| §3.2 CLI surface | Tasks 13, 16, 17, 19–24 |
| §3.3 Dashboard surface | **Deferred** to follow-up plan extending Plan 2d |
| §3.4 Outcome reporting (`record_outcome`) | Task 11 (registered), Task 12 (loop enforces) |
| §4.1 strategy module | Task 2 |
| §4.2 risk module | Task 3 |
| §4.3 deploy module | Task 4 |
| §4.4 report module + EOD | Tasks 5, 6 |
| §4.5 maintenance module | Task 7 |
| §4.6 schedule module | Task 8 |
| §4.7 autoresearch module | Task 9 |
| §4.8 Tool naming | Task 11 |
| §5 Internal agent runner | Tasks 10, 11, 12 |
| §6 Durable events scheduler | Tasks 14, 15 |
| §7 Audit trails | Tasks 1, 2, 3, 4 |
| §7.2 Cost telemetry | Task 12 (recorded), Task 22 (`xvn report token-spend`) |
| §7.3 Anomaly heuristics | Task 5 (scaffold; live wiring follow-up) |
| §8 Non-custodial budget semantics | Task 3 (`risk.set_capital` mutates xvn-side only) |
| §9 Migration / relationships | This plan replaces 2c scheduler section, integrates with 2d via deferred follow-up |
| §10 Out-of-scope | Honored throughout |
| §10a Default schedules | Task 18 |
| §11 End-to-end example | Task 25 |

**Known limitations baked into v1:**
- No real `LlmToolDispatch` impl yet — `--mock` required on `xvn agent ask` and `xvn agent run`. Real Anthropic + OpenAI-compat impls are a follow-up plan (~1 task each).
- `report.strategy_review` doesn't yet read `scheduler_events` for decisions/PnL — returns active strategies only. Wires when 2c live daemon writes to `scheduler_events`.
- `deploy.start` / `stop` mark the record but don't (yet) supervise an actual daemon process. Process supervision lands when 2c's live daemon is wired to the system scheduler.
- Anomaly heuristics scaffolded but not wired against real data.

These limitations are intentional — they ship safely as stubs, and the plumbing they sit behind is in place for the follow-ups to complete.

---

## What's next

- **Follow-up: real LlmToolDispatch impl.** Two small tasks: Anthropic Messages API and OpenAI Chat Completions, each reusing existing `xvision-intern` plumbing.
- **Follow-up: dashboard /schedule routes.** Extends Plan 2d (`xvision-dashboard`). Adds list/detail/create routes + Live cockpit panel.
- **Follow-up: scheduler_events live data wiring.** Once Plan 2c daemon writes to `scheduler_events`, fill in `report.strategy_review` decisions/PnL and the anomaly heuristics.
- **Follow-up: AR-2 wires `autoresearch.run_evening_cycle`** to actual mutator + judge logic per AR-2 plan.
